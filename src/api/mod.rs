use actix_web::{
    HttpRequest, HttpResponse, delete,
    dev::HttpServiceFactory,
    get,
    http::header,
    middleware::from_fn,
    patch, post, services,
    web::{self, Data, Json, Query},
};
use futures::future::try_join_all;
use log::warn;
use moonlight_common::{crypto::openssl::OpenSSLCryptoBackend, http::pair::PairPin};
use sha2::{Digest, Sha256};
use tokio::spawn;

use crate::{
    api::{
        app::{get_app_image, get_apps},
        auth::auth_middleware,
        host::{delete_host, get_host, pair_host, patch_host, post_host, wake_host},
        response_streaming::StreamedResponse,
        user::{add_user, get_user, list_users, patch_user},
    },
    app::{
        App, AppError,
        host::{AppId, HostId},
        storage::StorageHostModify,
        user::{AuthenticatedUser, RoleType, UserId},
    },
};
use common::api_bindings::{
    self, DeleteHostQuery, DetailedUser, GetAppImageQuery, GetAppsQuery, GetAppsResponse,
    GetHostQuery, GetHostResponse, GetHostsResponse, GetUserQuery, PatchHostRequest,
    PostHostRequest, PostHostResponse, PostPairRequest, PostPairResponse1, PostPairResponse2,
    PostWakeUpRequest, UndetailedHost,
};

pub mod app;
pub mod auth;
pub mod host;
pub mod role;
pub mod stream;
pub mod user;

pub mod response_streaming;

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
            get_host,
            post_host,
            patch_host,
            wake_host,
            delete_host,
            pair_host,
        ])
        .service(services![
            // -- Apps
            get_apps,
            get_app_image,
        ])
        .service(services![
            // -- Users
            get_user, add_user, patch_user, list_users,
        ])
        .service(services![
            // -- Stream
            stream::start_host,
            stream::cancel_host,
        ])
}
