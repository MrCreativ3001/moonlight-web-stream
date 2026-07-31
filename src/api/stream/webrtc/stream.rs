use moonlight_common::stream::{
    proto::{
        audio::AudioStreamEvent,
        control::{ControlStreamEvent, packet::ControlPacket},
        video::VideoStreamEvent,
    },
    tokio::{MoonlightStream, MoonlightStreamEvent},
};
use tokio::select;
use tracing::{debug, info, warn};
use webrtc::peer_connection::{RTCPeerConnection, peer_connection_state::RTCPeerConnectionState};

use crate::{
    api::stream::webrtc::{
        audio::AudioChannel,
        control::{ControlChannel, ControlChannelEvent},
        video::{VideoChannel, VideoChannelEvent},
    },
    app::AppError,
};

pub async fn webrtc_loop(
    mut stream: MoonlightStream,
    peer: &RTCPeerConnection,
    mut audio_channel: AudioChannel,
    mut video_channel: VideoChannel,
    mut control_channel: ControlChannel,
) -> Result<(), AppError> {
    info!("started main webrtc loop");

    let mut moonlight_disconnected = false;
    let mut control_channel_active = false;
    loop {
        if !stream.is_alive() {
            info!("stopping stream because the moonlight stream is dead");
            break;
        }

        if matches!(
            peer.connection_state(),
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
        ) && !moonlight_disconnected
        {
            let _ = stream.disconnect();
            moonlight_disconnected = true;
        }

        select! {
            result = stream.drive() => {
                if moonlight_disconnected {
                    continue;
                }

                let event = result?;

                match event {
                    MoonlightStreamEvent::Audio(AudioStreamEvent::OnFrame(frame)) => {
                        if !control_channel_active {
                            continue;
                        }

                        audio_channel.on_frame(frame);
                    }
                    MoonlightStreamEvent::Video(VideoStreamEvent::SignalIdr) => {
                        if let Err(err) = stream.send_raw(ControlPacket::RequestIdr) {
                            warn!(error = %err, "failed to send idr");
                        }
                    }
                    MoonlightStreamEvent::Video(VideoStreamEvent::OnFrame(frame)) => {
                        if !control_channel_active {
                            continue;
                        }

                        video_channel.on_frame(frame);
                    }
                    MoonlightStreamEvent::Control(ControlStreamEvent::Packet(packet)) => {
                        control_channel.send(packet);
                    }
                    _ => {}
                }
            }
            result = video_channel.drive() => {
                let event = result?;

                match event {
                    VideoChannelEvent::SignalIdr => {
                        if let Err(err) = stream.send_raw(ControlPacket::RequestIdr) {
                            warn!(error = %err, "failed to send idr");
                        }
                    }
                }
            }
            result = control_channel.drive() => {
                let event = result?;

                match event {
                    ControlChannelEvent::Active => {
                        control_channel_active = true;
                        debug!("control channel active");
                    },
                    ControlChannelEvent::Inactive => {
                        control_channel_active = false;
                        debug!("control channel inactive");
                    },
                    ControlChannelEvent::Packet(packet) => {
                        if let Err(err) = stream.send_raw(packet) {
                            warn!(error = %err, "failed to relay webrtc client packet to server");
                        }
                    },
                }
            }
        }
    }

    Ok(())
}
