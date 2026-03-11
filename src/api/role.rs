use actix_web::{
    HttpResponse, delete, get, patch, post,
    web::{Data, Json, Query},
};
use common::api_bindings::{
    DeleteRoleQuery, GetRoleQuery, GetRoleResponse, PatchRoleRequest, PostRoleRequest,
    PostRoleResponse, StreamPermissions, StreamSettings,
};

use crate::app::{
    App, AppError,
    role::RoleId,
    storage::{StorageRoleAdd, StorageRolePermissions, StorageRoleSettings},
    user::{Admin, RoleType},
};

fn convert_settings(settings: StreamSettings) -> StorageRoleSettings {
    StorageRoleSettings {}
}
fn convert_permissions(permissions: StreamPermissions) -> StorageRolePermissions {
    StorageRolePermissions {}
}

#[post("/role")]
pub async fn add_role(
    app: Data<App>,
    admin: Admin,
    Json(request): Json<PostRoleRequest>,
) -> Result<Json<PostRoleResponse>, AppError> {
    let role = app
        .add_role(
            &admin,
            StorageRoleAdd {
                name: request.name,
                ty: RoleType::User,
                default_settings: convert_settings(request.default_settings),
                permissions: convert_permissions(request.permissions),
            },
        )
        .await?;

    Ok(Json(PostRoleResponse { id: role.id().0 }))
}

#[get("/role")]
pub async fn get_role(
    app: Data<App>,
    _admin: Admin,
    Query(query): Query<GetRoleQuery>,
) -> Result<Json<GetRoleResponse>, AppError> {
    let role_id = RoleId(query.id);

    let role = app.role_by_id(role_id).await?;

    Ok(Json(GetRoleResponse {
        default_settings: role.default_settings().await?,
        permissions: role.permissions().await?,
    }))
}

#[patch("/role")]
pub async fn patch_role(
    app: Data<App>,
    admin: Admin,
    Json(request): Json<PatchRoleRequest>,
) -> Result<HttpResponse, AppError> {
    let role_id = RoleId(request.id);

    let role = app.role_by_id(role_id).await?;

    Ok(HttpResponse::Ok())
}

#[delete("/role")]
pub async fn remove_role(
    app: Data<App>,
    admin: Admin,
    Query(query): Query<DeleteRoleQuery>,
) -> Result<HttpResponse, AppError> {
    let role_id = RoleId(query.id);

    let role = app.role_by_id(role_id).await?;

    role.delete(&admin).await?;

    Ok(HttpResponse::Ok().finish())
}
