use std::{
    collections::HashMap,
    io::ErrorKind,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use async_trait::async_trait;
use futures::future::join_all;
use log::{debug, error};
use openssl::rand::rand_bytes;
use tokio::{
    fs, spawn,
    sync::{
        RwLock,
        mpsc::{self, Receiver, Sender, error::TrySendError},
        oneshot,
    },
    task::JoinHandle,
    time::sleep,
};

use crate::app::{
    AppError,
    auth::SessionToken,
    host::HostId,
    password::StoragePassword,
    storage::{
        Either, Storage, StorageHost, StorageHostAdd, StorageHostCache, StorageHostModify,
        StorageHostPairInfo, StorageQueryHosts, StorageUser, StorageUserAdd, StorageUserModify,
        json::versions::{
            Json, V2, V2Host, V2HostCache, V2HostPairInfo, V2User, V2UserPassword,
            migrate_to_latest,
        },
    },
    user::UserId,
};

mod serde_helpers;
mod versions;

pub struct JsonStorage {
    file: PathBuf,
    store_sender: Sender<()>,
    session_expiration_checker: JoinHandle<()>,
    users: RwLock<HashMap<u32, RwLock<V2User>>>,
    hosts: RwLock<HashMap<u32, RwLock<V2Host>>>,
    sessions: RwLock<HashMap<SessionToken, Session>>,
}

impl Drop for JsonStorage {
    fn drop(&mut self) {
        self.session_expiration_checker.abort();
    }
}

struct Session {
    created_at: Instant,
    expiration: Duration,
    user_id: u32,
}

impl JsonStorage {
    pub async fn load(
        file: PathBuf,
        session_expiration_check_interval: Duration,
    ) -> Result<Arc<Self>, anyhow::Error> {
        let (store_sender, store_receiver) = mpsc::channel(1);

        let (this_sender, this_receiver) = oneshot::channel::<Arc<Self>>();

        let session_expiration_checker = spawn(async move {
            let this = match this_receiver.await {
                Ok(value) => value,
                Err(err) => {
                    error!(
                        "Failed to initialize session expiration checker: {err:?}. All sessions will last forever!"
                    );
                    return;
                }
            };

            loop {
                sleep(session_expiration_check_interval).await;
                debug!("Clearing all expired sessions!");

                let mut sessions = this.sessions.write().await;

                let now = Instant::now();
                sessions.retain(|_, session| {
                    let current_session_length = now - session.created_at;

                    current_session_length < session.expiration
                });
            }
        });

        let this = Self {
            file,
            store_sender,
            session_expiration_checker,
            hosts: Default::default(),
            users: Default::default(),
            sessions: Default::default(),
        };
        let this = Arc::new(this);

        if this_sender.send(this.clone()).is_err() {
            error!(
                "Failed to send values to session expiration checker. All sessions will last forever!"
            );
        }

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
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return Ok(());
            }
            Err(err) => {
                return Err(anyhow!("Failed to read data: {err:?}"));
            }
        };

        let json = match serde_json::from_str::<Json>(&text) {
            Ok(value) => value,
            Err(err) => {
                let error = serde_json::from_str::<V2>(&text)
                    .err()
                    .map(|x| x.to_string())
                    .unwrap_or("none".to_string());

                return Err(anyhow!(
                    "Failed to deserialize data as json: {err}, Version specific error: {error}"
                ));
            }
        };

        let data = migrate_to_latest(json)?;

        {
            let mut users = self.users.write().await;
            let mut hosts = self.hosts.write().await;

            *users = data
                .users
                .into_iter()
                .map(|(id, user)| (id, RwLock::new(user)))
                .collect();
            *hosts = data
                .hosts
                .into_iter()
                .map(|(id, host)| (id, RwLock::new(host)))
                .collect();
        }

        Ok(())
    }
    async fn store(&self) {
        let json = {
            let users = self.users.read().await;
            let hosts = self.hosts.read().await;

            let mut users_json = HashMap::new();
            for (key, value) in users.iter() {
                let value = value.read().await;

                users_json.insert(*key, (*value).clone());
            }

            let mut hosts_json = HashMap::new();
            for (key, value) in hosts.iter() {
                let value = value.read().await;

                hosts_json.insert(*key, (*value).clone());
            }

            Json::V2(V2 {
                users: users_json,
                hosts: hosts_json,
            })
        };

        let text = match serde_json::to_string_pretty(&json) {
            Ok(text) => text,
            Err(err) => {
                error!("Failed to serialize data to json: {err:?}");
                return;
            }
        };

        if let Err(err) = fs::write(&self.file, text).await {
            error!("Failed to write data to file: {err:?}");
        }
    }
}

