use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

use actix_web::web::Bytes;
use moonlight_common::{
    crypto::rustcrypto::RustCryptoBackend,
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

use crate::{
    api::bindings::{self, DetailedHost, HostState, PairStatus, UndetailedHost},
    app::storage::{StorageHost, StorageHostModify, StorageHostPairInfo},
    app::{AppError, AppInner, AppRef, RequestClient},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HostId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AppId(pub u32);

impl From<AppId> for moonlight_common::AppId {
    fn from(value: AppId) -> Self {
        Self(value.0)
    }
}

pub struct App {
    pub id: AppId,
    pub title: String,
    pub is_hdr_supported: bool,
}

impl From<moonlight_common::App> for App {
    fn from(value: moonlight_common::App) -> Self {
        Self {
            id: AppId(value.id.0),
            title: value.title,
            is_hdr_supported: value.is_hdr_supported,
        }
    }
}

impl From<App> for bindings::App {
    fn from(value: App) -> Self {
        Self {
            app_id: value.id.0,
            title: value.title,
            is_hdr_supported: value.is_hdr_supported,
        }
    }
}

pub struct Host {
    pub(super) app: AppRef,
    pub(super) id: HostId,
    pub(super) cache_storage: Option<StorageHost>,
    pub(super) cache_host_info: Option<ServerInfoResponse>,
}

impl Debug for Host {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.id)
    }
}

impl Host {
    pub async fn modify(&mut self, modify: StorageHostModify) -> Result<(), AppError> {
        let app = self.app.access()?;
        self.cache_storage = None;
        app.storage.modify_host(self.id, modify).await
    }

    async fn storage_host(&self, app: &AppInner) -> Result<StorageHost, AppError> {
        if let Some(host) = self.cache_storage.as_ref() {
            return Ok(host.clone());
        }
        app.storage.get_host(self.id).await
    }

    async fn use_request_client<R>(
        &mut self,
        app: &AppInner,
        f: impl AsyncFnOnce(&mut Self, &Arc<MoonlightHost<RequestClient>>) -> R,
    ) -> Result<R, AppError> {
        let host_data = self.storage_host(app).await?;
        let client_id = app.storage.client_unique_id().await?;
        let host = MoonlightHost::<RequestClient>::new(
            host_data.address.clone(),
            host_data.http_port,
            Some(client_id),
        )?;
        if let Some(pair_info) = host_data.pair_info {
            host.set_identity(
                ClientIdentifier::from_pem(pair_info.client_certificate),
                ClientSecret::from_pem(pair_info.client_private_key),
                ServerIdentifier::from_pem(pair_info.server_certificate),
            )
            .await?;
        }
        Ok(f(self, &Arc::new(host)).await)
    }

