use actix_web::{
    HttpRequest, http::header,
    HttpResponse, delete,
    dev::HttpServiceFactory,
    get,
    middleware::from_fn,
    patch, post, services,
    web::{self, Data, Json, Query},
};
use futures::future::try_join_all;
use log::warn;
use moonlight_common::PairPin;
use tokio::spawn;
use sha2::{Digest, Sha256};

use crate::{
    api::{
        admin::{add_user, delete_user, list_users, patch_user},
        auth::auth_middleware,
        response_streaming::StreamedResponse,
    },
    app::{
        App, AppError,
        host::{AppId, HostId},
        storage::StorageHostModify,
        user::{AuthenticatedUser, Role, UserId},
    },
};
use common::api_bindings::{
    self, DeleteHostQuery, DetailedUser, GetAppImageQuery, GetAppsQuery, GetAppsResponse,
    GetHostQuery, GetHostResponse, GetHostsResponse, GetUserQuery, PatchHostRequest,
    PostHostRequest, PostHostResponse, PostPairRequest, PostPairResponse1, PostPairResponse2,
    PostWakeUpRequest, UndetailedHost,
};

pub mod admin;
pub mod auth;
pub mod stream;

pub mod response_streaming;

#[get("/user")]
async fn get_user(
    app: Data<App>,
    mut user: AuthenticatedUser,
    Query(query): Query<GetUserQuery>,
) -> Result<Json<DetailedUser>, AppError> {
    match (query.name, query.user_id) {
        (None, None) => {
            let detailed_user = user.detailed_user().await?;

            Ok(Json(detailed_user))
        }
        (None, Some(user_id)) => {
            let target_user_id = UserId(user_id);

            let mut target_user = app.user_by_id(target_user_id).await?;

            let detailed_user = target_user.detailed_user(&mut user).await?;

            Ok(Json(detailed_user))
        }
        (Some(name), None) => {
            let mut target_user = app.user_by_name(&name).await?;

            let detailed_user = target_user.detailed_user(&mut user).await?;

            Ok(Json(detailed_user))
        }
        (Some(_), Some(_)) => Err(AppError::BadRequest),
    }
}

#[get("/hosts")]
async fn list_hosts(
    mut user: AuthenticatedUser,
) -> Result<StreamedResponse<GetHostsResponse, UndetailedHost>, AppError> {
    let (mut stream_response, stream_sender) =
        StreamedResponse::new(GetHostsResponse { hosts: Vec::new() });

    let hosts = user.hosts().await?;

    // Try join all because storage should always work, the actual host info will be send using response streaming
    let undetailed_hosts = try_join_all(hosts.into_iter().map(move |mut host| {
        let mut user = user.clone();
        let stream_sender = stream_sender.clone();

        async move {
            // First query db
            let undetailed_cache = host.undetailed_host_cached(&mut user).await;

            // Then send http request now
            let mut user = user.clone();

            spawn(async move {
                let undetailed = match host.undetailed_host(&mut user).await {
                    Ok(value) => value,
                    Err(err) => {
                        warn!("Failed to get undetailed host of {host:?}: {err}");
                        return;
                    }
                };

                if let Err(err) = stream_sender.send(undetailed).await {
                    warn!(
                        "Failed to send back undetailed host data using response streaming: {err}"
                    );
                }
            });

            undetailed_cache
        }
    }))
    .await?;

    stream_response.set_initial(GetHostsResponse {
        hosts: undetailed_hosts,
    });

    Ok(stream_response)
}

#[get("/host")]
async fn get_host(
    mut user: AuthenticatedUser,
    Query(query): Query<GetHostQuery>,
) -> Result<Json<GetHostResponse>, AppError> {
    let host_id = HostId(query.host_id);

    let mut host = user.host(host_id).await?;

    let detailed = host.detailed_host(&mut user).await?;

    Ok(Json(GetHostResponse { host: detailed }))
}

#[post("/host")]
async fn post_host(
    app: Data<App>,
    mut user: AuthenticatedUser,
    Json(request): Json<PostHostRequest>,
) -> Result<Json<PostHostResponse>, AppError> {
    let mut host = user
        .host_add(
            request.address,
            request
                .http_port
                .unwrap_or(app.config().moonlight.default_http_port),
        )
        .await?;

    Ok(Json(PostHostResponse {
        host: host.detailed_host(&mut user).await?,
    }))
}

