use actix_web::{
    HttpResponse, delete, get, patch, post,
    rt::spawn,
    web::{Data, Json, Query},
};
use common::api_bindings::{
    DeleteHostQuery, GetHostQuery, GetHostResponse, GetHostsResponse, PatchHostRequest,
    PostHostRequest, PostHostResponse, PostPairRequest, PostPairResponse1, PostPairResponse2,
    PostWakeUpRequest, UndetailedHost,
};
use futures::future::try_join_all;
use moonlight_common::{crypto::openssl::OpenSSLCryptoBackend, http::pair::PairPin};
use tracing::warn;

use crate::{
    api::response_streaming::StreamedResponse,
    app::{
        App, AppError,
        host::HostId,
        storage::StorageHostModify,
        user::{AuthenticatedUser, RoleType, UserId},
    },
};

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

    let mut role = user.role().await?;
    if request.change_owner {
        match role.ty().await? {
            RoleType::Admin => {
                modify.owner = Some(request.owner.map(UserId));
            }
            RoleType::User => {
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

    let pin = PairPin::new_random(&OpenSSLCryptoBackend)?;

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
