use actix_web::{get, web::Json};
use common::api_bindings::{StreamPermissions, StreamSettings};

use crate::app::{AppError, user::AuthenticatedUser};

#[get("/settings/default")]
pub async fn get_default_settings(
    mut user: AuthenticatedUser,
) -> Result<Json<StreamSettings>, AppError> {
    let mut role = user.role().await?;

    let default_settings = role.default_settings().await?;

    Ok(Json(default_settings))
}

#[get("/settings/permissions")]
pub async fn get_permissions(
    mut user: AuthenticatedUser,
) -> Result<Json<StreamPermissions>, AppError> {
    let mut role = user.role().await?;

    let permissions = role.permissions().await?;

    Ok(Json(permissions))
}