#[patch("/host")]
async fn patch_host(
    mut user: AuthenticatedUser,
    Json(request): Json<PatchHostRequest>,
) -> Result<HttpResponse, AppError> {
    let host_id = HostId(request.host_id);

    let mut host = user.host(host_id).await?;

    let mut modify = StorageHostModify::default();

    let role = user.role().await?;
    if request.change_owner {
        match role {
            Role::Admin => {
                modify.owner = Some(request.owner.map(UserId));
            }
            Role::User => {
                return Err(AppError::Forbidden);
            }
        }
    }

    host.modify(&mut user, modify).await?;

    Ok(HttpResponse::Ok().finish())
}

#[delete("/host")]
async fn delete_host(
    mut user: AuthenticatedUser,
    Query(query): Query<DeleteHostQuery>,
) -> Result<HttpResponse, AppError> {
    let host_id = HostId(query.host_id);

    user.host_delete(host_id).await?;

    Ok(HttpResponse::Ok().finish())
}

#[post("/pair")]
async fn pair_host(
    mut user: AuthenticatedUser,
    Json(request): Json<PostPairRequest>,
) -> Result<StreamedResponse<PostPairResponse1, PostPairResponse2>, AppError> {
    let host_id = HostId(request.host_id);

    let mut host = user.host(host_id).await?;

    let pin = PairPin::generate()?;

    let (stream_response, stream_sender) =
        StreamedResponse::new(PostPairResponse1::Pin(pin.to_string()));

    spawn(async move {
        let result = host.pair(&mut user, pin).await;

        let result = match result {
            Ok(()) => host.detailed_host(&mut user).await,
            Err(err) => Err(err),
        };

        match result {
            Ok(detailed_host) => {
                if let Err(err) = stream_sender
                    .send(PostPairResponse2::Paired(detailed_host))
                    .await
                {
                    warn!("Failed to send pair success: {err}");
                }
            }
            Err(err) => {
                warn!("Failed to pair host: {err}");
                if let Err(err) = stream_sender.send(PostPairResponse2::PairError).await {
                    warn!("Failed to send pair failure: {err}");
                }
            }
        }
    });

    Ok(stream_response)
}

#[post("/host/wake")]
async fn wake_host(
    mut user: AuthenticatedUser,
    Json(request): Json<PostWakeUpRequest>,
) -> Result<HttpResponse, AppError> {
    let host_id = HostId(request.host_id);

    let host = user.host(host_id).await?;

    host.wake(&mut user).await?;

    Ok(HttpResponse::Ok().finish())
}

#[get("/apps")]
async fn get_apps(
    mut user: AuthenticatedUser,
    Query(query): Query<GetAppsQuery>,
) -> Result<Json<GetAppsResponse>, AppError> {
    let host_id = HostId(query.host_id);

    let mut host = user.host(host_id).await?;

    let apps = host.list_apps(&mut user).await?;

    Ok(Json(GetAppsResponse {
        apps: apps
            .into_iter()
            .map(|app| api_bindings::App {
                app_id: app.id.0,
                title: app.title,
                is_hdr_supported: app.is_hdr_supported,
            })
            .collect(),
    }))
}

#[get("/app/image")]
async fn get_app_image(
    mut user: AuthenticatedUser,
    Query(query): Query<GetAppImageQuery>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let host_id = HostId(query.host_id);
    let app_id = AppId(query.app_id);

    let mut host = user.host(host_id).await?;

    let image = host
        .app_image(&mut user, app_id, query.force_refresh)
        .await?;

    let mut hasher = Sha256::new();
    hasher.update(&image);
    let etag = format!("\"{:x}\"", hasher.finalize());

    let cache_control = "private, no-cache, must-revalidate";

    if let Some(if_none_match) = req.headers().get(header::IF_NONE_MATCH) {
        if if_none_match.to_str().ok() == Some(&etag) && query.force_refresh == false {
            return Ok(
                HttpResponse::NotModified()
                    .insert_header((header::ETAG, etag))
                    .insert_header((header::CACHE_CONTROL, cache_control))
                    .finish()
            );
        }
    }

    Ok(
        HttpResponse::Ok()
            .insert_header((header::ETAG, etag))
            .insert_header((header::CACHE_CONTROL, cache_control))
            .body(image)
    )
}

pub fn api_service() -> impl HttpServiceFactory {
    web::scope("/api")
        .wrap(from_fn(auth_middleware))
        .service(services![
            // -- Auth
            auth::login,
            auth::logout,
            auth::authenticate
        ])
        .service(services![
            // -- Host
            get_user,
            list_hosts,
            get_host,
            post_host,
            patch_host,
            wake_host,
            delete_host,
            pair_host,
            get_apps,
            get_app_image,
        ])
        .service(services![
            // -- Stream
            stream::start_host,
            stream::cancel_host,
        ])
        .service(services![
            // -- Admin
            add_user,
            patch_user,
            delete_user,
            list_users
        ])
}
