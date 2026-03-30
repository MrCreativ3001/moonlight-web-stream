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
use tracing::{debug, error};

use crate::app::{
    AppError,
    auth::SessionToken,
    host::HostId,
    password::StoragePassword,
    role::RoleId,
    storage::{
        Either, Storage, StorageHost, StorageHostAdd, StorageHostCache, StorageHostModify,
        StorageHostPairInfo, StorageQueryHosts, StorageRole, StorageRoleAdd,
        StorageRoleDefaultSettings, StorageRoleModify, StorageRolePermissions, StorageUser,
        StorageUserAdd, StorageUserModify,
        json::versions::{
            Json, V2, V2Host, V2HostCache, V2HostPairInfo, V2UserPassword, V3, V3Role,
            V3RolePermissions, V3RoleSettings, V3RoleType, V3User, migrate_to_latest,
        },
    },
    user::{RoleType, UserId},
};

mod serde_helpers;
mod versions;

pub struct JsonStorage {
    file: PathBuf,
    store_sender: Sender<()>,
    session_expiration_checker: JoinHandle<()>,
    // IMPORTANT: only lock those mutexes in descending order to prevent deadlocks
    users: RwLock<HashMap<u32, RwLock<V3User>>>,
    hosts: RwLock<HashMap<u32, RwLock<V2Host>>>,
    roles: RwLock<HashMap<u32, RwLock<V3Role>>>,
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
            roles: Default::default(),
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
            let mut roles = self.roles.write().await;

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
            *roles = data
                .roles
                .into_iter()
                .map(|(id, role)| (id, RwLock::new(role)))
                .collect();
        }

        Ok(())
    }
    async fn store(&self) {
        let json = {
            let users = self.users.read().await;
            let hosts = self.hosts.read().await;
            let roles = self.roles.read().await;

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

            let mut roles_json = HashMap::new();
            for (key, value) in roles.iter() {
                let value = value.read().await;

                roles_json.insert(*key, (*value).clone());
            }

            Json::V3(V3 {
                users: users_json,
                hosts: hosts_json,
                roles: roles_json,
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

fn default_settings_from_json(settings: V3RoleSettings) -> StorageRoleDefaultSettings {
    StorageRoleDefaultSettings {
        bitrate_kpbs: settings.bitrate_kpbs,
        width: settings.width,
        height: settings.height,
        fps: settings.fps,
        play_audio_local: settings.play_audio_local,
        supported_codecs: settings.supported_codecs,
        hdr: settings.hdr,
    }
}
fn default_settings_to_json(settings: StorageRoleDefaultSettings) -> V3RoleSettings {
    V3RoleSettings {
        bitrate_kpbs: settings.bitrate_kpbs,
        width: settings.width,
        height: settings.height,
        fps: settings.fps,
        play_audio_local: settings.play_audio_local,
        supported_codecs: settings.supported_codecs,
        hdr: settings.hdr,
    }
}

fn permissions_from_json(permissions: V3RolePermissions) -> StorageRolePermissions {
    StorageRolePermissions {
        allow_add_hosts: permissions.allow_add_hosts,
        maximum_bitrate_kbps: permissions.maximum_bitrate_kbps,
        allow_codec_h264: permissions.allow_codec_h264,
        allow_codec_h265: permissions.allow_codec_h265,
        allow_codec_av1: permissions.allow_codec_av1,
        allow_hdr: permissions.allow_hdr,
        allow_transport_webrtc: permissions.allow_transport_webrtc,
        allow_transport_websockets: permissions.allow_transport_websockets,
    }
}
fn permissions_to_json(permissions: StorageRolePermissions) -> V3RolePermissions {
    V3RolePermissions {
        allow_add_hosts: permissions.allow_add_hosts,
        maximum_bitrate_kbps: permissions.maximum_bitrate_kbps,
        allow_codec_h264: permissions.allow_codec_h264,
        allow_codec_h265: permissions.allow_codec_h265,
        allow_codec_av1: permissions.allow_codec_av1,
        allow_hdr: permissions.allow_hdr,
        allow_transport_webrtc: permissions.allow_transport_webrtc,
        allow_transport_websockets: permissions.allow_transport_websockets,
    }
}

fn role_from_json(role_id: RoleId, role: &V3Role) -> StorageRole {
    StorageRole {
        id: role_id,
        name: role.name.clone(),
        ty: match role.ty {
            V3RoleType::Admin => RoleType::Admin,
            V3RoleType::User => RoleType::User,
        },
        default_settings: default_settings_from_json(role.default_settings.clone()),
        permissions: permissions_from_json(role.permissions.clone()),
    }
}

fn user_from_json(user_id: UserId, user: &V3User) -> StorageUser {
    StorageUser {
        id: user_id,
        name: user.name.clone(),
        password: user.password.as_ref().map(|password| StoragePassword {
            salt: password.salt,
            hash: password.hash,
        }),
        role_id: RoleId(user.role_id),
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

fn random_number() -> Result<u32, AppError> {
    let mut id_bytes = [0u8; 4];
    rand_bytes(&mut id_bytes)?;
    Ok(u32::from_be_bytes(id_bytes))
}

#[async_trait]
impl Storage for JsonStorage {
    async fn add_role(&self, role: StorageRoleAdd) -> Result<StorageRole, AppError> {
        let role = V3Role {
            ty: match role.ty {
                RoleType::Admin => V3RoleType::Admin,
                RoleType::User => V3RoleType::User,
            },
            name: role.name,
            default_settings: default_settings_to_json(role.default_settings),
            permissions: permissions_to_json(role.permissions),
        };

        let mut roles = self.roles.write().await;

        let mut id;
        loop {
            id = random_number()?;

            if !roles.contains_key(&id) {
                break;
            }
        }
        roles.insert(id, RwLock::new(role.clone()));

        drop(roles);

        self.force_write();

        Ok(StorageRole {
            ty: match role.ty {
                V3RoleType::Admin => RoleType::Admin,
                V3RoleType::User => RoleType::User,
            },
            id: RoleId(id),
            name: role.name,
            default_settings: default_settings_from_json(role.default_settings),
            permissions: permissions_from_json(role.permissions),
        })
    }
    async fn modify_role(
        &self,
        role_id: RoleId,
        modify: StorageRoleModify,
    ) -> Result<(), AppError> {
        let roles = self.roles.read().await;

        let role_lock = roles.get(&role_id.0).ok_or(AppError::RoleNotFound)?;
        let mut role = role_lock.write().await;

        if let Some(name) = modify.name {
            role.name = name;
        }
        if let Some(ty) = modify.ty {
            role.ty = match ty {
                RoleType::Admin => V3RoleType::Admin,
                RoleType::User => V3RoleType::User,
            };
        }
        if let Some(StorageRoleDefaultSettings {
            bitrate_kpbs,
            width,
            height,
            fps,
            play_audio_local,
            supported_codecs,
            hdr,
        }) = modify.default_settings
        {
            role.default_settings.bitrate_kpbs = bitrate_kpbs;
            role.default_settings.width = width;
            role.default_settings.height = height;
            role.default_settings.fps = fps;
            role.default_settings.play_audio_local = play_audio_local;
            role.default_settings.supported_codecs = supported_codecs;
            role.default_settings.hdr = hdr;
        }
        if let Some(StorageRolePermissions {
            allow_add_hosts,
            maximum_bitrate_kbps,
            allow_codec_h264,
            allow_codec_h265,
            allow_codec_av1,
            allow_hdr,
            allow_transport_webrtc,
            allow_transport_websockets,
        }) = modify.permissions
        {
            role.permissions.allow_add_hosts = allow_add_hosts;
            role.permissions.maximum_bitrate_kbps = maximum_bitrate_kbps;
            role.permissions.allow_codec_h264 = allow_codec_h264;
            role.permissions.allow_codec_h265 = allow_codec_h265;
            role.permissions.allow_codec_av1 = allow_codec_av1;
            role.permissions.allow_hdr = allow_hdr;
            role.permissions.allow_transport_webrtc = allow_transport_webrtc;
            role.permissions.allow_transport_websockets = allow_transport_websockets;
        }

        drop(role);
        drop(roles);

        self.force_write();

        Ok(())
    }
    async fn get_role(&self, role_id: RoleId) -> Result<StorageRole, AppError> {
        let roles = self.roles.read().await;

        let role_lock = roles.get(&role_id.0).ok_or(AppError::RoleNotFound)?;
        let role = role_lock.read().await;

        Ok(role_from_json(role_id, &role))
    }
    async fn remove_role(&self, role_id: RoleId) -> Result<(), AppError> {
        // Delete all users with that role
        {
            let mut users = self.users.write().await;

            let mut users_to_remove = vec![];

            // Find all users with that role
            for (user_id, user) in users.iter() {
                let user = user.read().await;

                if user.role_id == role_id.0 {
                    users_to_remove.push(*user_id);
                }
            }

            // Remove all user id's in that list
            for user_id in users_to_remove {
                users.remove(&user_id);
            }
        }

        // Delete that role
        let result = {
            let mut roles = self.roles.write().await;

            let result = match roles.remove(&role_id.0) {
                None => Err(AppError::RoleNotFound),
                Some(_) => Ok(()),
            };

            drop(roles);

            result
        };

        self.force_write();

        result
    }
    async fn list_roles(&self) -> Result<Either<Vec<RoleId>, Vec<StorageRole>>, AppError> {
        let roles = self.roles.read().await;

        let futures = roles.iter().map(|(id, value)| {
            let id = *id;
            async move {
                let role = value.read().await.clone();
                role_from_json(RoleId(id), &role)
            }
        });

        let out = join_all(futures).await;
        Ok(Either::Right(out))
    }

    async fn add_user(&self, user: StorageUserAdd) -> Result<StorageUser, AppError> {
        let user = V3User {
            role_id: user.role_id.0,
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
            id = random_number()?;

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
            role_id: RoleId(user.role_id),
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
        if let Some(role_id) = modify.role_id {
            user.role_id = role_id.0;
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
            id = random_number()?;

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
