use actix_web::{
    HttpResponse, delete, get, patch, post,
    web::{Data, Json, Query},
};
use common::api_bindings::{
    DeleteRoleQuery, GetRoleQuery, GetRoleResponse, GetRolesResponse, PatchRoleRequest,
    PostRoleRequest, PostRoleResponse, StreamPermissions, StreamSettings,
};

use futures::future::join_all;
use tracing::warn;

use crate::app::{
    App, AppError,
    role::RoleId,
    storage::{
        StorageRoleAdd, StorageRoleDefaultSettings, StorageRoleModify, StorageRolePermissions,
    },
    user::{Admin, RoleType},
};

fn convert_settings(settings: StreamSettings) -> StorageRoleDefaultSettings {
    StorageRoleDefaultSettings {}
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
    let mut role = app
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

    Ok(Json(PostRoleResponse {
        role: role.detailed_role().await?,
    }))
}

#[get("/role")]
pub async fn get_role(
    app: Data<App>,
    _admin: Admin,
    Query(query): Query<GetRoleQuery>,
) -> Result<Json<GetRoleResponse>, AppError> {
    let role_id = RoleId(query.id);

    let mut role = app.role_by_id(role_id).await?;

    Ok(Json(GetRoleResponse {
        role: role.detailed_role().await?,
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

    role.modify(
        &admin,
        StorageRoleModify {
            name: request.name,
            ty: None,
            permissions: request.permissions.map(|x| StorageRolePermissions {}),
            default_settings: request
                .default_settings
                .map(|x| StorageRoleDefaultSettings {}),
        },
    )
    .await?;

    Ok(HttpResponse::Ok().finish())
}

#[delete("/role")]
pub async fn delete_role(
    app: Data<App>,
    admin: Admin,
    Query(query): Query<DeleteRoleQuery>,
) -> Result<HttpResponse, AppError> {
    let role_id = RoleId(query.id);

    let role = app.role_by_id(role_id).await?;

    role.delete(&admin).await?;

    Ok(HttpResponse::Ok().finish())
}

#[get("/roles")]
pub async fn list_roles(app: Data<App>, admin: Admin) -> Result<Json<GetRolesResponse>, AppError> {
    let mut roles = app.all_roles().await?;

    let role_results = join_all(roles.iter_mut().map(|role| role.undetailed_role())).await;

    let mut out_roles = Vec::with_capacity(role_results.len());
    for (result, role) in role_results.into_iter().zip(roles) {
        match result {
            Ok(role) => {
                out_roles.push(role);
            }
            Err(err) => {
                warn!("Failed to query detailed role of {role:?}: {err}");
            }
        }
    }

    Ok(Json(GetRolesResponse { roles: out_roles }))
}
