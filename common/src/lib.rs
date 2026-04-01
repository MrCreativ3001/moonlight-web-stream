use log::warn;
use moonlight_common::stream::video::SupportedVideoFormats;
use serde::Serialize;

use crate::api_bindings::{StreamPermissions, StreamSettings};

pub mod api_bindings;
pub mod api_bindings_ext;
pub mod config;
pub mod ipc;

/// Applies the permissions / restrictions to the current settings of the user.
/// This won't error, it'll just overwrite it, because the GUI should indicate those restrictions.
pub fn apply_permissions_to_settings(
    permissions: &StreamPermissions,
    settings: &mut StreamSettings,
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
        && settings.bitrate_kbps > *maximum_bitrate
    {
        settings.bitrate_kbps = *maximum_bitrate;
    }

    let mut supported_codecs = SupportedVideoFormats::from_bits_truncate(settings.supported_codecs);
    if !allow_codec_h264 {
        supported_codecs &= !SupportedVideoFormats::MASK_H264;
    }
    if !allow_codec_h265 {
        supported_codecs &= !SupportedVideoFormats::MASK_H265;
    }
    if !allow_codec_av1 {
        supported_codecs &= !SupportedVideoFormats::MASK_AV1;
    }
    settings.supported_codecs = supported_codecs.bits();

    if !allow_hdr {
        settings.hdr = false;
    }

    // Transport restrictions are handled in the streamer
}

pub fn serialize_json<T>(message: &T) -> Option<String>
where
    T: Serialize,
{
    let Ok(json) = serde_json::to_string(&message) else {
        warn!("[Stream]: failed to serialize to json");
        return None;
    };

    Some(json)
}
