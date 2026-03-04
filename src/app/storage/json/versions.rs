use std::collections::HashMap;

use log::error;
use moonlight_common::mac::MacAddress;
use pem::Pem;
use serde::{Deserialize, Serialize};

use crate::app::user::Role;

#[derive(Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum Json {
    #[serde(rename = "2")]
    V2(V2),
    #[serde(untagged)]
    V1(V1),
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
    let mut v2_hosts = HashMap::new();

    for (id, old_host) in old.hosts.into_iter().enumerate() {
        let v2_host = V2Host {
            owner: None,
            address: old_host.address,
            http_port: old_host.http_port,
            pair_info: old_host
                .paired
                .and_then(|v1| match migrate_certificates_v1_to_v2(v1) {
                    Some(value) => Some(value),
                    None => {
                        error!("Migrating old pair data failed! Discarding this data!");
                        None
                    }
                }),
            cache: V2HostCache {
                name: old_host.cache.name.unwrap_or_else(|| "Unknown".to_string()),
                mac: old_host.cache.mac,
            },
        };

        v2_hosts.insert(id as u32, v2_host);
    }

    V2 {
        users: Default::default(),
        hosts: v2_hosts,
    }
}

// -- V2

use crate::app::storage::json::serde_helpers::{de_int_key, hex_array};

#[derive(Serialize, Deserialize)]
pub struct V2 {
    #[serde(deserialize_with = "de_int_key")]
    pub users: HashMap<u32, V2User>,
    #[serde(deserialize_with = "de_int_key")]
    pub hosts: HashMap<u32, V2Host>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2User {
    pub role: Role,
    pub name: String,
    pub password: Option<V2UserPassword>,
    pub client_unique_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2UserPassword {
    #[serde(with = "hex_array")]
    pub salt: [u8; 16],
    #[serde(with = "hex_array")]
    pub hash: [u8; 32],
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

pub fn migrate_to_latest(json: Json) -> Result<V2, anyhow::Error> {
    match json {
        Json::V1(v1) => Ok(migrate_v1_to_v2(v1)),
        Json::V2(v2) => Ok(v2),
    }
}
