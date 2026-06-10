use actix_web::{
    dev::{HttpServiceFactory, Service, ServiceResponse},
    http::header::{self, HeaderValue},
    middleware::from_fn,
    services,
    web::{self, Header},
};

use crate::api::{
    app::{get_app_image, get_apps},
    auth::auth_middleware,
    host::{
        cancel_host, delete_host, get_host, list_hosts, pair_host, patch_host, post_host, wake_host,
    },
    role::{add_role, delete_role, get_role, list_roles, patch_role},
    settings::{get_default_settings, get_permissions},
    stream::whep::{whep_delete, whep_get, whep_options, whep_patch, whep_post},
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
            cancel_host,
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
        .service(
            // -- Stream
            web::scope("/host/stream/whep")
                .wrap_fn(|req, service| {
                    let fut = service.call(req);
                    async {
                        let mut res = fut.await?;

                        // Allow access for other browser based services to call the whep apis
                        let headers = res.response_mut().headers_mut();
                        headers.append(
                            header::ACCESS_CONTROL_ALLOW_ORIGIN,
                            HeaderValue::from_static("*"),
                        );

                        Ok(res)
                    }
                })
                .service(whep_options)
                .service(whep_get)
                .service(whep_post)
                .service(whep_patch)
                .service(whep_delete),
        )
}
