use actix_web::{
    HttpRequest, HttpResponse, get,
    http::header,
    web::{Json, Query},
};
use common::api_bindings::{self, GetAppImageQuery, GetAppsQuery, GetAppsResponse};
use sha2::{Digest, Sha256};

use crate::app::{
    AppError,
    host::{AppId, HostId},
    user::AuthenticatedUser,
};

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

    if let Some(if_none_match) = req.headers().get(header::IF_NONE_MATCH)
        && if_none_match.to_str().ok() == Some(&etag)
        && !query.force_refresh
    {
        return Ok(HttpResponse::NotModified()
            .insert_header((header::ETAG, etag))
            .finish());
    }

    Ok(HttpResponse::Ok()
        .insert_header((header::ETAG, etag))
        .insert_header((header::CACHE_CONTROL, cache_control))
        .body(image))
}
