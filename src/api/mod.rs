use actix_web::{dev::HttpServiceFactory, services, web};

use crate::api::{
    app::{get_app_image, get_apps},
    host::{
        cancel_host, delete_host, get_host, list_hosts, pair_host, patch_host, post_host, wake_host,
    },
    stream::{
        web_socket::web_socket_stream,
        webrtc::{webrtc_delete, webrtc_get, webrtc_options, webrtc_patch, webrtc_post},
    },
};

pub mod app;
pub mod bindings;
pub(super) mod bindings_ext;
pub mod host;
pub mod response_streaming;
pub mod stream;

pub fn api_service() -> impl HttpServiceFactory {
    web::scope("/api")
        .service(services![
            list_hosts,
            get_host,
            post_host,
            patch_host,
            wake_host,
            delete_host,
            pair_host,
            cancel_host,
            get_apps,
            get_app_image,
            web_socket_stream,
        ])
        .service(web::scope("/host/stream/webrtc").service(services![
            webrtc_options,
            webrtc_get,
            webrtc_post,
            webrtc_patch,
            webrtc_delete,
        ]))
}
