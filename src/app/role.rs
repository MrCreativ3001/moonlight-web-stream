use common::api_bindings::{DetailedRole, StreamPermissions, UndetailedRole};
use common::api_bindings_ext::TsAny;

use crate::app::storage::StorageRoleModify;
use crate::app::user::Admin;
use crate::app::{AppError, AppRef, storage::StorageRole, user::RoleType};
use std::fmt::{self, Debug, Display};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoleId(pub u32);

impl Display for RoleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone)]
pub struct Role {
    pub(super) app: AppRef,
    pub(super) id: RoleId,
    pub(super) cache_storage: Option<Arc<StorageRole>>,
}

impl Debug for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.id)
    }
}

impl Role {
    pub fn id(&self) -> RoleId {
        self.id
    }

    async fn storage_role(&mut self) -> Result<Arc<StorageRole>, AppError> {
        if let Some(storage) = self.cache_storage.as_ref() {
            return Ok(storage.clone());
        }

        let app = self.app.access()?;

        let role = app.storage.get_role(self.id).await?;
        let role = Arc::new(role);

        self.cache_storage = Some(role.clone());

        Ok(role)
    }

    pub async fn ty(&mut self) -> Result<RoleType, AppError> {
        let storage = self.storage_role().await?;

        Ok(storage.ty)
    }

    pub async fn name(&mut self) -> Result<String, AppError> {
        let storage = self.storage_role().await?;

        Ok(storage.name.clone())
    }

    pub async fn permissions(&mut self) -> Result<StreamPermissions, AppError> {
        let storage = self.storage_role().await?;
        let permissions = &storage.permissions;

        Ok(StreamPermissions {
            allow_add_hosts: permissions.allow_add_hosts,
            maximum_bitrate_kbps: permissions.maximum_bitrate_kbps,
            allow_codec_h264: permissions.allow_codec_h264,
            allow_codec_h265: permissions.allow_codec_h265,
            allow_codec_av1: permissions.allow_codec_av1,
            allow_hdr: permissions.allow_hdr,
            allow_transport_webrtc: permissions.allow_transport_webrtc,
            allow_transport_websockets: permissions.allow_transport_websockets,
        })
    }
    pub async fn default_settings(&mut self) -> Result<TsAny, AppError> {
        let storage = self.storage_role().await?;
        let default_settings = &storage.default_settings;

        Ok(default_settings.value.clone().into())
    }

    pub async fn modify(&self, _admin: &Admin, modify: StorageRoleModify) -> Result<(), AppError> {
        let app = self.app.access()?;

        app.storage.modify_role(self.id, modify).await?;

        Ok(())
    }

    pub async fn delete(self, _admin: &Admin) -> Result<(), AppError> {
        let app = self.app.access()?;

        app.storage.remove_role(self.id).await?;

        Ok(())
    }

    pub async fn undetailed_role(&mut self) -> Result<UndetailedRole, AppError> {
        Ok(UndetailedRole {
            id: self.id().0,
            name: self.name().await?,
        })
    }
    pub async fn detailed_role(&mut self) -> Result<DetailedRole, AppError> {
        Ok(DetailedRole {
            id: self.id().0,
            name: self.name().await?,
            default_settings: self.default_settings().await?,
            permissions: self.permissions().await?,
        })
    }
}
