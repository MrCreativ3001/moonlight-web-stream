use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use common::config::StorageConfig;
use moonlight_common::mac::MacAddress;
use pem::Pem;

use crate::app::{
    AppError,
    auth::SessionToken,
    host::HostId,
    password::StoragePassword,
    storage::json::JsonStorage,
    user::{Role, UserId},
};

pub mod json;

pub async fn create_storage(
    config: StorageConfig,
) -> Result<Arc<dyn Storage + Send + Sync>, anyhow::Error> {
    match config {
        StorageConfig::Json {
            path,
            session_expiration_check_interval,
        } => {
            let storage = JsonStorage::load(path.into(), session_expiration_check_interval).await?;

            Ok(storage)
        }
    }
}

// Storages:
// - If two options are in a Modify struct it means: First option = change the field, second option = should pair info exist

#[derive(Clone)]
pub struct StorageUser {
    pub id: UserId,
    pub name: String,
    pub password: Option<StoragePassword>,
    pub role: Role,
    pub client_unique_id: String,
}
#[derive(Clone)]
pub struct StorageUserAdd {
    pub role: Role,
    pub name: String,
    pub password: Option<StoragePassword>,
    pub client_unique_id: String,
}
#[derive(Default, Clone)]
pub struct StorageUserModify {
    pub role: Option<Role>,
    pub password: Option<Option<StoragePassword>>,
    pub client_unique_id: Option<String>,
}

#[derive(Clone)]
pub struct StorageHost {
    pub id: HostId,
    // If this is none it means the host is accessible by everyone
    pub owner: Option<UserId>,
    pub address: String,
    pub http_port: u16,
    pub pair_info: Option<StorageHostPairInfo>,
    pub cache: StorageHostCache,
}
#[derive(Clone)]
pub struct StorageHostAdd {
    pub owner: Option<UserId>,
    pub address: String,
    pub http_port: u16,
    pub pair_info: Option<StorageHostPairInfo>,
    pub cache: StorageHostCache,
}
#[derive(Clone)]
pub struct StorageHostCache {
    pub name: String,
    pub mac: Option<MacAddress>,
}
#[derive(Clone)]
pub struct StorageHostPairInfo {
    pub client_private_key: Pem,
    pub client_certificate: Pem,
    pub server_certificate: Pem,
}
#[derive(Default, Clone)]
pub struct StorageHostModify {
    pub owner: Option<Option<UserId>>,
    pub address: Option<String>,
    pub http_port: Option<u16>,
    pub pair_info: Option<Option<StorageHostPairInfo>>,
    pub cache_name: Option<String>,
    pub cache_mac: Option<Option<MacAddress>>,
}

#[derive(Clone)]
pub struct StorageQueryHosts {
    pub user_id: UserId,
}

pub enum Either<L, R> {
    #[allow(dead_code)]
    Left(L),
    Right(R),
}

#[async_trait]
pub trait Storage {
    /// No duplicate names are allowed!
    async fn add_user(&self, user: StorageUserAdd) -> Result<StorageUser, AppError>;
    async fn modify_user(&self, user_id: UserId, user: StorageUserModify) -> Result<(), AppError>;
    async fn get_user(&self, user_id: UserId) -> Result<StorageUser, AppError>;
    /// The returned tuple can contain a StorageUser if the Storage thinks it's more efficient to query all data directly
    async fn get_user_by_name(&self, name: &str)
    -> Result<(UserId, Option<StorageUser>), AppError>;
    async fn remove_user(&self, user_id: UserId) -> Result<(), AppError>;
    /// The returned tuple can contain a Vec<UserId> or Vec<StorageUser> if the Storage thinks it's more efficient to query all data directly
    async fn list_users(&self) -> Result<Either<Vec<UserId>, Vec<StorageUser>>, AppError>;
    async fn any_user_exists(&self) -> Result<bool, AppError>;

    async fn create_session_token(
        &self,
        user_id: UserId,
        expires_after: Duration,
    ) -> Result<SessionToken, AppError>;
    async fn remove_session_token(&self, session: SessionToken) -> Result<(), AppError>;
    #[allow(dead_code)]
    async fn remove_all_user_session_tokens(&self, user_id: UserId) -> Result<(), AppError>;
    /// The returned tuple can contain a StorageUser if the Storage thinks it's more efficient to query all data directly
    async fn get_user_by_session_token(
        &self,
        session: SessionToken,
    ) -> Result<(UserId, Option<StorageUser>), AppError>;

    async fn add_host(&self, host: StorageHostAdd) -> Result<StorageHost, AppError>;
    async fn modify_host(&self, host_id: HostId, host: StorageHostModify) -> Result<(), AppError>;
    async fn get_host(&self, host_id: HostId) -> Result<StorageHost, AppError>;
    async fn remove_host(&self, host_id: HostId) -> Result<(), AppError>;

    /// Returns all hosts that either have no owner (global) or have the specified user_id as an owner
    ///
    /// The returned tuple in the Vec can contain a StorageHost if the Storage thinks it's more efficient to query all data directly
    async fn list_user_hosts(
        &self,
        query: StorageQueryHosts,
    ) -> Result<Vec<(HostId, Option<StorageHost>)>, AppError>;
}
