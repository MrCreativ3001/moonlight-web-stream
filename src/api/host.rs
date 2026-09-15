use actix_web::{
    HttpResponse, delete, get, patch, post,
    rt::spawn,
    web::{Data, Json, Query},
};
use futures::future::try_join_all;
use moonlight_common::{crypto::rustcrypto::RustCryptoBackend, http::pair::PairPin};
use tracing::warn;

use crate::{
    api::{
        bindings::{
            DeleteHostQuery, GetHostQuery, GetHostResponse, GetHostsResponse, PatchHostRequest,
            PostCancelRequest, PostCancelResponse, PostHostRequest, PostHostResponse,
            PostPairRequest, PostPairResponse1, PostPairResponse2, PostWakeUpRequest,
            UndetailedHost,
        },
        response_streaming::StreamedResponse,
    },
    app::{App, AppError, host::HostId, storage::StorageHostModify},
};

#[get("/hosts")]
async fn list_hosts(
    app: Data<App>,
) -> Result<StreamedResponse<GetHostsResponse, UndetailedHost>, AppError> {
    let (mut response, sender) = StreamedResponse::new(GetHostsResponse { hosts: Vec::new() });
    let hosts = app.hosts().await?;
    let initial = try_join_all(hosts.into_iter().map(|mut host| {
        let sender = sender.clone();
        async move {
            let cached = host.undetailed_host_cached().await?;
            spawn(async move {
                match host.undetailed_host().await {
                    Ok(value) => {
                        let _ = sender.send(value).await;
                    }
                    Err(err) => warn!("Failed to get host data: {err}"),
                }
            });
            Ok::<_, AppError>(cached)
        }
    }))
    .await?;
    response.set_initial(GetHostsResponse { hosts: initial });
    Ok(response)
}

#[get("/host")]
async fn get_host(
    app: Data<App>,
    Query(query): Query<GetHostQuery>,
) -> Result<Json<GetHostResponse>, AppError> {
    let mut host = app.host(HostId(query.host_id)).await?;
    Ok(Json(GetHostResponse {
        host: host.detailed_host().await?,
    }))
}

#[post("/host")]
async fn post_host(
    app: Data<App>,
    Json(request): Json<PostHostRequest>,
) -> Result<Json<PostHostResponse>, AppError> {
    let mut host = app
        .host_add(
            request.address,
            request
                .http_port
                .unwrap_or(app.config().moonlight.default_http_port),
        )
        .await?;
    Ok(Json(PostHostResponse {
        host: host.detailed_host().await?,
    }))
}

#[patch("/host")]
async fn patch_host(
    app: Data<App>,
    Json(request): Json<PatchHostRequest>,
) -> Result<HttpResponse, AppError> {
    let mut host = app.host(HostId(request.host_id)).await?;
    host.modify(StorageHostModify {
        address: request.address,
        http_port: request.http_port,
        ..Default::default()
    })
    .await?;
    Ok(HttpResponse::Ok().finish())
}

#[delete("/host")]
async fn delete_host(
    app: Data<App>,
    Query(query): Query<DeleteHostQuery>,
) -> Result<HttpResponse, AppError> {
    app.host_delete(HostId(query.host_id)).await?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/pair")]
async fn pair_host(
    app: Data<App>,
    Json(request): Json<PostPairRequest>,
) -> Result<StreamedResponse<PostPairResponse1, PostPairResponse2>, AppError> {
    let mut host = app.host(HostId(request.host_id)).await?;
    let pin = PairPin::new_random(&RustCryptoBackend)?;
    let (response, sender) = StreamedResponse::new(PostPairResponse1::Pin(pin.to_string()));
    spawn(async move {
        let result = match host.pair(pin).await {
            Ok(()) => host.detailed_host().await,
            Err(err) => Err(err),
        };
        let message = result
            .map(PostPairResponse2::Paired)
            .unwrap_or(PostPairResponse2::PairError);
        let _ = sender.send(message).await;
    });
    Ok(response)
}

#[post("/host/wake")]
async fn wake_host(
    app: Data<App>,
    Json(request): Json<PostWakeUpRequest>,
) -> Result<HttpResponse, AppError> {
    app.host(HostId(request.host_id)).await?.wake().await?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/host/cancel")]
pub async fn cancel_host(
    app: Data<App>,
    Json(request): Json<PostCancelRequest>,
) -> Result<Json<PostCancelResponse>, AppError> {
    app.host(HostId(request.host_id))
        .await?
        .cancel_app()
        .await?;
    Ok(Json(PostCancelResponse { success: true }))
}
