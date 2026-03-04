use std::{
    fmt::Display,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    num::ParseIntError,
    str::FromStr,
    time::Duration,
};

use log::LevelFilter;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::api_bindings::RtcIceServer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub data_storage: StorageConfig,
    #[serde(default)]
    pub webrtc: WebRtcConfig,
    #[serde(default)]
    pub web_server: WebServerConfig,
    #[serde(default)]
    pub moonlight: MoonlightConfig,
    #[serde(default = "default_streamer_path")]
    pub streamer_path: String,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub default_settings: Option<Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_storage: Default::default(),
            streamer_path: default_streamer_path(),
            web_server: Default::default(),
            moonlight: Default::default(),
            webrtc: Default::default(),
            log: Default::default(),
            default_settings: Default::default(),
        }
    }
}

// -- Log

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    pub level_filter: LevelFilter,
    pub file_path: Option<String>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level_filter: default_level_filter(),
            file_path: None,
        }
    }
}

fn default_level_filter() -> LevelFilter {
    LevelFilter::Info
}

// -- Data Storage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum StorageConfig {
    Json {
        path: String,
        session_expiration_check_interval: Duration,
    },
}

impl Default for StorageConfig {
    fn default() -> Self {
        StorageConfig::Json {
            path: "server/data.json".to_string(),
            session_expiration_check_interval: default_session_expiration_check_interval(),
        }
    }
}

fn default_session_expiration_check_interval() -> Duration {
    Duration::from_mins(5)
}

// -- WebRTC Config

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcConfig {
    #[serde(default = "default_ice_servers")]
    pub ice_servers: Vec<RtcIceServer>,
    #[serde(default)]
    pub port_range: Option<PortRange>,
    #[serde(default)]
    pub nat_1to1: Option<WebRtcNat1To1Mapping>,
    #[serde(default = "default_network_types")]
    pub network_types: Vec<WebRtcNetworkType>,
    #[serde(default = "default_include_loopback_candidates")]
    pub include_loopback_candidates: bool,
}

