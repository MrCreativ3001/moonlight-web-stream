use crate::config::WebRtcNat1To1IceCandidateType;
use webrtc::ice_transport::ice_candidate_type::RTCIceCandidateType;

pub fn into_webrtc_ice_candidate(value: WebRtcNat1To1IceCandidateType) -> RTCIceCandidateType {
    match value {
        WebRtcNat1To1IceCandidateType::Host => RTCIceCandidateType::Host,
        WebRtcNat1To1IceCandidateType::Srflx => RTCIceCandidateType::Srflx,
    }
}
