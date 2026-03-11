use actix_web::{
    dev::HttpServiceFactory,
    middleware::from_fn, services,
    web::{self},
};

use crate::api::{
        app::{get_app_image, get_apps},
        auth::auth_middleware,
        host::{delete_host, get_host, pair_host, patch_host, post_host, wake_host},
        user::{add_user, get_user, list_users, patch_user},
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
