use std::fmt::{Debug, Formatter};

use actix_web::web::Bytes;
use common::api_bindings::{self, DetailedHost, HostOwner, HostState, PairStatus, UndetailedHost};
use moonlight_common::{
    crypto::openssl::OpenSSLCryptoBackend,
    high::{
        MoonlightClientError,
        tokio::{MoonlightHost, broadcast_magic_packet},
    },
    http::{
        ClientIdentifier, ClientSecret, ServerIdentifier,
        pair::{PairPin, PairingCryptoBackend, client::ClientPairingError},
        server_info::ServerInfoResponse,
    },
};

use crate::app::{
    AppError, AppInner, AppRef, MoonlightClient,
    storage::{StorageHost, StorageHostModify, StorageHostPairInfo},
    user::{AuthenticatedUser, Role, UserId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HostId(pub u32);

pub struct Host {
    pub(super) app: AppRef,
    pub(super) id: HostId,
    pub(super) cache_storage: Option<StorageHost>,
    pub(super) cache_host_info: Option<(UserId, ServerInfoResponse)>,
}

impl Debug for Host {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AppId(pub u32);

pub struct App {
    pub id: AppId,
    pub title: String,
    pub is_hdr_supported: bool,
}

impl From<moonlight_common::http::app_list::App> for App {
    fn from(value: moonlight_common::http::app_list::App) -> Self {
        Self {
            id: AppId(value.id),
            title: value.title,
            is_hdr_supported: value.is_hdr_supported,
        }
    }
}
impl From<App> for api_bindings::App {
    fn from(value: App) -> Self {
        Self {
            app_id: value.id.0,
            title: value.title,
            is_hdr_supported: value.is_hdr_supported,
        }
    }
}

impl Host {
    #[allow(dead_code)]
    pub fn id(&self) -> HostId {
        self.id
    }

    async fn can_use(&self, user: &mut AuthenticatedUser) -> Result<(), AppError> {
        let owner = self.owner().await?;
        if owner.is_none() || owner == Some(user.id()) || matches!(user.role().await?, Role::Admin)
        {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    pub async fn modify(
        &mut self,
        user: &mut AuthenticatedUser,
        modify: StorageHostModify,
    ) -> Result<(), AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        self.cache_storage = None;

        app.storage.modify_host(self.id, modify).await?;

        Ok(())
    }

    pub async fn owner(&self) -> Result<Option<UserId>, AppError> {
        let app = self.app.access()?;

        let host = self.storage_host(&app).await?;

        Ok(host.owner)
    }
    async fn owner_info(
        &self,
        user: &AuthenticatedUser,
        this: &StorageHost,
    ) -> Result<HostOwner, AppError> {
        Ok(match this.owner {
            None => HostOwner::Global,
            Some(user_id) if user.id() == user_id => HostOwner::ThisUser,
            _ => unreachable!(),
        })
    }

    pub async fn undetailed_host_cached(
        &self,
        user: &mut AuthenticatedUser,
    ) -> Result<UndetailedHost, AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        let storage = self.storage_host(&app).await?;
        let owner = self.owner_info(user, &storage).await?;

        Ok(UndetailedHost {
            host_id: storage.id.0,
            name: storage.cache.name,
            owner,
            paired: if storage.pair_info.is_some() {
                PairStatus::Paired
            } else {
                PairStatus::NotPaired
            },
            server_state: None,
        })
    }

    async fn use_client<R>(
        &mut self,
        app: &AppInner,
        user: &mut AuthenticatedUser,
        // app, https_capable, client, host, port, client_info
        f: impl AsyncFnOnce(&mut Self, &MoonlightHost<MoonlightClient>) -> R,
    ) -> Result<R, AppError> {
        let user_unique_id = user.host_unique_id().await?;
        let host_data = self.storage_host(app).await?;

        // TODO: put this globally somewhere and retrieve it?
        let host = MoonlightHost::<MoonlightClient>::new(
            host_data.address.clone(),
            host_data.http_port,
            Some(user_unique_id),
        )?;

        if let Some(pair_info) = host_data.pair_info {
            host.set_identity(
                ClientIdentifier::from_pem(pair_info.client_certificate),
                ClientSecret::from_pem(pair_info.client_private_key),
                ServerIdentifier::from_pem(pair_info.server_certificate),
            )
            .await?;
        }

        Ok(f(self, &host).await)
    }

    async fn storage_host(&self, app: &AppInner) -> Result<StorageHost, AppError> {
        if let Some(host) = self.cache_storage.as_ref() {
            return Ok(host.clone());
        }

        app.storage.get_host(self.id).await
    }

    pub async fn address_port(
        &self,
        user: &mut AuthenticatedUser,
    ) -> Result<(String, u16), AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        let host = app.storage.get_host(self.id).await?;

        Ok((host.address, host.http_port))
    }

    pub async fn pair_info(
        &self,
        user: &mut AuthenticatedUser,
    ) -> Result<StorageHostPairInfo, AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        let host = app.storage.get_host(self.id).await?;

        host.pair_info.ok_or(AppError::HostNotPaired)
    }

    fn is_offline<T>(
        &self,
        result: Result<T, MoonlightClientError>,
    ) -> Result<Option<T>, AppError> {
        match result {
            Ok(value) => Ok(Some(value)),
            Err(MoonlightClientError::Offline) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }
    // None = Offline
    async fn host_info(
        &mut self,
        app: &AppInner,
        user: &mut AuthenticatedUser,
    ) -> Result<Option<ServerInfoResponse>, AppError> {
        let user_id = user.id();

        if let Some((cache_user_id, cache)) = self.cache_host_info.as_ref()
            && *cache_user_id == user_id
        {
            return Ok(Some(cache.clone()));
        }

        self.use_client(app, user, async |this, host| {
            let info = match this.is_offline(host.server_info().await) {
                Ok(Some(value)) => value,
                err => return err,
            };

            this.cache_host_info = Some((user_id, info.clone()));

            Ok(Some(info))
        })
        .await?
    }

    pub async fn undetailed_host(
        &mut self,
        user: &mut AuthenticatedUser,
    ) -> Result<UndetailedHost, AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        let storage = self.storage_host(&app).await?;
        let owner = self.owner_info(user, &storage).await?;

        match self.host_info(&app, user).await {
            Ok(Some(info)) => Ok(UndetailedHost {
                host_id: self.id.0,
                name: info.host_name,
                owner,
                paired: PairStatus::from_paired(info.paired),
                server_state: Some(HostState::from(info.state)),
            }),
            Ok(None) => {
                let host = self.storage_host(&app).await?;

                let paired = if host.pair_info.is_some() {
                    PairStatus::Paired
                } else {
                    PairStatus::NotPaired
                };

                Ok(UndetailedHost {
                    host_id: self.id.0,
                    name: host.cache.name,
                    owner,
                    paired,
                    server_state: None,
                })
            }
            Err(err) => Err(err),
        }
    }
    pub async fn detailed_host(
        &mut self,
        user: &mut AuthenticatedUser,
    ) -> Result<DetailedHost, AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        let storage = self.storage_host(&app).await?;

        let owner = self.owner_info(user, &storage).await?;

        match self.host_info(&app, user).await {
            Ok(Some(info)) => Ok(DetailedHost {
                host_id: self.id.0,
                owner,
                name: info.host_name,
                paired: PairStatus::from_paired(info.paired),
                server_state: Some(HostState::from(info.state)),
                address: storage.address,
                http_port: storage.http_port,
                https_port: info.https_port,
                external_port: info.external_port,
                version: info.app_version.to_string(),
                gfe_version: info.gfe_version,
                unique_id: info.unique_id.to_string(),
                mac: info.mac.map(|mac| mac.to_string()),
                local_ip: info.local_ip.to_string(),
                current_game: info.current_game,
                max_luma_pixels_hevc: info.max_luma_pixels_hevc,
                server_codec_mode_support: info.server_codec_mode_support.bits(),
            }),
            Ok(None) => {
                let paired = if storage.pair_info.is_some() {
                    PairStatus::Paired
                } else {
                    PairStatus::NotPaired
                };

                Ok(DetailedHost {
                    host_id: self.id.0,
                    owner,
                    name: storage.cache.name,
                    paired,
                    server_state: None,
                    address: storage.address,
                    http_port: storage.http_port,
                    https_port: 0,
                    external_port: 0,
                    version: "Offline".to_string(),
                    gfe_version: "Offline".to_string(),
                    unique_id: "Offline".to_string(),
                    mac: storage.cache.mac.map(|mac| mac.to_string()),
                    local_ip: "Offline".to_string(),
                    current_game: 0,
                    max_luma_pixels_hevc: 0,
                    server_codec_mode_support: 0,
                })
            }
            Err(err) => Err(err),
        }
    }

    #[allow(dead_code)]
    pub async fn is_paired(
        &mut self,
        user: &mut AuthenticatedUser,
    ) -> Result<PairStatus, AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        match self.host_info(&app, user).await? {
            Some(info) => Ok(PairStatus::from_paired(info.paired)),
            None => Ok(PairStatus::NotPaired),
        }
    }

    pub async fn pair(
        &mut self,
        user: &mut AuthenticatedUser,
        pin: PairPin,
    ) -> Result<(), AppError> {
        self.can_use(user).await?;

        let user_id = user.id();
        let app = self.app.access()?;

        let info = self
            .host_info(&app, user)
            .await?
            .ok_or(AppError::HostNotFound)?;

        if info.paired {
            return Err(AppError::HostPaired);
        }

        let modify = self
            .use_client(&app, user, async |this, host| {
                let (client_identifier, client_secret) = OpenSSLCryptoBackend
                    .generate_client_identity()
                    .map_err(|err| {
                        MoonlightClientError::Pairing(ClientPairingError::Crypto(Box::new(err)))
                    })?;

                // Store pair info
                host.pair(
                    &client_identifier,
                    &client_secret,
                    "roth".to_string(),
                    pin,
                    OpenSSLCryptoBackend,
                )
                .await?;
                let info = host.server_info().await?;

                let host_name = info.host_name.clone();
                let mac = info.mac;

                this.cache_host_info = Some((user_id, info.clone()));

                let Some((_, _, server_identifier)) = host.identity().await else {
                    unreachable!()
                };

                Ok::<_, AppError>(StorageHostModify {
                    pair_info: Some(Some(StorageHostPairInfo {
                        client_certificate: client_identifier.to_pem(),
                        client_private_key: client_secret.to_pem(),
                        server_certificate: server_identifier.to_pem(),
                    })),
                    cache_name: Some(host_name),
                    cache_mac: Some(mac),
                    ..Default::default()
                })
            })
            .await??;

        self.modify(user, modify).await
    }

    #[allow(dead_code)]
    pub async fn unpair(&self, user: &mut AuthenticatedUser) -> Result<Host, AppError> {
        self.can_use(user).await?;

        todo!()
    }

    pub async fn wake(&self, user: &mut AuthenticatedUser) -> Result<(), AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        let storage = self.storage_host(&app).await?;

        if let Some(mac) = storage.cache.mac {
            broadcast_magic_packet(mac).await?;
            Ok(())
        } else {
            Err(AppError::HostNotFound)
        }
    }

    pub async fn list_apps(&mut self, user: &mut AuthenticatedUser) -> Result<Vec<App>, AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        self.use_client(&app, user, async |_this, host| {
            let apps = host.app_list().await?;

            let apps = apps.into_iter().map(App::from).collect::<Vec<_>>();

            Ok(apps)
        })
        .await?
    }
    pub async fn app_image(
        &mut self,
        user: &mut AuthenticatedUser,
        app_id: AppId,
        force_refresh: bool,
    ) -> Result<Bytes, AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        let cache_key = (user.id(), self.id, app_id);
        if !force_refresh {
            {
                let app_images = app.app_image_cache.read().await;
                if let Some(app_image) = app_images.get(&cache_key) {
                    return Ok(app_image.clone());
                }
            }
        }

        let app_image = self
            .use_client(&app, user, async |_this, host| {
                let image = host.request_app_image(app_id.0).await?;

                Ok::<_, AppError>(image)
            })
            .await??;
        let app_image = Bytes::from_owner(app_image);

        {
            let mut app_images = app.app_image_cache.write().await;
            app_images.insert(cache_key, app_image.clone());
        }

        Ok(app_image)
    }

    pub async fn cancel_app(&mut self, user: &mut AuthenticatedUser) -> Result<bool, AppError> {
        self.can_use(user).await?;

        let app = self.app.access()?;

        self.use_client(&app, user, async |_this, host| {
            let success = host.cancel().await?;

            Ok(success)
        })
        .await?
    }

    pub async fn delete(self, user: &mut AuthenticatedUser) -> Result<(), AppError> {
        let app = self.app.access()?;

        let host = app.storage.get_host(self.id).await?;

        if host.owner == Some(user.id()) || matches!(user.role().await?, Role::Admin) {
            {
                let mut app_images = app.app_image_cache.write().await;
                app_images.retain(|(_, host_id, _), _| *host_id != self.id);
            }

            drop(app);
            self.delete_no_auth().await
        } else {
            Err(AppError::Forbidden)
        }
    }
    pub async fn delete_no_auth(self) -> Result<(), AppError> {
        let app = self.app.access()?;

        app.storage.remove_host(self.id).await?;

        Ok(())
    }
}
