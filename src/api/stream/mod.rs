use moonlight_common::{
    ServerVersion,
    stream::proto::control::packet::{ControlPacketConfig, RawControlPacketType},
};

pub mod web_socket;
pub mod webrtc;

fn server_version() -> ServerVersion {
    ServerVersion::new(7, 0, 0, 0)
}
fn create_control_packet_config() -> ControlPacketConfig {
    let mut config =
        ControlPacketConfig::new(server_version(), true).expect("control packet config");

    config.web_state = Some(RawControlPacketType(0x7001));

    config
}
