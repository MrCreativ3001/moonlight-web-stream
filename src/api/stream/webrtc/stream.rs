use std::{future::pending, sync::Arc};

use moonlight_common::stream::{
    proto::{
        audio::AudioStreamEvent,
        control::{ControlStreamEvent, packet::ControlPacket},
        video::VideoStreamEvent,
    },
    tokio::{MoonlightStream, MoonlightStreamEvent},
};
use tokio::{select, sync::mpsc};
use tracing::{debug, info, warn};
use webrtc::{
    data_channel::RTCDataChannel,
    peer_connection::{RTCPeerConnection, peer_connection_state::RTCPeerConnectionState},
};

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
    mut on_data_channel: mpsc::UnboundedReceiver<Arc<RTCDataChannel>>,
) -> Result<(), AppError> {
    info!("started main webrtc loop");

    // Sunshine can begin encoding before the WebRTC peer has completed ICE.
    // Ask for an IDR immediately so the first relayable frame is independently decodable.
    if let Err(err) = stream.send_raw(ControlPacket::RequestIdr) {
        warn!(error = %err, "failed to request initial idr");
    }

    let mut moonlight_disconnected = false;
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
            Some(data_channel) = async { if on_data_channel.is_closed() { pending().await } else { on_data_channel.recv().await } } => {
                debug!(data_channel = ?data_channel.label(), "got data channel");

                if control_channel.try_add_channel(&data_channel) {
                    continue;
                }
            }
            result = stream.drive() => {
                if moonlight_disconnected {
                    continue;
                }

                let event = result?;

                match event {
                    MoonlightStreamEvent::Audio(AudioStreamEvent::OnFrame(frame)) => {
                        audio_channel.on_frame(frame);
                    }
                    MoonlightStreamEvent::Video(VideoStreamEvent::SignalIdr) => {
                        if let Err(err) = stream.send_raw(ControlPacket::RequestIdr) {
                            warn!(error = %err, "failed to send idr");
                        }
                    }
                    MoonlightStreamEvent::Video(VideoStreamEvent::OnFrame(frame)) => {
                        video_channel.on_frame(frame);
                    }
                    MoonlightStreamEvent::Control(ControlStreamEvent::Packet(packet)) => {
                        if let ControlPacket::HdrMode { enabled, sunshine } = &packet {
                            video_channel.set_hdr_enabled(*enabled, *sunshine);
                        }

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
                    ControlChannelEvent::Packet(packet) => {
                        if let Err(err) = stream.send_raw(packet) {
                            warn!(error = %err, "failed to relay webrtc client packet to server");
                        }
                    },
                    ControlChannelEvent::Closed => {
                        info!("control channel closed");
                    },
                }
            }
        }
    }

    Ok(())
}
