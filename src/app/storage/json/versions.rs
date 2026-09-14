use std::collections::HashMap;

use anyhow::anyhow;
use log::error;
use moonlight_common::{
    crypto::rustcrypto::RustCryptoBackend, http::pair::PairingCryptoBackend, mac::MacAddress,
};
use pem::Pem;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::serde_helpers::{de_int_key, hex_array};

#[derive(Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum Json {
    #[serde(rename = "4")]
    V4(V4),
    #[serde(rename = "3")]
    V3(V3),
    #[serde(rename = "2")]
    V2(V2),
    #[serde(untagged)]
    V1(V1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V4 {
    pub client_unique_id: String,
    #[serde(deserialize_with = "de_int_key")]
    pub hosts: HashMap<u32, V4Host>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V4Host {
    pub address: String,
    pub http_port: u16,
    pub pair_info: Option<V2HostPairInfo>,
    pub cache: V2HostCache,
}

// -- V1

#[derive(Serialize, Deserialize)]
pub struct V1 {
    hosts: Vec<V1Host>,
}

#[derive(Serialize, Deserialize)]
pub struct V1Host {
    address: String,
    http_port: u16,
    #[serde(default)]
    cache: V1HostCache,
    paired: Option<V1HostPairInfo>,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct V1HostCache {
    pub name: Option<String>,
    pub mac: Option<MacAddress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V1HostPairInfo {
    pub client_private_key: String,
    pub client_certificate: String,
    pub server_certificate: String,
}

fn migrate_certificates_v1_to_v2(v1: V1HostPairInfo) -> Option<V2HostPairInfo> {
    Some(V2HostPairInfo {
        client_private_key: v1.client_private_key.parse().ok()?,
        client_certificate: v1.client_certificate.parse().ok()?,
        server_certificate: v1.server_certificate.parse().ok()?,
    })
}

pub fn migrate_v1_to_v2(old: V1) -> V2 {
    let mut hosts = HashMap::new();

    for (id, old_host) in old.hosts.into_iter().enumerate() {
        hosts.insert(
            id as u32,
            V2Host {
                owner: None,
                address: old_host.address,
                http_port: old_host.http_port,
                pair_info: old_host.paired.and_then(
                    |pair_info| match migrate_certificates_v1_to_v2(pair_info) {
                        Some(value) => Some(value),
                        None => {
                            error!("Migrating old pair data failed! Discarding this data!");
                            None
                        }
                    },
                ),
                cache: V2HostCache {
                    name: old_host.cache.name.unwrap_or_else(|| "Unknown".to_string()),
                    mac: old_host.cache.mac,
                },
            },
        );
    }

    V2 {
        users: Default::default(),
        hosts,
    }
}

// -- V2

#[derive(Serialize, Deserialize)]
pub struct V2 {
    #[serde(deserialize_with = "de_int_key")]
    pub users: HashMap<u32, V2User>,
    #[serde(deserialize_with = "de_int_key")]
    pub hosts: HashMap<u32, V2Host>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2User {
    pub role: LegacyRoleType,
    pub name: String,
    pub password: Option<V2UserPassword>,
    pub client_unique_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LegacyRoleType {
    User,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2UserPassword {
    #[serde(with = "hex_array")]
    pub salt: [u8; 16],
    #[serde(with = "hex_array")]
    pub hash: [u8; 32],
    #[serde(default = "default_legacy_iterations")]
    pub iterations: u32,
}

fn default_legacy_iterations() -> u32 {
    150_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2Host {
    pub owner: Option<u32>,
    pub address: String,
    pub http_port: u16,
    pub pair_info: Option<V2HostPairInfo>,
    pub cache: V2HostCache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2HostPairInfo {
    pub client_private_key: Pem,
    pub client_certificate: Pem,
    pub server_certificate: Pem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2HostCache {
    pub name: String,
    pub mac: Option<MacAddress>,
}

fn migrate_v2_to_v3(old: V2) -> V3 {
    const ADMIN_ID: u32 = 0;
    const USER_ID: u32 = 1;

    let mut roles = HashMap::new();
    roles.insert(
        ADMIN_ID,
        V3Role {
            name: "Admin".to_string(),
            ty: V3RoleType::Admin,
            default_settings: Default::default(),
            permissions: V3RolePermissions::default(),
        },
    );
    roles.insert(
        USER_ID,
        V3Role {
            name: "User".to_string(),
            ty: V3RoleType::User,
            default_settings: Default::default(),
            permissions: V3RolePermissions::default(),
        },
    );

    V3 {
        users: old
            .users
            .into_iter()
            .map(|(id, user)| {
                (
                    id,
                    V3User {
                        client_unique_id: user.client_unique_id,
                        name: user.name,
                        password: user.password,
                        role_id: match user.role {
                            LegacyRoleType::Admin => ADMIN_ID,
                            LegacyRoleType::User => USER_ID,
                        },
                    },
                )
            })
            .collect(),
        hosts: old.hosts,
        roles,
        default_role_id: None,
        default_user_id: None,
    }
}

// -- V3

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V3 {
    #[serde(deserialize_with = "de_int_key")]
    pub users: HashMap<u32, V3User>,
    #[serde(deserialize_with = "de_int_key")]
    pub hosts: HashMap<u32, V2Host>,
    #[serde(deserialize_with = "de_int_key")]
    pub roles: HashMap<u32, V3Role>,
    #[serde(default)]
    pub default_role_id: Option<u32>,
    #[serde(default)]
    pub default_user_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V3User {
    pub role_id: u32,
    pub name: String,
    pub password: Option<V2UserPassword>,
    pub client_unique_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum V3RoleType {
    User,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V3Role {
    pub name: String,
    pub ty: V3RoleType,
    pub default_settings: Value,
    pub permissions: V3RolePermissions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V3RolePermissions {
    pub allow_add_hosts: bool,
    pub maximum_bitrate_kbps: Option<u32>,
    pub allow_codec_h264: bool,
    pub allow_codec_h265: bool,
    pub allow_codec_av1: bool,
    pub allow_hdr: bool,
    pub allow_transport_webrtc: bool,
    pub allow_transport_websockets: bool,
}

impl Default for V3RolePermissions {
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

fn fresh_client_unique_id() -> Result<String, anyhow::Error> {
    let mut bytes = [0; 8];
    RustCryptoBackend
        .random_bytes(&mut bytes)
        .map_err(|err| anyhow!(err.to_string()))?;
    Ok(hex::encode(bytes))
}

fn client_unique_id_from_v3(data: &V3) -> Result<String, anyhow::Error> {
    data.users
        .iter()
        .filter(|(_, user)| {
            matches!(
                data.roles.get(&user.role_id).map(|role| &role.ty),
                Some(V3RoleType::Admin)
            )
        })
        .min_by_key(|(id, _)| *id)
        .or_else(|| data.users.iter().min_by_key(|(id, _)| *id))
        .map(|(_, user)| user.client_unique_id.clone())
        .map_or_else(fresh_client_unique_id, Ok)
}

pub fn migrate_v3_to_v4(old: V3) -> Result<V4, anyhow::Error> {
    Ok(V4 {
        client_unique_id: client_unique_id_from_v3(&old)?,
        hosts: old
            .hosts
            .into_iter()
            .map(|(id, host)| {
                (
                    id,
                    V4Host {
                        address: host.address,
                        http_port: host.http_port,
                        pair_info: host.pair_info,
                        cache: host.cache,
                    },
                )
            })
            .collect(),
    })
}

pub fn migrate_to_latest(json: Json) -> Result<V4, anyhow::Error> {
    match json {
        Json::V1(v1) => migrate_v3_to_v4(migrate_v2_to_v3(migrate_v1_to_v2(v1))),
        Json::V2(v2) => migrate_v3_to_v4(migrate_v2_to_v3(v2)),
        Json::V3(v3) => migrate_v3_to_v4(v3),
        Json::V4(v4) => Ok(v4),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_v3_to_v4_uses_lowest_admin_identity_and_keeps_hosts() {
        let mut roles = HashMap::new();
        roles.insert(
            1,
            V3Role {
                name: "Admin".into(),
                ty: V3RoleType::Admin,
                default_settings: Value::Null,
                permissions: V3RolePermissions::default(),
            },
        );
        roles.insert(
            2,
            V3Role {
                name: "User".into(),
                ty: V3RoleType::User,
                default_settings: Value::Null,
                permissions: V3RolePermissions::default(),
            },
        );

        let mut users = HashMap::new();
        users.insert(
            1,
            V3User {
                role_id: 1,
                name: "admin".into(),
                password: None,
                client_unique_id: "admin-identity".into(),
            },
        );
        users.insert(
            0,
            V3User {
                role_id: 2,
                name: "user".into(),
                password: None,
                client_unique_id: "user-identity".into(),
            },
        );

        let mut hosts = HashMap::new();
        hosts.insert(
            7,
            V2Host {
                owner: Some(1),
                address: "one".into(),
                http_port: 47989,
                pair_info: None,
                cache: V2HostCache {
                    name: "One".into(),
                    mac: None,
                },
            },
        );
        hosts.insert(
            9,
            V2Host {
                owner: Some(0),
                address: "two".into(),
                http_port: 47989,
                pair_info: None,
                cache: V2HostCache {
                    name: "Two".into(),
                    mac: None,
                },
            },
        );

        let migrated = migrate_v3_to_v4(V3 {
            users,
            hosts,
            roles,
            default_role_id: None,
            default_user_id: None,
        })
        .expect("migration should succeed");

        assert_eq!(migrated.client_unique_id, "admin-identity");
        assert_eq!(migrated.hosts.len(), 2);
        assert_eq!(migrated.hosts[&7].address, "one");
        assert_eq!(migrated.hosts[&9].address, "two");
    }
}
