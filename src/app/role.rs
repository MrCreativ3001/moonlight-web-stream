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

    async fn storage_role(&self) -> Result<Arc<StorageRole>, AppError> {
        if let Some(storage) = self.cache_storage.as_ref() {
            return Ok(storage.clone());
        }

        let app = self.app.access()?;

        let user = app.storage.get_user(self.id).await?;
        let user = Arc::new(user);

        self.cache_storage = Some(user.clone());

        Ok(user)
    }

    pub async fn ty(&self) -> Result<RoleType, AppError> {
        todo!()
    }

    pub async fn name(&self) -> Result<RoleType, AppError> {
        todo!()
    }
}
