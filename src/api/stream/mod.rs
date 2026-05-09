use moonlight_common::{ServerVersion, stream::proto::control::packet::ControlPacketConfig};

pub mod web_socket;
pub mod whep;

fn create_control_packet_config() -> ControlPacketConfig {
    ControlPacketConfig::new(ServerVersion::new(7, 0, 0, 0), true).expect("control packet config")
}
