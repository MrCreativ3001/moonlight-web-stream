use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use common::config::StorageConfig;
use moonlight_common::mac::MacAddress;
use pem::Pem;
use serde_json::Value;

use crate::app::{
    AppError,
    auth::SessionToken,
    host::HostId,
    password::StoragePassword,
    role::RoleId,
    storage::json::JsonStorage,
    user::{RoleType, UserId},
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
// - If two options are in a Modify struct it means: First option = change the field, second option = is this value null

// --- User ---
#[derive(Clone)]
pub struct StorageUser {
    pub id: UserId,
    pub name: String,
    pub password: Option<StoragePassword>,
    pub role_id: RoleId,
    pub client_unique_id: String,
}
#[derive(Clone)]
pub struct StorageUserAdd {
    pub role_id: RoleId,
    pub name: String,
    pub password: Option<StoragePassword>,
    pub client_unique_id: String,
}
#[derive(Default, Clone)]
pub struct StorageUserModify {
    pub role_id: Option<RoleId>,
    pub password: Option<Option<StoragePassword>>,
    pub client_unique_id: Option<String>,
}

// --- Roles ---
#[derive(Clone, Default)]
pub struct StorageRoleDefaultSettings {
    pub value: Value,
}

#[derive(Clone)]
pub struct StorageRolePermissions {
    pub allow_add_hosts: bool,
    pub maximum_bitrate_kbps: Option<u32>,
    pub allow_codec_h264: bool,
    pub allow_codec_h265: bool,
    pub allow_codec_av1: bool,
    pub allow_hdr: bool,
    pub allow_transport_webrtc: bool,
    pub allow_transport_websockets: bool,
}

impl Default for StorageRolePermissions {
    fn default() -> Self {
        Self {
            allow_add_hosts: true,
            maximum_bitrate_kbps: None,
            allow_codec_h264: true,
            allow_codec_h265: true,
            allow_codec_av1: true,
            allow_hdr: true,
            allow_transport_webrtc: true,
            allow_transport_websockets: true,
        }
    }
}

#[derive(Clone)]
pub struct StorageRole {
    pub id: RoleId,
    pub name: String,
    pub ty: RoleType,
    pub default_settings: StorageRoleDefaultSettings,
    pub permissions: StorageRolePermissions,
}

#[derive(Clone)]
pub struct StorageRoleAdd {
    pub name: String,
    pub ty: RoleType,
    pub default_settings: StorageRoleDefaultSettings,
    pub permissions: StorageRolePermissions,
}

#[derive(Clone)]
pub struct StorageRoleModify {
    pub name: Option<String>,
    pub ty: Option<RoleType>,
    pub default_settings: Option<StorageRoleDefaultSettings>,
    pub permissions: Option<StorageRolePermissions>,
}

// --- Hosts ---
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
    // -- Roles --
    async fn add_role(&self, role: StorageRoleAdd) -> Result<StorageRole, AppError>;
    async fn modify_role(&self, role_id: RoleId, host: StorageRoleModify) -> Result<(), AppError>;
    async fn get_role(&self, role_id: RoleId) -> Result<StorageRole, AppError>;
    /// Deletes a role.
    ///
    /// All users that are in that role should also be delete.
    async fn remove_role(&self, role_id: RoleId) -> Result<(), AppError>;
    /// The returned tuple can contain a Vec<RoleId> or Vec<StorageRole> if the Storage thinks it's more efficient to query all data directly
    async fn list_roles(&self) -> Result<Either<Vec<RoleId>, Vec<StorageRole>>, AppError>;

    // -- Users --
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

    // -- Session Tokens --
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

    // -- Hosts --
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