async fn file_writer(mut store_receiver: Receiver<()>, json: Arc<JsonStorage>) {
    loop {
        if store_receiver.recv().await.is_none() {
            return;
        }

        json.store().await;
    }
}

fn user_from_json(user_id: UserId, user: &V2User) -> StorageUser {
    StorageUser {
        id: user_id,
        name: user.name.clone(),
        password: user.password.as_ref().map(|password| StoragePassword {
            salt: password.salt,
            hash: password.hash,
        }),
        role: user.role,
        client_unique_id: user.client_unique_id.clone(),
    }
}

fn host_from_json(host_id: HostId, host: &V2Host) -> StorageHost {
    StorageHost {
        id: host_id,
        owner: host.owner.map(UserId),
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

#[async_trait]
impl Storage for JsonStorage {
    async fn add_user(&self, user: StorageUserAdd) -> Result<StorageUser, AppError> {
        let user = V2User {
            role: user.role,
            name: user.name,
            password: user.password.map(|password| V2UserPassword {
                salt: password.salt,
                hash: password.hash,
            }),
            client_unique_id: user.client_unique_id,
        };

        {
            match self.get_user_by_name(&user.name).await {
                Err(AppError::UserNotFound) => {
                    // Fallthrough
                }
                Ok(_) => return Err(AppError::UserAlreadyExists),
                Err(err) => return Err(err),
            }
        }

        let mut users = self.users.write().await;

        let mut id;
        loop {
            let mut id_bytes = [0u8; 4];
            rand_bytes(&mut id_bytes)?;
            id = u32::from_be_bytes(id_bytes);

            if !users.contains_key(&id) {
                break;
            }
        }
        users.insert(id, RwLock::new(user.clone()));

        drop(users);

        self.force_write();

        Ok(StorageUser {
            id: UserId(id),
            name: user.name,
            password: user.password.map(|password| StoragePassword {
                salt: password.salt,
                hash: password.hash,
            }),
            role: user.role,
            client_unique_id: user.client_unique_id,
        })
    }
    async fn modify_user(
        &self,
        user_id: UserId,
        modify: StorageUserModify,
    ) -> Result<(), AppError> {
        let users = self.users.read().await;

        let user_lock = users.get(&user_id.0).ok_or(AppError::UserNotFound)?;
        let mut user = user_lock.write().await;

        if let Some(password) = modify.password {
            user.password = password.map(|password| V2UserPassword {
                salt: password.salt,
                hash: password.hash,
            });
        }
        if let Some(role) = modify.role {
            user.role = role;
        }
        if let Some(client_unique_id) = modify.client_unique_id {
            user.client_unique_id = client_unique_id;
        }

        drop(user);
        drop(users);

        self.force_write();

        Ok(())
    }
    async fn get_user(&self, user_id: UserId) -> Result<StorageUser, AppError> {
        let users = self.users.read().await;

        let user_lock = users.get(&user_id.0).ok_or(AppError::UserNotFound)?;
        let user = user_lock.read().await;

        Ok(user_from_json(user_id, &user))
    }
    async fn get_user_by_name(
        &self,
        name: &str,
    ) -> Result<(UserId, Option<StorageUser>), AppError> {
        let users = self.users.read().await;

        let results = join_all(users.iter().map(|(user_id, user)| async move {
            let user = user.read().await;

            let user_id = UserId(*user_id);
            let user = (user.name == name).then(|| user_from_json(user_id, &user));

            (user_id, user)
        }))
        .await;

        let user = results.into_iter().find(|(_, user)| user.is_some());

        user.ok_or(AppError::UserNotFound)
    }
    async fn remove_user(&self, user_id: UserId) -> Result<(), AppError> {
        let mut users = self.users.write().await;

        let result = match users.remove(&user_id.0) {
            None => Err(AppError::UserNotFound),
            Some(_) => Ok(()),
        };

        drop(users);

        self.force_write();

        result
    }
    async fn list_users(&self) -> Result<Either<Vec<UserId>, Vec<StorageUser>>, AppError> {
        let users = self.users.read().await;

        let futures = users.iter().map(|(id, value)| {
            let id = *id;
            async move {
                let user = value.read().await.clone();
                user_from_json(UserId(id), &user)
            }
        });

        let out = join_all(futures).await;
        Ok(Either::Right(out))
    }
    async fn any_user_exists(&self) -> Result<bool, AppError> {
        let users = self.users.read().await;

        Ok(!users.is_empty())
    }

    async fn create_session_token(
        &self,
        user_id: UserId,
        expiration: Duration,
    ) -> Result<SessionToken, AppError> {
        let mut token;
        {
            let sessions = self.sessions.read().await;

            loop {
                token = SessionToken::new()?;
                if !sessions.contains_key(&token) {
                    break;
                }
            }
        };

        let mut sessions = self.sessions.write().await;

        sessions.insert(
            token,
            Session {
                created_at: Instant::now(),
                expiration,
                user_id: user_id.0,
            },
        );

        Ok(token)
    }
    async fn remove_session_token(&self, session: SessionToken) -> Result<(), AppError> {
        let mut sessions = self.sessions.write().await;

        sessions.remove(&session);

        Ok(())
    }
    async fn remove_all_user_session_tokens(&self, user_id: UserId) -> Result<(), AppError> {
        let mut sessions = self.sessions.write().await;

        sessions.retain(|_, session| UserId(session.user_id) != user_id);

        Ok(())
    }
    async fn get_user_by_session_token(
        &self,
        session: SessionToken,
    ) -> Result<(UserId, Option<StorageUser>), AppError> {
        let sessions = self.sessions.read().await;

        sessions
            .get(&session)
            .map(|session| (UserId(session.user_id), None))
            .ok_or(AppError::SessionTokenNotFound)
    }

    async fn add_host(&self, host: StorageHostAdd) -> Result<StorageHost, AppError> {
        let host = V2Host {
            owner: host.owner.map(|user_id| user_id.0),
            address: host.address,
            http_port: host.http_port,
            pair_info: host.pair_info.map(|pair_info| V2HostPairInfo {
                client_private_key: pair_info.client_private_key,
                client_certificate: pair_info.client_certificate,
                server_certificate: pair_info.server_certificate,
            }),
            cache: V2HostCache {
                name: host.cache.name,
                mac: host.cache.mac,
            },
        };

        let mut hosts = self.hosts.write().await;

        let mut id;
        loop {
            let mut id_bytes = [0u8; 4];
            rand_bytes(&mut id_bytes)?;
            id = u32::from_be_bytes(id_bytes);

            if !hosts.contains_key(&id) {
                break;
            }
        }
        hosts.insert(id, RwLock::new(host.clone()));

        self.force_write();

        Ok(StorageHost {
            id: HostId(id),
            owner: host.owner.map(UserId),
            address: host.address,
            http_port: host.http_port,
            pair_info: host.pair_info.map(|pair_info| StorageHostPairInfo {
                client_private_key: pair_info.client_private_key,
                client_certificate: pair_info.client_certificate,
                server_certificate: pair_info.server_certificate,
            }),
            cache: StorageHostCache {
                name: host.cache.name,
                mac: host.cache.mac,
            },
        })
    }
    async fn modify_host(
        &self,
        host_id: HostId,
        modify: StorageHostModify,
    ) -> Result<(), AppError> {
        let hosts = self.hosts.read().await;

        let host = hosts.get(&host_id.0).ok_or(AppError::HostNotFound)?;
        let mut host = host.write().await;

        if let Some(new_owner) = modify.owner {
            host.owner = new_owner.map(|user_id| user_id.0);
        }
        if let Some(new_address) = modify.address {
            host.address = new_address;
        }
        if let Some(new_http_port) = modify.http_port {
            host.http_port = new_http_port;
        }
        if let Some(new_pair_info) = modify.pair_info {
            host.pair_info = new_pair_info.map(|new_pair_info| V2HostPairInfo {
                client_private_key: new_pair_info.client_private_key,
                client_certificate: new_pair_info.client_certificate,
                server_certificate: new_pair_info.server_certificate,
            });
        }
        if let Some(new_cache_name) = modify.cache_name {
            host.cache.name = new_cache_name;
        }
        if let Some(new_cache_mac) = modify.cache_mac {
            host.cache.mac = new_cache_mac;
        }

        self.force_write();

        Ok(())
    }
    async fn get_host(&self, host_id: HostId) -> Result<StorageHost, AppError> {
        let hosts = self.hosts.read().await;

        let host = hosts.get(&host_id.0).ok_or(AppError::HostNotFound)?;
        let host = host.read().await;

        Ok(host_from_json(host_id, &host))
    }
    async fn remove_host(&self, host_id: HostId) -> Result<(), AppError> {
        let mut hosts = self.hosts.write().await;

        if hosts.remove(&host_id.0).is_none() {
            return Err(AppError::HostNotFound);
        }

        self.force_write();

        Ok(())
    }

    async fn list_user_hosts(
        &self,
        query: StorageQueryHosts,
    ) -> Result<Vec<(HostId, Option<StorageHost>)>, AppError> {
        let hosts = self.hosts.read().await;

        let mut user_hosts = Vec::new();
        for (host_id, host) in &*hosts {
            let host_id = HostId(*host_id);
            let host = host.read().await;

            if host.owner.is_none() || host.owner.map(UserId) == Some(query.user_id) {
                user_hosts.push((host_id, Some(host_from_json(host_id, &host))));
            }
        }

        Ok(user_hosts)
    }
}
