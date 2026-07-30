use actix_web::{
    HttpResponse, delete, get, patch, post,
    rt::spawn,
    web::{Data, Json, Query},
};
use common::api_bindings::{
    DeleteHostQuery, GetHostQuery, GetHostResponse, GetHostsResponse, PairFailReason,
    PatchHostRequest, PostHostRequest, PostHostResponse, PostPairCancelRequest, PostPairRequest,
    PostPairResponse1, PostPairResponse2, PostWakeUpRequest, UndetailedHost,
};
use futures::future::try_join_all;
use moonlight_common::{
    crypto::openssl::OpenSSLCryptoBackend, high::MoonlightClientError, http::pair::PairPin,
};
use tracing::warn;

use crate::{
    api::response_streaming::StreamedResponse,
    app::{
        App, AppError,
        host::{Host, HostId},
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

/// Maps a pairing failure onto the reason communicated to the client, plus a
/// human-readable detail string.
fn pair_fail_reason(err: &AppError) -> (PairFailReason, Option<String>) {
    use moonlight_common::http::pair::client::ClientPairingError;

    let reason = match err {
        AppError::PairingInProgress => PairFailReason::PairingInProgress,
        AppError::PairingTimedOut => PairFailReason::TimedOut,
        AppError::PairingCancelled => PairFailReason::Cancelled,
        AppError::HostPaired => PairFailReason::AlreadyPaired,
        AppError::HostNotFound => PairFailReason::HostUnreachable,
        AppError::Moonlight(err) => match err {
            MoonlightClientError::Pairing(ClientPairingError::FailedWrongPin) => {
                PairFailReason::PinIncorrect
            }
            MoonlightClientError::Pairing(ClientPairingError::FailedAlreadyInProgress) => {
                PairFailReason::PairingInProgress
            }
            MoonlightClientError::Offline | MoonlightClientError::Backend(_) => {
                PairFailReason::HostUnreachable
            }
            _ => PairFailReason::Internal,
        },
        _ => PairFailReason::Internal,
    };

    (reason, Some(err.to_string()))
}

#[post("/pair")]
async fn pair_host(
    mut user: AuthenticatedUser,
    Json(request): Json<PostPairRequest>,
) -> Result<StreamedResponse<PostPairResponse1, PostPairResponse2>, AppError> {
    let host_id = HostId(request.host_id);

    let mut host = user.host(host_id).await?;

    // Advisory pre-check so a duplicate attempt is rejected before the client
    // ever sees a pin; `Host::pair` still guards atomically against races.
    if host.pair_in_progress()? {
        let (reason, detail) = pair_fail_reason(&AppError::PairingInProgress);
        return Ok(StreamedResponse::new(PostPairResponse1::PairFailed {
            reason,
            detail,
        })
        .0);
    }

    let pin = PairPin::new_random(&OpenSSLCryptoBackend)?;

    let (stream_response, stream_sender) = StreamedResponse::new(PostPairResponse1::Pin {
        pin: pin.to_string(),
        expires_in_secs: Host::PAIR_TIMEOUT_SECS,
    });

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
                let (reason, detail) = pair_fail_reason(&err);
                if let Err(err) = stream_sender
                    .send(PostPairResponse2::PairFailed { reason, detail })
                    .await
                {
                    warn!("Failed to send pair failure: {err}");
                }
            }
        }
    });

    Ok(stream_response)
}

#[post("/pair/cancel")]
async fn pair_cancel_host(
    mut user: AuthenticatedUser,
    Json(request): Json<PostPairCancelRequest>,
) -> Result<HttpResponse, AppError> {
    let host_id = HostId(request.host_id);

    let host = user.host(host_id).await?;

    host.pair_cancel()?;

    Ok(HttpResponse::Ok().finish())
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

#[cfg(test)]
mod tests {
    use super::*;
    use moonlight_common::http::pair::client::ClientPairingError;

    fn reason_of(err: AppError) -> PairFailReason {
        pair_fail_reason(&err).0
    }

    #[test]
    fn pairing_lifecycle_errors_map_to_their_reason() {
        assert_eq!(
            reason_of(AppError::PairingInProgress),
            PairFailReason::PairingInProgress
        );
        assert_eq!(
            reason_of(AppError::PairingTimedOut),
            PairFailReason::TimedOut
        );
        assert_eq!(
            reason_of(AppError::PairingCancelled),
            PairFailReason::Cancelled
        );
        assert_eq!(reason_of(AppError::HostPaired), PairFailReason::AlreadyPaired);
    }

    #[test]
    fn moonlight_pairing_errors_map_to_their_reason() {
        assert_eq!(
            reason_of(AppError::Moonlight(MoonlightClientError::Pairing(
                ClientPairingError::FailedWrongPin
            ))),
            PairFailReason::PinIncorrect
        );
        assert_eq!(
            reason_of(AppError::Moonlight(MoonlightClientError::Pairing(
                ClientPairingError::FailedAlreadyInProgress
            ))),
            PairFailReason::PairingInProgress
        );
        assert_eq!(
            reason_of(AppError::Moonlight(MoonlightClientError::Offline)),
            PairFailReason::HostUnreachable
        );
    }

    #[test]
    fn unknown_errors_fall_back_to_internal_with_detail() {
        let (reason, detail) = pair_fail_reason(&AppError::Forbidden);
        assert_eq!(reason, PairFailReason::Internal);
        assert!(detail.is_some());
    }
}
