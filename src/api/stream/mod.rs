use moonlight_common::{
    ServerVersion,
    stream::{
        MoonlightStreamConfig, MoonlightStreamSettings,
        proto::control::packet::ControlPacketConfig, video::VideoFormats,
    },
};
use std::{io, net::IpAddr};
use tokio::net::lookup_host;

use crate::{api::bindings::StreamPermissions, app::AppError};

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

/// The streaming protocol needs a numeric address, while the HTTP API accepts host names.
/// Resolve only after the HTTPS launch request so pairing and certificate handling keep the
/// user-configured host name.
pub async fn resolve_stream_address(
    mut config: MoonlightStreamConfig,
) -> Result<MoonlightStreamConfig, AppError> {
    if config.address.parse::<IpAddr>().is_ok() {
        return Ok(config);
    }

    let configured_address = config.address.clone();
    let mut resolved = lookup_host((configured_address.as_str(), 0)).await?;
    let mut fallback = None;

    while let Some(address) = resolved.next() {
        if address.is_ipv4() {
            config.address = address.ip().to_string();
            return Ok(config);
        }
        fallback.get_or_insert(address.ip());
    }

    let address = fallback.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            format!("could not resolve configured Sunshine host {configured_address:?}"),
        )
    })?;
    config.address = address.to_string();

    Ok(config)
}