    pub async fn use_host(&mut self) -> Result<Arc<MoonlightHost<RequestClient>>, AppError> {
        let app = self.app.access()?;
        self.use_request_client(&app, async |_this, client| client.clone())
            .await
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

    async fn host_info(&mut self, app: &AppInner) -> Result<Option<ServerInfoResponse>, AppError> {
        if let Some(cache) = self.cache_host_info.as_ref() {
            return Ok(Some(cache.clone()));
        }
        self.use_request_client(app, async |this, host| {
            let info = match this.is_offline(host.server_info().await) {
                Ok(Some(value)) => value,
                err => return err,
            };
            this.cache_host_info = Some(info.clone());
            Ok(Some(info))
        })
        .await?
    }

    pub async fn undetailed_host_cached(&self) -> Result<UndetailedHost, AppError> {
        let app = self.app.access()?;
        let storage = self.storage_host(&app).await?;
        Ok(UndetailedHost {
            host_id: storage.id.0,
            name: storage.cache.name,
            paired: if storage.pair_info.is_some() {
                PairStatus::Paired
            } else {
                PairStatus::NotPaired
            },
            server_state: None,
        })
    }

    pub async fn undetailed_host(&mut self) -> Result<UndetailedHost, AppError> {
        let app = self.app.access()?;
        match self.host_info(&app).await? {
            Some(info) => Ok(UndetailedHost {
                host_id: self.id.0,
                name: info.host_name,
                paired: PairStatus::from_paired(info.paired),
                server_state: Some(HostState::from(info.state)),
            }),
            None => self.undetailed_host_cached().await,
        }
    }

    pub async fn detailed_host(&mut self) -> Result<DetailedHost, AppError> {
        let app = self.app.access()?;
        let storage = self.storage_host(&app).await?;
        match self.host_info(&app).await? {
            Some(info) => Ok(DetailedHost {
                host_id: self.id.0,
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
                mac: info.mac.map(|m| m.to_string()),
                local_ip: info.local_ip.to_string(),
                current_game: info.current_game,
                max_luma_pixels_hevc: info.max_luma_pixels_hevc,
                server_codec_mode_support: info.server_codec_mode_support.bits(),
            }),
            None => Ok(DetailedHost {
                host_id: self.id.0,
                name: storage.cache.name,
                paired: if storage.pair_info.is_some() {
                    PairStatus::Paired
                } else {
                    PairStatus::NotPaired
                },
                server_state: None,
                address: storage.address,
                http_port: storage.http_port,
                https_port: 0,
                external_port: None,
                version: "Offline".into(),
                gfe_version: "Offline".into(),
                unique_id: "Offline".into(),
                mac: storage.cache.mac.map(|m| m.to_string()),
                local_ip: "Offline".into(),
                current_game: 0,
                max_luma_pixels_hevc: 0,
                server_codec_mode_support: 0,
            }),
        }
    }

    pub async fn pair(&mut self, pin: PairPin) -> Result<(), AppError> {
        let app = self.app.access()?;
        let info = self.host_info(&app).await?.ok_or(AppError::HostNotFound)?;
        if info.paired {
            return Err(AppError::HostPaired);
        }
        let device_name = app.config.moonlight.pair_device_name.clone();
        let modify = self
            .use_request_client(&app, async |this, host| {
                let (client_identifier, client_secret) = RustCryptoBackend
                    .generate_client_identity()
                    .map_err(|err| {
                        MoonlightClientError::Pairing(ClientPairingError::Crypto(Box::new(err)))
                    })?;
                host.pair(
                    &client_identifier,
                    &client_secret,
                    device_name,
                    pin,
                    RustCryptoBackend,
                )
                .await?;
                let info = host.server_info().await?;
                this.cache_host_info = Some(info.clone());
                let Some((_, _, server_identifier)) = host.identity().await else {
                    unreachable!()
                };
                Ok::<_, AppError>(StorageHostModify {
                    pair_info: Some(Some(StorageHostPairInfo {
                        client_certificate: client_identifier.to_pem(),
                        client_private_key: client_secret.to_pem(),
                        server_certificate: server_identifier.to_pem(),
                    })),
                    cache_name: Some(info.host_name),
                    cache_mac: Some(info.mac),
                    ..Default::default()
                })
            })
            .await??;
        self.modify(modify).await
    }

    pub async fn wake(&self) -> Result<(), AppError> {
        let app = self.app.access()?;
        let storage = self.storage_host(&app).await?;
        let Some(mac) = storage.cache.mac else {
            return Err(AppError::HostNotFound);
        };
        broadcast_magic_packet(mac).await?;
        Ok(())
    }

    pub async fn list_apps(&mut self) -> Result<Vec<App>, AppError> {
        let app = self.app.access()?;
        self.use_request_client(&app, async |_this, host| {
            Ok(host.app_list().await?.into_iter().map(App::from).collect())
        })
        .await?
    }

    pub async fn app_image(
        &mut self,
        app_id: AppId,
        force_refresh: bool,
    ) -> Result<Bytes, AppError> {
        let app = self.app.access()?;
        let key = (self.id, app_id);
        if !force_refresh && let Some(image) = app.app_image_cache.read().await.get(&key) {
            return Ok(image.clone());
        }
        let image = self
            .use_request_client(&app, async |_this, host| {
                Ok::<_, AppError>(host.request_app_image(app_id.into()).await?)
            })
            .await??;
        let image = Bytes::from_owner(image);
        app.app_image_cache.write().await.insert(key, image.clone());
        Ok(image)
    }

    pub async fn cancel_app(&mut self) -> Result<bool, AppError> {
        let app = self.app.access()?;
        self.use_request_client(&app, async |_this, host| Ok(host.cancel().await?))
            .await?
    }
}
