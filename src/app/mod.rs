use std::{
    collections::HashMap,
    io,
    ops::Deref,
    sync::{Arc, Weak},
    time::Instant,
};

use actix_web::{HttpResponse, ResponseError, body::BoxBody, http::StatusCode};
use hex::FromHexError;
use moonlight_common::{
    crypto::rustcrypto::{RustCryptoBackend, RustCryptoError},
    high::{MoonlightClientError, StreamConfigError},
    http::{
        ClientInfo,
        client::{RequestError, async_client::RequestClient as _, tokio_hyper::TokioHyperClient},
        pair::PairingCryptoBackend,
        server_info::{ServerInfoEndpoint, ServerInfoRequest},
    },
    stream::tokio::MoonlightStreamError,
    webrtc::WebRTCParseError,
};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use crate::{
    app::{
        host::{AppId, Host, HostId},
        storage::{Storage, StorageHostAdd, StorageHostCache, create_storage},
        stream::{Stream, StreamId},
    },
    config::Config,
};

pub mod host;
pub mod storage;
pub mod stream;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("the app got destroyed")]
    AppDestroyed,
    #[error("the host was not found")]
    HostNotFound,
    #[error("the host was already paired")]
    HostPaired,
    #[error("the host must be paired for this action")]
    HostNotPaired,
    #[error("the client doesn't support the required codecs")]
    WebRtcClientCodecNotSupported,
    #[error("the stream was already closed")]
    StreamClosed,
    #[error("the host doesn't support the given config: {0}")]
    StreamConfig(#[from] StreamConfigError),
    #[error("failed to parse the given sdp: {0}")]
    WebRTCParse(#[from] WebRTCParseError),
    #[error("rustcrypto error occured: {0}")]
    RustCrypto(#[from] RustCryptoError),
    #[error("hex error occured: {0}")]
    Hex(#[from] FromHexError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("moonlight error: {0}")]
    Moonlight(#[from] MoonlightClientError),
    #[error("moonlight error: {0}")]
    MoonlightStream(#[from] MoonlightStreamError),
    #[error("webrtc: {0}")]
    WebRTC(#[from] webrtc::error::Error),
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        self.error_response().status()
    }

    fn error_response(&self) -> HttpResponse<BoxBody> {
        match self {
            Self::HostNotFound => HttpResponse::NotFound().body("host not found"),
            Self::HostNotPaired => HttpResponse::Forbidden().finish(),
            Self::HostPaired => HttpResponse::NotModified().body("host already paired"),
            Self::StreamClosed => HttpResponse::NotFound().body("stream not found"),
            Self::WebRtcClientCodecNotSupported => HttpResponse::BadRequest().finish(),
            Self::WebRTCParse(_) => HttpResponse::BadRequest().finish(),
            _ => HttpResponse::InternalServerError().finish(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct AppRef {
    inner: Weak<AppInner>,
}

impl AppRef {
    pub(crate) fn access(&self) -> Result<impl Deref<Target = AppInner> + 'static, AppError> {
        self.inner.upgrade().ok_or(AppError::AppDestroyed)
    }
}

pub(crate) struct AppInner {
    config: Config,
    pub(crate) storage: Arc<dyn Storage + Send + Sync>,
    pub(crate) app_image_cache: RwLock<HashMap<(HostId, AppId), actix_web::web::Bytes>>,
    pub(crate) streams: RwLock<HashMap<StreamId, Stream>>,
    pub(crate) ice_server_script_cache:
        Mutex<Option<(Instant, Vec<crate::api::bindings::RtcIceServer>)>>,
}

pub type RequestClient = TokioHyperClient;

#[derive(Clone)]
pub struct App {
    pub(crate) inner: Arc<AppInner>,
}

impl App {
    pub async fn new(config: Config) -> Result<Self, anyhow::Error> {
        Ok(Self {
            inner: Arc::new(AppInner {
                storage: create_storage(config.data_storage.clone()).await?,
                config,
                app_image_cache: Default::default(),
                streams: Default::default(),
                ice_server_script_cache: Default::default(),
            }),
        })
    }

    pub(crate) fn new_ref(&self) -> AppRef {
        AppRef {
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    pub async fn client_unique_id(&self) -> Result<String, AppError> {
        self.inner.storage.client_unique_id().await
    }

    pub async fn hosts(&self) -> Result<Vec<Host>, AppError> {
        Ok(self
            .inner
            .storage
            .list_hosts()
            .await?
            .into_iter()
            .map(|host| Host {
                app: self.new_ref(),
                id: host.id,
                cache_storage: Some(host),
                cache_host_info: None,
            })
            .collect())
    }

    pub async fn host(&self, host_id: HostId) -> Result<Host, AppError> {
        let host = self.inner.storage.get_host(host_id).await?;
        Ok(Host {
            app: self.new_ref(),
            id: host.id,
            cache_storage: Some(host),
            cache_host_info: None,
        })
    }

    pub async fn host_add(&self, address: String, http_port: u16) -> Result<Host, AppError> {
        let unique_id = self.client_unique_id().await?;
        let client = RequestClient::with_defaults()
            .map_err(|err| MoonlightClientError::Backend(Box::new(err)))?;
        let info = match client
            .send_http::<ServerInfoEndpoint>(
                ClientInfo {
                    uuid: uuid::Uuid::new_v4(),
                    unique_id,
                },
                &format!("{address}:{http_port}"),
                &ServerInfoRequest {},
            )
            .await
        {
            Ok(info) => info,
            Err(err) if err.is_connect() => return Err(AppError::HostNotFound),
            Err(err) => return Err(MoonlightClientError::Backend(Box::new(err)).into()),
        };
        let host = self
            .inner
            .storage
            .add_host(StorageHostAdd {
                address,
                http_port,
                pair_info: None,
                cache: StorageHostCache {
                    name: info.host_name,
                    mac: info.mac,
                },
            })
            .await?;
        Ok(Host {
            app: self.new_ref(),
            id: host.id,
            cache_storage: Some(host),
            cache_host_info: None,
        })
    }

    pub async fn host_delete(&self, host_id: HostId) -> Result<(), AppError> {
        self.inner.storage.remove_host(host_id).await?;
        let mut images = self.inner.app_image_cache.write().await;
        images.retain(|(id, _), _| *id != host_id);
        Ok(())
    }

    pub(crate) async fn insert_stream(
        &self,
        f: impl FnOnce(StreamId) -> Stream,
    ) -> Result<Stream, AppError> {
        let mut streams = self.inner.streams.write().await;
        let mut id = StreamId(0);
        while streams.contains_key(&id) {
            let mut random = [0; 4];
            RustCryptoBackend.random_bytes(&mut random)?;
            id = StreamId(u32::from_be_bytes(random));
        }
        let stream = f(id);
        streams.insert(id, stream.clone());
        Ok(stream)
    }

    pub async fn stream_by_id(&self, id: StreamId) -> Result<Stream, AppError> {
        self.inner
            .streams
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or(AppError::StreamClosed)
    }
}
