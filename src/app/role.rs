use common::api_bindings::{StreamPermissions, StreamSettings};

use crate::app::storage::StorageRoleModify;
use crate::app::user::Admin;
use crate::app::{AppError, AppRef, storage::StorageRole, user::RoleType};
use std::fmt::{self, Display};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoleId(pub u32);

impl Display for RoleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl RoleId {
    pub fn role_type(&self) -> RoleType {
        if self.0 == 0 {
            RoleType::Admin
        } else {
            RoleType::User
        }
    }
}

#[derive(Clone)]
pub struct Role {
    pub(super) app: AppRef,
    pub(super) id: RoleId,
    pub(super) cache_storage: Option<Arc<StorageRole>>,
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

    pub async fn permissions(&self) -> Result<StreamPermissions, AppError> {
        let storage = self.storage_role().await?;

        Ok(storage.permissions.clone())
    }
    pub async fn default_settings(&self) -> Result<StreamSettings, AppError> {
        let storage = self.storage_role().await?;

        Ok(storage.default_settings.clone())
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
}
