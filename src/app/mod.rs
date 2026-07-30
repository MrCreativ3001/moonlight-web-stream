use std::{
    collections::HashMap,
    io, mem,
    ops::Deref,
    sync::{Arc, Weak},
};

use actix_web::{ResponseError, http::StatusCode, web::Bytes};
use common::config::Config;
use futures_concurrency::future::RaceOk;
use hex::FromHexError;
use moonlight_common::{high::MoonlightClientError, http::client::tokio_hyper::TokioHyperClient};
use openssl::error::ErrorStack;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::app::{
    auth::{SessionToken, UserAuth},
    host::{AppId, HostId},
    oidc::{OidcError, PendingOidcLogins, validate_oidc_startup_config},
    password::StoragePassword,
    role::{Role, RoleId},
    storage::{
        Either, Storage, StorageHostModify, StorageOidcIdentity, StorageRoleAdd,
        StorageRoleDefaultSettings, StorageRolePermissions, StorageUserAdd, create_storage,
    },
    user::{Admin, AuthenticatedUser, RoleType, User, UserId},
};

pub mod auth;
pub mod host;
pub mod oidc;
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
    oidc_pending_logins: PendingOidcLogins,
}

pub type MoonlightClient = TokioHyperClient;

pub struct App {
    inner: Arc<AppInner>,
}

