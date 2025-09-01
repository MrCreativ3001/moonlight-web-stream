use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use serde::{Deserialize, Serialize};

use crate::api_bindings::RtcIceServer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub credentials: String,
    #[serde(default = "data_path_default")]
    pub data_path: String,
    #[serde(default = "default_bind_address")]
    pub bind_address: SocketAddr,
    #[serde(default = "moonlight_default_http_port_default")]
    pub moonlight_default_http_port: u16,
    #[serde(default = "default_pair_device_name")]
    pub pair_device_name: String,
    #[serde(default = "default_ice_servers")]
    pub webrtc_ice_servers: Vec<RtcIceServer>,
    #[serde(default)]
    pub webrtc_port_range: Option<PortRange>,
    #[serde(default)]
    pub webrtc_nat_1to1: Option<WebRtcNat1To1Mapping>,
    #[serde(default = "default_network_types")]
    pub webrtc_network_types: Vec<WebRtcNetworkType>,
    #[serde(default)]
    pub web_path_prefix: String,
    pub certificate: Option<ConfigSsl>,
    #[serde(default = "default_streamer_path")]
    pub streamer_path: String,
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
pub struct ConfigSsl {
    pub private_key_pem: String,
    pub certificate_pem: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRange {
    pub min: u16,
    pub max: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            credentials: "default".to_string(),
            data_path: data_path_default(),
            bind_address: default_bind_address(),
            moonlight_default_http_port: moonlight_default_http_port_default(),
            webrtc_ice_servers: default_ice_servers(),
            webrtc_port_range: Default::default(),
            webrtc_nat_1to1: Default::default(),
            webrtc_network_types: default_network_types(),
            pair_device_name: default_pair_device_name(),
            web_path_prefix: String::new(),
            certificate: None,
            streamer_path: default_streamer_path(),
        }
    }
}

fn data_path_default() -> String {
    "server/data.json".to_string()
}

fn default_bind_address() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8080))
}

fn moonlight_default_http_port_default() -> u16 {
    47989
}

fn default_ice_servers() -> Vec<RtcIceServer> {
    vec![
        // Google
        RtcIceServer {
            urls: vec![
                // Google
                "stun:l.google.com:19302".to_owned(),
                "stun:stun.l.google.com:19302".to_owned(),
                "stun:stun1.l.google.com:19302".to_owned(),
                "stun:stun2.l.google.com:19302".to_owned(),
                "stun:stun3.l.google.com:19302".to_owned(),
                "stun:stun4.l.google.com:19302".to_owned(),
            ],
            ..Default::default()
        },
    ]
}
fn default_network_types() -> Vec<WebRtcNetworkType> {
    vec![WebRtcNetworkType::Udp4, WebRtcNetworkType::Udp6]
}

fn default_pair_device_name() -> String {
    "roth".to_string()
}

fn default_streamer_path() -> String {
    "./streamer".to_string()
}
