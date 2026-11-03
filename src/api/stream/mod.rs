use moonlight_common::{
    ServerVersion,
    stream::{
        EncryptionFlags, MoonlightStreamSettings,
        proto::control::packet::{ControlPacketConfig, RawControlPacketType},
        video::VideoFormats,
    },
};

use crate::api::bindings::StreamPermissions;

pub mod web_socket;
pub mod webrtc;

const PACKET_SIZE: u32 = 1024;

/// Video is included so that hosts with mandatory encryption accept the stream and so that
/// it is protected between the host and this server, no matter which transport is used.
const ENCRYPTION_FLAGS: EncryptionFlags = EncryptionFlags::AUDIO
    .union(EncryptionFlags::VIDEO)
    .union(EncryptionFlags::FOUNDATION_MICROPHONE);

fn server_version() -> ServerVersion {
    ServerVersion::new(7, 0, 0, 0)
}
fn create_control_packet_config() -> ControlPacketConfig {
    let mut config =
        ControlPacketConfig::new(server_version(), true).expect("control packet config");

    config.web_state = Some(RawControlPacketType(0x7001));

    config
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

#[cfg(test)]
mod tests {
    use moonlight_common::stream::EncryptionFlags;

    use crate::api::stream::{ENCRYPTION_FLAGS, PACKET_SIZE};

    #[test]
    fn streams_request_encrypted_video() {
        assert!(ENCRYPTION_FLAGS.contains(EncryptionFlags::VIDEO));
        assert!(ENCRYPTION_FLAGS.contains(EncryptionFlags::AUDIO));
    }

    /// Video encryption requires the packet size to be a multiple of the FEC chunk size
    #[test]
    fn packet_size_is_a_multiple_of_16() {
        assert!(PACKET_SIZE.is_multiple_of(16));
    }
}
