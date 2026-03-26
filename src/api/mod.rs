use actix_web::{
    dev::HttpServiceFactory,
    middleware::from_fn,
    services,
    web::{self},
};

use crate::api::{
    app::{get_app_image, get_apps},
    auth::auth_middleware,
    host::{delete_host, get_host, list_hosts, pair_host, patch_host, post_host, wake_host},
    role::{add_role, delete_role, get_role, list_roles, patch_role},
    settings::{get_default_settings, get_permissions},
    user::{add_user, delete_user, get_user, list_users, patch_user},
};

pub mod app;
pub mod auth;
pub mod host;
pub mod role;
pub mod settings;
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
            list_hosts,
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
            get_user,
            add_user,
            patch_user,
            delete_user,
            list_users,
        ])
        .service(services![
            // -- Roles
            // TODO: delete role?
            get_role,
            add_role,
            patch_role,
            delete_role,
            list_roles,
        ])
        .service(services![
            // -- Settings
            get_default_settings,
            get_permissions
        ])
        .service(services![
            // -- Stream
            stream::start_host,
            stream::cancel_host,
        ])
}