impl Default for WebRtcConfig {
    fn default() -> Self {
        Self {
            ice_servers: default_ice_servers(),
            port_range: None,
            nat_1to1: None,
            network_types: default_network_types(),
            include_loopback_candidates: default_include_loopback_candidates(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WebRtcNetworkType {
    #[serde(rename = "udp4")]
    Udp4,
    #[serde(rename = "udp6")]
    Udp6,
    #[serde(rename = "tcp4")]
    Tcp4,
    #[serde(rename = "tcp6")]
    Tcp6,
}

impl Display for WebRtcNetworkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ty = match self {
            Self::Udp4 => "udp4",
            Self::Udp6 => "udp6",
            Self::Tcp4 => "tcp4",
            Self::Tcp6 => "tcp6",
        };
        write!(f, "{}", ty)
    }
}

#[derive(Debug, Error)]
#[error("not a valid network type")]
pub struct WebRtcNetworkTypeFromStr;

impl FromStr for WebRtcNetworkType {
    type Err = WebRtcNetworkTypeFromStr;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "udp4" => Ok(Self::Udp4),
            "udp6" => Ok(Self::Udp6),
            "tcp4" => Ok(Self::Tcp4),
            "tcp6" => Ok(Self::Tcp6),
            _ => Err(WebRtcNetworkTypeFromStr),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcNat1To1Mapping {
    pub ips: Vec<String>,
    pub ice_candidate_type: WebRtcNat1To1IceCandidateType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WebRtcNat1To1IceCandidateType {
    #[serde(rename = "srflx")]
    Srflx,
    #[serde(rename = "host")]
    Host,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRange {
    pub min: u16,
    pub max: u16,
}

#[derive(Debug, Error)]
pub enum PortRangeFromStrError {
    #[error("the port range must be of format \"MIN:MAX\"")]
    Split,
    #[error("couldn't parse number: {0}")]
    ParseNumber(#[from] ParseIntError),
}

impl FromStr for PortRange {
    type Err = PortRangeFromStrError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (min, max) = s.split_once(":").ok_or(PortRangeFromStrError::Split)?;
        Ok(PortRange {
            min: min.parse().map_err(PortRangeFromStrError::ParseNumber)?,
            max: max.parse().map_err(PortRangeFromStrError::ParseNumber)?,
        })
    }
}

fn default_ice_servers() -> Vec<RtcIceServer> {
    vec![RtcIceServer {
        is_default: true,
        urls: vec![
            // Google
            "stun:stun.l.google.com:19302".to_string(),
            "stun:stun.l.google.com:5349".to_string(),
            "stun:stun1.l.google.com:3478".to_string(),
            "stun:stun1.l.google.com:5349".to_string(),
            "stun:stun2.l.google.com:19302".to_string(),
            "stun:stun2.l.google.com:5349".to_string(),
            "stun:stun3.l.google.com:3478".to_string(),
            "stun:stun3.l.google.com:5349".to_string(),
            "stun:stun4.l.google.com:19302".to_string(),
            "stun:stun4.l.google.com:5349".to_string(),
        ],
        ..Default::default()
    }]
}
fn default_network_types() -> Vec<WebRtcNetworkType> {
    vec![WebRtcNetworkType::Udp4, WebRtcNetworkType::Udp6]
}
fn default_include_loopback_candidates() -> bool {
    true
}

// -- Web Server Config

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebServerConfig {
    // TODO: create streamer overwrite for ice servers
    #[serde(default = "default_bind_address")]
    pub bind_address: SocketAddr,
    pub certificate: Option<ConfigSsl>,
    #[serde(default)]
    pub url_path_prefix: String,
    #[serde(default = "default_session_cookie_secure")]
    pub session_cookie_secure: bool,
    #[serde(default = "default_session_cookie_expiration")]
    pub session_cookie_expiration: Duration,
    pub first_login_create_admin: bool,
    pub first_login_assign_global_hosts: bool,
    pub default_user_id: Option<u32>,
    pub forwarded_header: Option<ForwardedHeaders>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSsl {
    pub private_key_pem: String,
    pub certificate_pem: String,
}

impl Default for WebServerConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            certificate: None,
            url_path_prefix: "".to_string(),
            session_cookie_secure: default_session_cookie_secure(),
            session_cookie_expiration: default_session_cookie_expiration(),
            first_login_create_admin: true,
            first_login_assign_global_hosts: true,
            default_user_id: None,
            forwarded_header: None,
        }
    }
}

fn default_bind_address() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080))
}
fn default_session_cookie_secure() -> bool {
    false
}
fn default_session_cookie_expiration() -> Duration {
    const DAY_SECONDS: u64 = 24 * 60 * 60;

    Duration::from_secs(DAY_SECONDS)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardedHeaders {
    pub username_header: String,
    #[serde(default = "default_forwarded_headers_auto_create_user")]
    pub auto_create_missing_user: bool,
}

impl Default for ForwardedHeaders {
    fn default() -> Self {
        Self {
            username_header: "X-Forwarded-User".to_string(),
            auto_create_missing_user: default_forwarded_headers_auto_create_user(),
        }
    }
}

fn default_forwarded_headers_auto_create_user() -> bool {
    true
}

// -- Moonlight

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoonlightConfig {
    #[serde(default = "default_moonlight_http_port")]
    pub default_http_port: u16,
    #[serde(default = "default_pair_device_name")]
    pub pair_device_name: String,
}

impl Default for MoonlightConfig {
    fn default() -> Self {
        Self {
            default_http_port: default_moonlight_http_port(),
            pair_device_name: default_pair_device_name(),
        }
    }
}

fn default_moonlight_http_port() -> u16 {
    47989
}

fn default_pair_device_name() -> String {
    "roth".to_string()
}

fn default_streamer_path() -> String {
    "./streamer".to_string()
}
