use actix_web::{
    HttpResponse, delete, get, patch, post,
    web::{Data, Json, Query},
};
use common::api_bindings::{
    DeleteUserRequest, DetailedUser, GetUserQuery, GetUsersResponse, PatchUserRequest,
    PostUserRequest,
};
use futures::future::join_all;
use tracing::warn;

use crate::app::{
    App, AppError,
    password::StoragePassword,
    role::RoleId,
    storage::{StorageUserAdd, StorageUserModify},
    user::{Admin, AuthenticatedUser, UserId},
};

#[get("/user")]
async fn get_user(
    app: Data<App>,
    mut user: AuthenticatedUser,
    Query(query): Query<GetUserQuery>,
) -> Result<Json<DetailedUser>, AppError> {
    match (query.name, query.user_id) {
        (None, None) => {
            let detailed_user = user.detailed_user().await?;

            Ok(Json(detailed_user))
        }
        (None, Some(user_id)) => {
            let target_user_id = UserId(user_id);

            let mut target_user = app.user_by_id(target_user_id).await?;

            let detailed_user = target_user.detailed_user(&mut user).await?;

            Ok(Json(detailed_user))
        }
        (Some(name), None) => {
            let mut target_user = app.user_by_name(&name).await?;

            let detailed_user = target_user.detailed_user(&mut user).await?;

            Ok(Json(detailed_user))
        }
        (Some(_), Some(_)) => Err(AppError::BadRequest),
    }
}

#[post("/user")]
pub async fn add_user(
    app: Data<App>,
    admin: Admin,
    Json(request): Json<PostUserRequest>,
) -> Result<Json<DetailedUser>, AppError> {
    let mut user = app
        .add_user(
            &admin,
            StorageUserAdd {
                name: request.name.clone(),
                password: Some(StoragePassword::new(&request.password)?),
                role_id: RoleId(request.role_id),
                client_unique_id: request.client_unique_id,
            },
        )
        .await?;

    let detailed_user = user.detailed_user().await?;

    Ok(Json(detailed_user))
}

#[patch("/user")]
pub async fn patch_user(
    app: Data<App>,
    user: AuthenticatedUser,
    Json(request): Json<PatchUserRequest>,
) -> Result<HttpResponse, AppError> {
    let target_user_id = UserId(request.id);

    match Admin::try_from(user).await? {
        Ok(admin) => {
            let mut target_user = app.user_by_id(target_user_id).await?;

            let new_password = if let Some(new_password) = request.password {
                Some(StoragePassword::new(&new_password)?)
            } else {
                None
            };

            target_user
                .modify(
                    &admin,
                    StorageUserModify {
                        password: new_password.map(Some),
                        role_id: request.role_id.map(RoleId),
                        client_unique_id: request.client_unique_id,
                    },
                )
                .await?;
        }
        Err(mut user) => {
            if user.id() != target_user_id {
                return Err(AppError::Forbidden);
            }

            // Only allow changing the password
            let PatchUserRequest {
                id: _,
                password: _,
                role_id,
                client_unique_id,
            } = &request;
            if role_id.is_some() || client_unique_id.is_some() {
                return Err(AppError::Forbidden);
            }

            if let Some(new_password) = request.password {
                user.set_password(StoragePassword::new(&new_password)?)
                    .await?;
            }
        }
    }

    Ok(HttpResponse::Ok().finish())
}

#[delete("/user")]
pub async fn delete_user(
    app: Data<App>,
    admin: Admin,
    Json(request): Json<DeleteUserRequest>,
) -> Result<HttpResponse, AppError> {
    let user_id = UserId(request.id);

    let user = app.user_by_id(user_id).await?;

    user.delete(&admin).await?;

    Ok(HttpResponse::Ok().finish())
}

#[get("/users")]
pub async fn list_users(app: Data<App>, admin: Admin) -> Result<Json<GetUsersResponse>, AppError> {
    let mut users = app.all_users(admin).await?;

    let user_results = join_all(users.iter_mut().map(|user| user.detailed_user_no_auth())).await;

    let mut out_users = Vec::with_capacity(user_results.len());
    for (result, user) in user_results.into_iter().zip(users) {
        match result {
            Ok(user) => {
                out_users.push(user);
            }
            Err(err) => {
                warn!("Failed to query detailed user of {user:?}: {err}");
            }
        }
    }

    Ok(Json(GetUsersResponse { users: out_users }))
}
