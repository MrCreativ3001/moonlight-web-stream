use common::{
    api_bindings::{RtcIceServer, RtcSdpType},
    config::{WebRtcNat1To1IceCandidateType, WebRtcNetworkType},
};
use webrtc::{
    ice::network_type::NetworkType,
    ice_transport::{ice_candidate_type::RTCIceCandidateType, ice_server::RTCIceServer},
    peer_connection::sdp::sdp_type::RTCSdpType,
};

pub fn from_webrtc_sdp(value: RTCSdpType) -> RtcSdpType {
    match value {
        RTCSdpType::Offer => RtcSdpType::Offer,
        RTCSdpType::Answer => RtcSdpType::Answer,
        RTCSdpType::Pranswer => RtcSdpType::Pranswer,
        RTCSdpType::Rollback => RtcSdpType::Rollback,
        RTCSdpType::Unspecified => RtcSdpType::Unspecified,
    }
}

pub fn into_webrtc_ice(value: RtcIceServer) -> RTCIceServer {
    RTCIceServer {
        urls: value.urls,
        username: value.username,
        credential: value.credential,
    }
}

pub fn into_webrtc_ice_candidate(value: WebRtcNat1To1IceCandidateType) -> RTCIceCandidateType {
    match value {
        WebRtcNat1To1IceCandidateType::Host => RTCIceCandidateType::Host,
        WebRtcNat1To1IceCandidateType::Srflx => RTCIceCandidateType::Srflx,
    }
}

pub fn into_webrtc_network_type(value: WebRtcNetworkType) -> NetworkType {
    match value {
        WebRtcNetworkType::Udp4 => NetworkType::Udp4,
        WebRtcNetworkType::Udp6 => NetworkType::Udp6,
        WebRtcNetworkType::Tcp4 => NetworkType::Tcp4,
        WebRtcNetworkType::Tcp6 => NetworkType::Tcp6,
    }
}