impl App {
    pub async fn new(config: Config) -> Result<Self, anyhow::Error> {
        validate_oidc_startup_config(&config)?;

        let app = AppInner {
            storage: create_storage(config.data_storage.clone()).await?,
            config,
            app_image_cache: Default::default(),
            oidc_pending_logins: Default::default(),
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

    pub fn oidc_pending_logins(&self) -> &PendingOidcLogins {
        &self.inner.oidc_pending_logins
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
                oidc_identity: None,
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

                        let role = self.default_role().await?;

                        info!("Adding new user {username:?} from proxy.");

                        let user = self
                            .add_user_no_auth(StorageUserAdd {
                                role_id: role.id(),
                                name: username.clone(),
                                password: None,
                                client_unique_id: username.clone(),
                                oidc_identity: None,
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

    pub async fn user_by_oidc_identity(
        &self,
        issuer: String,
        subject: String,
        username: String,
    ) -> Result<AuthenticatedUser, OidcError> {
        if issuer.is_empty() || subject.is_empty() {
            return Err(OidcError::InvalidIdToken);
        }
        if username.is_empty() {
            return Err(OidcError::MissingUsernameClaim);
        }

        let Some(config_oidc) = &self.config().web_server.oidc else {
            return Err(OidcError::NotConfigured);
        };

        match self
            .inner
            .storage
            .get_user_by_oidc_identity(&issuer, &subject)
            .await
        {
            Ok((id, user)) => Ok(AuthenticatedUser {
                inner: User {
                    app: self.new_ref(),
                    id,
                    cache_storage: user.map(Arc::new),
                },
            }),
            Err(AppError::UserNotFound) if config_oidc.auto_create_missing_user => {
                if self.user_by_name(&username).await.is_ok() {
                    return Err(OidcError::UsernameCollision);
                }
                let role = self
                    .default_role()
                    .await
                    .map_err(|_| OidcError::MissingUser)?;
                match self
                    .add_user_no_auth(StorageUserAdd {
                        role_id: role.id(),
                        name: username.clone(),
                        password: None,
                        client_unique_id: username,
                        oidc_identity: Some(StorageOidcIdentity {
                            issuer: issuer.clone(),
                            subject: subject.clone(),
                        }),
                    })
                    .await
                {
                    Ok(user) => Ok(user),
                    Err(AppError::UserAlreadyExists) => {
                        // A concurrent callback for the same identity may have
                        // provisioned the account after our initial lookup.
                        match self
                            .inner
                            .storage
                            .get_user_by_oidc_identity(&issuer, &subject)
                            .await
                        {
                            Ok((id, user)) => Ok(AuthenticatedUser {
                                inner: User {
                                    app: self.new_ref(),
                                    id,
                                    cache_storage: user.map(Arc::new),
                                },
                            }),
                            _ => Err(OidcError::UsernameCollision),
                        }
                    }
                    Err(_) => Err(OidcError::MissingUser),
                }
            }
            Err(AppError::UserNotFound) => Err(OidcError::MissingUser),
            Err(_) => Err(OidcError::MissingUser),
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

    async fn find_role(
        &self,
        filter: impl AsyncFn(&mut Role) -> Result<bool, AppError>,
    ) -> Result<Role, AppError> {
        let roles = self.all_roles_no_auth().await?;

        let role = roles
            .into_iter()
            .map(|mut role| async {
                if filter(&mut role).await? {
                    Ok(role)
                } else {
                    Err(AppError::RoleNotFound)
                }
            })
            .collect::<Vec<_>>()
            .race_ok()
            .await
            .map_err(|mut err| {
                let err = mem::take(&mut *err);
                err.into_iter()
                    .find(|x| !matches!(x, AppError::RoleNotFound))
                    .unwrap_or(AppError::RoleNotFound)
            })?;

        Ok(role)
    }

    /// Returns any role that is an Admin
    pub async fn admin_role(&self) -> Result<Role, AppError> {
        let result = self
            .find_role(async |role| {
                let ty = role.ty().await?;

                Ok(matches!(ty, RoleType::Admin))
            })
            .await;

        match result {
            Ok(value) => Ok(value),
            Err(AppError::RoleNotFound) => {
                // We've got no admin role -> add an admin role

                info!("There was no admin role found. Adding an Admin role");

                let role = self
                    .add_role_no_auth(StorageRoleAdd {
                        name: "Admin".to_owned(),
                        ty: RoleType::Admin,
                        default_settings: StorageRoleDefaultSettings::default(),
                        permissions: StorageRolePermissions::default(),
                    })
                    .await?;

                info!("Added admin role: {role:?}");

                Ok(role)
            }
            Err(err) => Err(err),
        }
    }
    /// Returns the first user role it finds
    pub async fn default_role(&self) -> Result<Role, AppError> {
        let default_role_id = self.config().web_server.default_role_id.map(RoleId);

        match default_role_id {
            None => {
                let result = self
                    .find_role(async |role| {
                        let ty = role.ty().await?;

                        Ok(matches!(ty, RoleType::User))
                    })
                    .await;

                match result {
                    Ok(value) => Ok(value),
                    Err(AppError::RoleNotFound) => {
                        // We've got no admin role -> add an admin role

                        info!("There was no default role found. Adding a new default user role");

                        let role = self
                            .add_role_no_auth(StorageRoleAdd {
                                name: "User".to_owned(),
                                ty: RoleType::User,
                                default_settings: StorageRoleDefaultSettings::default(),
                                permissions: StorageRolePermissions::default(),
                            })
                            .await?;

                        info!("Added user role: {role:?}");

                        Ok(role)
                    }
                    Err(err) => Err(err),
                }
            }
            Some(id) => self.role_by_id(id).await,
        }
    }

    pub async fn add_role(&self, _admin: &Admin, role: StorageRoleAdd) -> Result<Role, AppError> {
        self.add_role_no_auth(role).await
    }
    pub async fn add_role_no_auth(&self, role: StorageRoleAdd) -> Result<Role, AppError> {
        let role = self.inner.storage.add_role(role).await?;

        Ok(Role {
            app: self.new_ref(),
            id: role.id,
            cache_storage: Some(role.into()),
        })
    }

    pub async fn role_by_id(&self, id: RoleId) -> Result<Role, AppError> {
        let role = self.inner.storage.get_role(id).await?;

        Ok(Role {
            app: self.new_ref(),
            id: role.id,
            cache_storage: Some(role.into()),
        })
    }

    pub async fn all_roles(&self, _admin: &Admin) -> Result<Vec<Role>, AppError> {
        self.all_roles_no_auth().await
    }

    pub async fn all_roles_no_auth(&self) -> Result<Vec<Role>, AppError> {
        let roles = self.inner.storage.list_roles().await?;

        let roles = match roles {
            Either::Left(role_ids) => role_ids
                .into_iter()
                .map(|id| Role {
                    app: self.new_ref(),
                    id,
                    cache_storage: None,
                })
                .collect::<Vec<_>>(),
            Either::Right(roles) => roles
                .into_iter()
                .map(|role| Role {
                    app: self.new_ref(),
                    id: role.id,
                    cache_storage: Some(role.into()),
                })
                .collect::<Vec<_>>(),
        };

        Ok(roles)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use common::config::{Config, OidcConfig, StorageConfig};

    use crate::app::{
        App,
        oidc::OidcError,
        role::RoleId,
        storage::{
            StorageOidcIdentity, StorageRoleAdd, StorageRoleDefaultSettings,
            StorageRolePermissions, StorageUserAdd,
        },
        user::RoleType,
    };

    fn test_config(auto_create_missing_user: bool) -> Config {
        let mut config = Config::default();
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        config.data_storage = StorageConfig::Json {
            path: std::env::temp_dir()
                .join(format!("moonlight-web-stream-oidc-app-test-{suffix}.json"))
                .display()
                .to_string(),
            session_expiration_check_interval: Duration::from_secs(60),
        };
        config.web_server.session_cookie_secure = true;
        config.web_server.oidc = Some(OidcConfig {
            issuer_url: "https://idp.example.com/realms/moonlight".to_string(),
            client_id: "moonlight-web".to_string(),
            client_secret: None,
            redirect_url: "https://example.com/api/oidc/callback".to_string(),
            scopes: vec!["openid".to_string(), "profile".to_string()],
            username_claim: "preferred_username".to_string(),
            auto_create_missing_user,
            display_label: "OpenID Connect".to_string(),
        });
        config
    }

    #[actix_web::test]
    async fn oidc_mapping_rejects_existing_local_user_with_same_username() {
        let app = App::new(test_config(false))
            .await
            .expect("app should start");
        let admin_role = app
            .add_role_no_auth(StorageRoleAdd {
                name: "Admin".to_string(),
                ty: RoleType::Admin,
                default_settings: StorageRoleDefaultSettings::default(),
                permissions: StorageRolePermissions::default(),
            })
            .await
            .expect("role should be created");
        let _existing = app
            .add_user_no_auth(StorageUserAdd {
                role_id: admin_role.id(),
                name: "alice".to_string(),
                password: Some(
                    crate::app::password::StoragePassword::new("password")
                        .expect("password should hash"),
                ),
                client_unique_id: "alice".to_string(),
                oidc_identity: None,
            })
            .await
            .expect("user should be created");

        let result = app
            .user_by_oidc_identity(
                "https://idp.example.com/realms/moonlight".to_string(),
                "subject-1".to_string(),
                "alice".to_string(),
            )
            .await;

        assert!(matches!(result, Err(OidcError::MissingUser)));
    }

    #[actix_web::test]
    async fn oidc_mapping_rejects_missing_user_when_auto_create_disabled() {
        let app = App::new(test_config(false))
            .await
            .expect("app should start");

        let result = app
            .user_by_oidc_identity(
                "https://idp.example.com/realms/moonlight".to_string(),
                "subject-1".to_string(),
                "alice".to_string(),
            )
            .await;

        assert!(matches!(result, Err(OidcError::MissingUser)));
    }

    #[actix_web::test]
    async fn oidc_mapping_auto_creates_default_role_user_with_identity_when_enabled() {
        let mut config = test_config(true);
        config.web_server.default_role_id = None;
        let app = App::new(config).await.expect("app should start");

        let mut mapped = app
            .user_by_oidc_identity(
                "https://idp.example.com/realms/moonlight".to_string(),
                "subject-1".to_string(),
                "alice".to_string(),
            )
            .await
            .expect("missing user should be created");

        let mapped_id = mapped.id();
        assert_eq!(
            mapped
                .detailed_user()
                .await
                .expect("details should load")
                .name,
            "alice"
        );
        assert_ne!(mapped.role_id().await.expect("role should load"), RoleId(0));

        let stored = app
            .user_by_id(mapped_id)
            .await
            .expect("stored user should load");
        let storage = app
            .inner
            .storage
            .get_user(stored.id())
            .await
            .expect("storage user should load");
        assert_eq!(
            storage.oidc_identity,
            Some(StorageOidcIdentity {
                issuer: "https://idp.example.com/realms/moonlight".to_string(),
                subject: "subject-1".to_string(),
            })
        );
    }

    #[actix_web::test]
    async fn oidc_mapping_rejects_username_collision_between_subjects() {
        let app = App::new(test_config(true)).await.expect("app should start");
        let first = app
            .user_by_oidc_identity(
                "https://idp.example.com/realms/moonlight".to_string(),
                "subject-1".to_string(),
                "alice".to_string(),
            )
            .await
            .expect("first subject should provision");

        let second = app
            .user_by_oidc_identity(
                "https://idp.example.com/realms/moonlight".to_string(),
                "subject-2".to_string(),
                "alice".to_string(),
            )
            .await;

        assert!(matches!(second, Err(OidcError::UsernameCollision)));
        assert_eq!(
            first.id(),
            app.user_by_name("alice")
                .await
                .expect("alice should still resolve by local username")
                .id()
        );
    }

    #[actix_web::test]
    async fn oidc_mapping_reuses_identity_when_username_claim_changes() {
        let app = App::new(test_config(true)).await.expect("app should start");
        let first = app
            .user_by_oidc_identity(
                "https://idp.example.com/realms/moonlight".to_string(),
                "subject-1".to_string(),
                "alice".to_string(),
            )
            .await
            .expect("first login should provision");
        let second = app
            .user_by_oidc_identity(
                "https://idp.example.com/realms/moonlight".to_string(),
                "subject-1".to_string(),
                "renamed-alice".to_string(),
            )
            .await
            .expect("same identity should map");

        assert_eq!(first.id(), second.id());
    }

    #[actix_web::test]
    async fn concurrent_oidc_provisioning_reuses_one_identity() {
        let app = App::new(test_config(true)).await.expect("app should start");

        let first = app.user_by_oidc_identity(
            "https://idp.example.com/realms/moonlight".to_string(),
            "subject-concurrent".to_string(),
            "concurrent-alice".to_string(),
        );
        let second = app.user_by_oidc_identity(
            "https://idp.example.com/realms/moonlight".to_string(),
            "subject-concurrent".to_string(),
            "concurrent-alice".to_string(),
        );
        let (first, second) = tokio::join!(first, second);

        let first = first.expect("first callback should resolve");
        let second = second.expect("concurrent callback should resolve");
        assert_eq!(first.id(), second.id());
    }

    #[actix_web::test]
    async fn old_json_without_oidc_identity_loads() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "moonlight-web-stream-oidc-old-json-test-{suffix}.json"
        ));
        fs::write(
            &path,
            r#"{
                "version": "3",
                "users": {
                    "1": {
                        "role_id": 1,
                        "name": "alice",
                        "password": null,
                        "client_unique_id": "alice"
                    }
                },
                "hosts": {},
                "roles": {
                    "1": {
                        "name": "User",
                        "ty": "User",
                        "default_settings": {},
                        "permissions": {
                            "allow_add_hosts": true,
                            "maximum_bitrate_kbps": null,
                            "allow_codec_h264": true,
                            "allow_codec_h265": true,
                            "allow_codec_av1": true,
                            "allow_hdr": true,
                            "allow_transport_webrtc": true,
                            "allow_transport_websockets": true
                        }
                    }
                }
            }"#,
        )
        .expect("old json should be written");

        let mut config = test_config(false);
        config.data_storage = StorageConfig::Json {
            path: path.display().to_string(),
            session_expiration_check_interval: Duration::from_secs(60),
        };
        let app = App::new(config).await.expect("old json should load");
        let storage = app
            .inner
            .storage
            .get_user_by_name("alice")
            .await
            .expect("old user should load")
            .1
            .expect("old user should be returned");

        assert_eq!(storage.oidc_identity, None);
        let _ = fs::remove_file(path);
    }
}
