use moonlight_common::{
    ServerVersion,
    stream::{
        MoonlightStreamSettings, proto::control::packet::ControlPacketConfig, video::VideoFormats,
    },
};

use crate::api::bindings::StreamPermissions;

pub mod web_socket;
pub mod webrtc;

fn server_version() -> ServerVersion {
    ServerVersion::new(7, 0, 0, 0)
}
fn create_control_packet_config() -> ControlPacketConfig {
    ControlPacketConfig::new(server_version(), true).expect("control packet config")
}

/// IMPORTANT: This doesn't handle transport restrictions!
pub fn apply_role_restrictions(
    permissions: &StreamPermissions,
    settings: &mut MoonlightStreamSettings,
) {
    let StreamPermissions {
        allow_add_hosts: _,
        maximum_bitrate_kbps,
        allow_codec_h264,
        allow_codec_h265,
        allow_codec_av1,
        allow_hdr,
        allow_transport_webrtc: _,
        allow_transport_websockets: _,
    } = permissions;

    if let Some(maximum_bitrate) = maximum_bitrate_kbps
        && settings.bitrate > *maximum_bitrate
    {
        settings.bitrate = *maximum_bitrate;
    }

    let mut supported_formats = settings.supported_video_formats;
    if !allow_codec_h264 {
        supported_formats &= !VideoFormats::MASK_H264;
    }
    if !allow_codec_h265 {
        supported_formats &= !VideoFormats::MASK_H265;
    }
    if !allow_codec_av1 {
        supported_formats &= !VideoFormats::MASK_AV1;
    }
    settings.supported_video_formats = supported_formats;

    if !allow_hdr {
        settings.hdr = false;
    }
}
