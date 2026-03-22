use std::{
    collections::HashMap,
    io,
    ops::Deref,
    sync::{Arc, Weak},
};

use actix_web::{ResponseError, http::StatusCode, web::Bytes};
use common::config::Config;
use futures_concurrency::future::RaceOk;
use hex::FromHexError;
use log::{error, warn};
use moonlight_common::{high::MoonlightClientError, http::client::tokio_hyper::TokioHyperClient};
use openssl::error::ErrorStack;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::app::{
    auth::{SessionToken, UserAuth},
    host::{AppId, HostId},
    password::StoragePassword,
    role::{Role, RoleId},
    storage::{Either, Storage, StorageHostModify, StorageRoleAdd, StorageUserAdd, create_storage},
    user::{Admin, AuthenticatedUser, RoleType, User, UserId},
};

pub mod auth;
pub mod host;
pub mod password;
pub mod role;
pub mod storage;
pub mod user;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("the app got destroyed")]
    AppDestroyed,
    #[error("the user was not found")]
    UserNotFound,
    #[error("the role was not found")]
    RoleNotFound,
    #[error("more than one user already exists")]
    FirstUserAlreadyExists,
    #[error("the config option first_login_create_admin is not true")]
    FirstLoginCreateAdminNotSet,
    #[error("the user already exists")]
    UserAlreadyExists,
    #[error("the host was not found")]
    HostNotFound,
    #[error("the host was already paired")]
    HostPaired,
    #[error("the host must be paired for this action")]
    HostNotPaired,
    // -- Unauthorized
    #[error("the credentials don't exists")]
    CredentialsWrong,
    #[error("the host was not found")]
    SessionTokenNotFound,
    #[error("the action is not allowed because the user is not authorized, 401")]
    Unauthorized,
    #[error("using a custom header for authorization is disabled")]
    HeaderAuthDisabled,
    // --
    #[error("the action is not allowed with the current privileges, 403")]
    Forbidden,
    // -- Bad Request
    #[error("the authorization header is not a bearer")]
    AuthorizationNotBearer,
    #[error("the custom header used to authorize is malformed")]
    HeaderAuthMalformed,
    #[error("the authorization header is not a bearer")]
    BearerMalformed,
    #[error("the password is empty")]
    PasswordEmpty,
    #[error("the password is empty")]
    UserNameEmpty,
    #[error("the authorization header is not a bearer")]
    BadRequest,
    // --
    #[error("openssl error occured: {0}")]
    OpenSSL(#[from] ErrorStack),
    #[error("hex error occured: {0}")]
    Hex(#[from] FromHexError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("moonlight error: {0}")]
    Moonlight(#[from] MoonlightClientError),
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::AppDestroyed => StatusCode::INTERNAL_SERVER_ERROR,
            Self::FirstUserAlreadyExists => StatusCode::INTERNAL_SERVER_ERROR,
            Self::FirstLoginCreateAdminNotSet => StatusCode::INTERNAL_SERVER_ERROR,
            Self::HostNotFound => StatusCode::NOT_FOUND,
            Self::HostNotPaired => StatusCode::FORBIDDEN,
            Self::HostPaired => StatusCode::NOT_MODIFIED,
            Self::UserNotFound => StatusCode::NOT_FOUND,
            Self::RoleNotFound => StatusCode::NOT_FOUND,
            Self::UserAlreadyExists => StatusCode::CONFLICT,
            Self::CredentialsWrong => StatusCode::UNAUTHORIZED,
            Self::SessionTokenNotFound => StatusCode::UNAUTHORIZED,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::OpenSSL(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::HeaderAuthDisabled => StatusCode::UNAUTHORIZED,
            Self::Hex(_) => StatusCode::BAD_REQUEST,
            Self::AuthorizationNotBearer => StatusCode::BAD_REQUEST,
            Self::HeaderAuthMalformed => StatusCode::BAD_REQUEST,
            Self::BearerMalformed => StatusCode::BAD_REQUEST,
            Self::PasswordEmpty => StatusCode::BAD_REQUEST,
            Self::UserNameEmpty => StatusCode::BAD_REQUEST,
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Moonlight(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Clone)]
struct AppRef {
    inner: Weak<AppInner>,
}

impl AppRef {
    fn access(&self) -> Result<impl Deref<Target = AppInner> + 'static, AppError> {
        Weak::upgrade(&self.inner).ok_or(AppError::AppDestroyed)
    }
}

struct AppInner {
    config: Config,
    storage: Arc<dyn Storage + Send + Sync>,
    app_image_cache: RwLock<HashMap<(UserId, HostId, AppId), Bytes>>,
}

pub type MoonlightClient = TokioHyperClient;

pub struct App {
    inner: Arc<AppInner>,
}

impl App {
    pub async fn new(config: Config) -> Result<Self, anyhow::Error> {
        let app = AppInner {
            storage: create_storage(config.data_storage.clone()).await?,
            config,
            app_image_cache: Default::default(),
        };

        Ok(Self {
            inner: Arc::new(app),
        })
    }

    fn new_ref(&self) -> AppRef {
        AppRef {
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// Handles all logic related to adding the first user:
    /// - Is this even currently allowed?
    /// - Moving hosts from global to first user
    pub async fn try_add_first_login(
        &self,
        username: String,
        password: String,
    ) -> Result<AuthenticatedUser, AppError> {
        if !self.config().web_server.first_login_create_admin {
            return Err(AppError::FirstLoginCreateAdminNotSet);
        }

        let any_user_exists = self.inner.storage.any_user_exists().await?;
        if any_user_exists {
            return Err(AppError::FirstUserAlreadyExists);
        }

        let admin_role = self.admin_role().await?;

        let mut user = self
            .add_user_no_auth(StorageUserAdd {
                name: username.clone(),
                password: Some(StoragePassword::new(&password)?),
                role_id: admin_role.id(),
                client_unique_id: username,
            })
            .await?;

        if self.config().web_server.first_login_assign_global_hosts {
            // Note: only this user exists and all hosts are global, if migrated from v1 to v2
            // -> list_hosts will show just global hosts

            let hosts = user.hosts().await?;

            let user_id = user.id();
            for mut host in hosts {
                match host
                    .modify(
                        &mut user,
                        StorageHostModify {
                            owner: Some(Some(user_id)),
                            ..Default::default()
                        },
                    )
                    .await
                {
                    Ok(_) => {}
                    Err(err) => {
                        warn!("failed to move global host to new user {user_id:?}: {err}");
                    }
                }
            }
        }

        Ok(user)
    }

    /// admin: The admin that tries to do this action
    pub async fn add_user(
        &self,
        _: &Admin,
        user: StorageUserAdd,
    ) -> Result<AuthenticatedUser, AppError> {
        self.add_user_no_auth(user).await
    }

    async fn add_user_no_auth(&self, user: StorageUserAdd) -> Result<AuthenticatedUser, AppError> {
        if user.name.is_empty() {
            return Err(AppError::UserNameEmpty);
        }

        let user = self.inner.storage.add_user(user).await?;

        Ok(AuthenticatedUser {
            inner: User {
                app: self.new_ref(),
                id: user.id,
                cache_storage: Some(user.into()),
            },
        })
    }

    pub async fn user_by_auth(&self, auth: UserAuth) -> Result<AuthenticatedUser, AppError> {
        match auth {
            UserAuth::None => {
                let user_id = self.config().web_server.default_user_id.map(UserId);
                if let Some(user_id) = user_id {
                    let user = match self.user_by_id(user_id).await {
                        Ok(user) => user,
                        Err(AppError::UserNotFound) => {
                            error!("the default user {user_id:?} was not found!");
                            return Err(AppError::UserNotFound);
                        }
                        Err(err) => return Err(err),
                    };

                    user.authenticate(&UserAuth::None).await
                } else {
                    Err(AppError::Unauthorized)
                }
            }
            UserAuth::UserPassword { ref username, .. } => {
                let user = self.user_by_name(username).await?;

                user.authenticate(&auth).await
            }
            UserAuth::Session(session) => {
                let user = self.user_by_session(session).await?;

                Ok(user)
            }
            UserAuth::ForwardedHeaders { ref username } => {
                let user = match self.user_by_name(username).await {
                    Ok(user) => user,
                    Err(AppError::UserNotFound) => {
                        let Some(config_forwarded_headers) =
                            &self.config().web_server.forwarded_header
                        else {
                            return Err(AppError::Unauthorized);
                        };

                        if !config_forwarded_headers.auto_create_missing_user {
                            return Err(AppError::Unauthorized);
                        }

                        // TODO: what role???
                        let role = self.default_role().await?;

                        let user = self
                            .add_user_no_auth(StorageUserAdd {
                                role_id: role.id(),
                                name: username.clone(),
                                password: None,
                                client_unique_id: username.clone(),
                            })
                            .await?;

                        return Ok(user);
                    }
                    Err(err) => return Err(err),
                };

                user.authenticate(&auth).await
            }
        }
    }

    pub async fn user_by_id(&self, user_id: UserId) -> Result<User, AppError> {
        let user = self.inner.storage.get_user(user_id).await?;

        Ok(User {
            app: self.new_ref(),
            id: user_id,
            cache_storage: Some(user.into()),
        })
    }
    pub async fn user_by_name(&self, name: &str) -> Result<User, AppError> {
        let (user_id, user) = self.inner.storage.get_user_by_name(name).await?;

        Ok(User {
            app: self.new_ref(),
            id: user_id,
            cache_storage: user.map(Into::into),
        })
    }
    pub async fn user_by_session(
        &self,
        session: SessionToken,
    ) -> Result<AuthenticatedUser, AppError> {
        let (user_id, user) = self
            .inner
            .storage
            .get_user_by_session_token(session)
            .await?;

        Ok(AuthenticatedUser {
            inner: User {
                app: self.new_ref(),
                id: user_id,
                cache_storage: user.map(Into::into),
            },
        })
    }

    pub async fn all_users(&self, _: Admin) -> Result<Vec<User>, AppError> {
        let users = self.inner.storage.list_users().await?;

        let users = match users {
            Either::Left(user_ids) => user_ids
                .into_iter()
                .map(|id| User {
                    app: self.new_ref(),
                    id,
                    cache_storage: None,
                })
                .collect::<Vec<_>>(),
            Either::Right(users) => users
                .into_iter()
                .map(|user| User {
                    app: self.new_ref(),
                    id: user.id,
                    cache_storage: Some(user.into()),
                })
                .collect::<Vec<_>>(),
        };

        Ok(users)
    }

    pub async fn delete_session(&self, session: SessionToken) -> Result<(), AppError> {
        self.inner.storage.remove_session_token(session).await
    }

    pub async fn admin_role(&self) -> Result<Role, AppError> {
        let roles = self.all_roles().await?;

        let result = roles
            .into_iter()
            .map(|mut role| async {
                let ty = role.ty().await?;

                if ty == RoleType::Admin {
                    Ok(role)
                } else {
                    Err(AppError::Unauthorized)
                }
            })
            .collect::<Vec<_>>()
            .race_ok()
            .await;

        match result {
            Ok(value) => return Ok(value),
            Err(_) => {
                // We've got no admin role -> add an admin role

                todo!()
            }
        }
    }
    pub async fn default_role(&self) -> Result<Role, AppError> {
        todo!()
    }

    pub async fn add_role(&self, _admin: &Admin, role: StorageRoleAdd) -> Result<Role, AppError> {
        let role = self.inner.storage.add_role(role).await?;

        Ok(Role {
            app: self.new_ref(),
            id: role.id,
            cache_storage: Some(role.into()),
        })
    }

    pub async fn role_by_id(&self, _id: RoleId) -> Result<Role, AppError> {
        todo!()
    }

    pub async fn all_roles(&self) -> Result<Vec<Role>, AppError> {
        todo!()
    }
}
