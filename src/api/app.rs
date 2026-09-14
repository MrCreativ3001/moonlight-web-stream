use actix_web::{
    HttpRequest, HttpResponse, get,
    http::header,
    web::{Data, Query},
};
use sha2::{Digest, Sha256};

use crate::{
    api::bindings::{self, GetAppImageQuery, GetAppsQuery, GetAppsResponse},
    app::{
        App, AppError,
        host::{AppId, HostId},
    },
};

#[get("/apps")]
async fn get_apps(
    app: Data<App>,
    Query(query): Query<GetAppsQuery>,
) -> Result<actix_web::web::Json<GetAppsResponse>, AppError> {
    let apps = app.host(HostId(query.host_id)).await?.list_apps().await?;
    Ok(actix_web::web::Json(GetAppsResponse {
        apps: apps.into_iter().map(bindings::App::from).collect(),
    }))
}

#[get("/app/image")]
async fn get_app_image(
    app: Data<App>,
    Query(query): Query<GetAppImageQuery>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let image = app
        .host(HostId(query.host_id))
        .await?
        .app_image(AppId(query.app_id), query.force_refresh)
        .await?;
    let mut hasher = Sha256::new();
    hasher.update(&image);
    let etag = format!("\"{:x}\"", hasher.finalize());
    if let Some(value) = req.headers().get(header::IF_NONE_MATCH)
        && value.to_str().ok() == Some(&etag)
        && !query.force_refresh
    {
        return Ok(HttpResponse::NotModified()
            .insert_header((header::ETAG, etag))
            .finish());
    }
    Ok(HttpResponse::Ok()
        .insert_header((header::ETAG, etag))
        .insert_header((header::CACHE_CONTROL, "private, no-cache, must-revalidate"))
        .body(image))
}
