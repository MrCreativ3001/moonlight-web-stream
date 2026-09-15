use std::{collections::HashMap, io::ErrorKind, path::PathBuf, sync::Arc};

use anyhow::anyhow;
use async_trait::async_trait;
use moonlight_common::{crypto::rustcrypto::RustCryptoBackend, http::pair::PairingCryptoBackend};
use tokio::{
    fs, spawn,
    sync::{
        RwLock,
        mpsc::{self, Receiver, Sender, error::TrySendError},
    },
};
use tracing::error;

use crate::app::{
    AppError,
    host::HostId,
    storage::{
        Storage, StorageHost, StorageHostAdd, StorageHostCache, StorageHostModify,
        StorageHostPairInfo,
        json::versions::{Json, V2HostCache, V2HostPairInfo, V4, V4Host, migrate_to_latest},
    },
};

mod serde_helpers;
mod versions;

pub struct JsonStorage {
    file: PathBuf,
    store_sender: Sender<()>,
    client_unique_id: RwLock<String>,
    hosts: RwLock<HashMap<u32, V4Host>>,
}

impl JsonStorage {
    pub async fn load(file: PathBuf) -> Result<Arc<Self>, anyhow::Error> {
        let (store_sender, store_receiver) = mpsc::channel(1);
        let this = Arc::new(Self {
            file,
            store_sender,
            client_unique_id: RwLock::new(fresh_client_unique_id()?),
            hosts: Default::default(),
        });

        this.load_internal().await?;
        spawn({
            let this = this.clone();
            async move { file_writer(store_receiver, this).await }
        });
        Ok(this)
    }

    pub fn force_write(&self) {
        if let Err(TrySendError::Closed(_)) = self.store_sender.try_send(()) {
            error!("Failed to save data because the writer task closed!");
        }
    }

    async fn load_internal(&self) -> Result<(), anyhow::Error> {
        let text = match fs::read_to_string(&self.file).await {
            Ok(text) => text,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(anyhow!("Failed to read data: {err:?}")),
        };
        let json = serde_json::from_str::<Json>(&text)
            .map_err(|err| anyhow!("Failed to deserialize data as json: {err}"))?;
        let data = migrate_to_latest(json)?;
        *self.client_unique_id.write().await = data.client_unique_id;
        *self.hosts.write().await = data.hosts;
        Ok(())
    }

    async fn store(&self) {
        let json = Json::V4(V4 {
            client_unique_id: self.client_unique_id.read().await.clone(),
            hosts: self.hosts.read().await.clone(),
        });
        let text = match serde_json::to_string_pretty(&json) {
            Ok(text) => text,
            Err(err) => {
                error!("Failed to serialize data to json: {err:?}");
                return;
            }
        };
        if let Some(parent) = self.file.parent()
            && let Err(err) = fs::create_dir_all(parent).await
        {
            error!(error = %err, "Failed to create directory for data");
        }
        if let Err(err) = fs::write(&self.file, text).await {
            error!(error = %err, "Failed to write data to file");
        }
    }
}

async fn file_writer(mut receiver: Receiver<()>, storage: Arc<JsonStorage>) {
    while receiver.recv().await.is_some() {
        storage.store().await;
    }
}

fn fresh_client_unique_id() -> Result<String, anyhow::Error> {
    let mut bytes = [0; 8];
    RustCryptoBackend
        .random_bytes(&mut bytes)
        .map_err(|err| anyhow!(err.to_string()))?;
    Ok(hex::encode(bytes))
}

fn host_from_json(host_id: HostId, host: &V4Host) -> StorageHost {
    StorageHost {
        id: host_id,
        address: host.address.clone(),
        http_port: host.http_port,
        pair_info: host.pair_info.clone().map(|pair_info| StorageHostPairInfo {
            client_certificate: pair_info.client_certificate,
            client_private_key: pair_info.client_private_key,
            server_certificate: pair_info.server_certificate,
        }),
        cache: StorageHostCache {
            name: host.cache.name.clone(),
            mac: host.cache.mac,
        },
    }
}

fn pair_to_json(pair_info: StorageHostPairInfo) -> V2HostPairInfo {
    V2HostPairInfo {
        client_private_key: pair_info.client_private_key,
        client_certificate: pair_info.client_certificate,
        server_certificate: pair_info.server_certificate,
    }
}

fn host_to_json(host: StorageHostAdd) -> V4Host {
    V4Host {
        address: host.address,
        http_port: host.http_port,
        pair_info: host.pair_info.map(pair_to_json),
        cache: V2HostCache {
            name: host.cache.name,
            mac: host.cache.mac,
        },
    }
}

fn random_number() -> Result<u32, AppError> {
    let mut bytes = [0; 4];
    RustCryptoBackend.random_bytes(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

#[async_trait]
impl Storage for JsonStorage {
    async fn client_unique_id(&self) -> Result<String, AppError> {
        Ok(self.client_unique_id.read().await.clone())
    }

    async fn add_host(&self, host: StorageHostAdd) -> Result<StorageHost, AppError> {
        let host = host_to_json(host);
        let mut hosts = self.hosts.write().await;
        let id = loop {
            let id = random_number()?;
            if !hosts.contains_key(&id) {
                break id;
            }
        };
        hosts.insert(id, host.clone());
        drop(hosts);
        self.force_write();
        Ok(host_from_json(HostId(id), &host))
    }

    async fn modify_host(
        &self,
        host_id: HostId,
        modify: StorageHostModify,
    ) -> Result<(), AppError> {
        let mut hosts = self.hosts.write().await;
        let host = hosts.get_mut(&host_id.0).ok_or(AppError::HostNotFound)?;
        if let Some(address) = modify.address {
            host.address = address;
        }
        if let Some(http_port) = modify.http_port {
            host.http_port = http_port;
        }
        if let Some(pair_info) = modify.pair_info {
            host.pair_info = pair_info.map(pair_to_json);
        }
        if let Some(name) = modify.cache_name {
            host.cache.name = name;
        }
        if let Some(mac) = modify.cache_mac {
            host.cache.mac = mac;
        }
        drop(hosts);
        self.force_write();
        Ok(())
    }

    async fn get_host(&self, host_id: HostId) -> Result<StorageHost, AppError> {
        let hosts = self.hosts.read().await;
        let host = hosts.get(&host_id.0).ok_or(AppError::HostNotFound)?;
        Ok(host_from_json(host_id, host))
    }

    async fn remove_host(&self, host_id: HostId) -> Result<(), AppError> {
        let mut hosts = self.hosts.write().await;
        if hosts.remove(&host_id.0).is_none() {
            return Err(AppError::HostNotFound);
        }
        drop(hosts);
        self.force_write();
        Ok(())
    }

    async fn list_hosts(&self) -> Result<Vec<StorageHost>, AppError> {
        let hosts = self.hosts.read().await;
        Ok(hosts
            .iter()
            .map(|(id, host)| host_from_json(HostId(*id), host))
            .collect())
    }
}
