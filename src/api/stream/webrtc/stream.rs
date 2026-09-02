use std::{future::pending, sync::Arc};

use moonlight_common::stream::{
    control::{
        CompactKeyStates, KeyAction, KeyCode, KeyFlags, KeyModifiers, MouseButton,
        MouseButtonAction,
    },
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

    let mut last_key_states_sequence_number = 0;
    let mut last_key_states = CompactKeyStates::default();

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
                    ControlChannelEvent::Packet(ControlPacket::WebState { sequence_number, keys }) => {
                        // The server doesn't support this packet, so we must remove it
                        if sequence_number <= last_key_states_sequence_number && sequence_number.abs_diff(last_key_states_sequence_number) < 1000 {
                            // packets can be dropped when the sequence number is larger than the last the sequence number
                            // and when the sequence number doesn't change dramatically
                            continue;
                        }
                        last_key_states_sequence_number = sequence_number;

                        let modifiers = extract_modifiers(keys);

                        // Find newly pressed keys
                        for changed_key in (keys & !last_key_states).pressed_iter() {
                            send_key_change(&mut stream, modifiers, changed_key, KeyAction::Down);
                        }

                        // Find newly released keys
                        for changed_key in (last_key_states & !keys).pressed_iter() {
                            send_key_change(&mut stream, modifiers, changed_key, KeyAction::Up);
                        }

                        // Update last keys
                        last_key_states = keys;

                        continue;
                    }
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

fn send_key_change(
    stream: &mut MoonlightStream,
    modifiers: KeyModifiers,
    key_code: KeyCode,
    action: KeyAction,
) {
    info!(action = ?action, key_code = ?key_code, "test");

    let mouse_button = match key_code {
        KeyCode::VK_LBUTTON => Some(MouseButton::Left),
        KeyCode::VK_MBUTTON => Some(MouseButton::Middle),
        KeyCode::VK_RBUTTON => Some(MouseButton::Right),
        KeyCode::VK_XBUTTON1 => Some(MouseButton::X1),
        KeyCode::VK_XBUTTON2 => Some(MouseButton::X2),
        _ => None,
    };

    let packet = if let Some(button) = mouse_button {
        let action = if action == KeyAction::Down {
            MouseButtonAction::Press
        } else {
            MouseButtonAction::Release
        };

        ControlPacket::MouseButton { action, button }
    } else {
        ControlPacket::Keyboard {
            action,
            flags: KeyFlags::empty(),
            key_code,
            modifiers,
            zero: 0,
        }
    };

    if let Err(err) = stream.send_raw(packet) {
        warn!(error = %err, "failed to send control packet");
    }
}

fn extract_modifiers(key_states: CompactKeyStates) -> KeyModifiers {
    // Get current modifiers
    let mut modifiers = KeyModifiers::empty();

    if key_states.is_pressed(KeyCode::VK_SHIFT).expect("shift key") == KeyAction::Down
        || key_states
            .is_pressed(KeyCode::VK_LSHIFT)
            .expect("left shift key")
            == KeyAction::Down
        || key_states
            .is_pressed(KeyCode::VK_RSHIFT)
            .expect("right shift key")
            == KeyAction::Down
    {
        modifiers |= KeyModifiers::SHIFT;
    }

    if key_states
        .is_pressed(KeyCode::VK_CONTROL)
        .expect("control key")
        == KeyAction::Down
        || key_states
            .is_pressed(KeyCode::VK_LCONTROL)
            .expect("left control key")
            == KeyAction::Down
        || key_states
            .is_pressed(KeyCode::VK_RCONTROL)
            .expect("right control key")
            == KeyAction::Down
    {
        modifiers |= KeyModifiers::CTRL;
    }

    if key_states.is_pressed(KeyCode::VK_MENU).expect("alt key") == KeyAction::Down
        || key_states
            .is_pressed(KeyCode::VK_LMENU)
            .expect("left alt key")
            == KeyAction::Down
        || key_states
            .is_pressed(KeyCode::VK_RMENU)
            .expect("right alt key")
            == KeyAction::Down
    {
        modifiers |= KeyModifiers::ALT;
    }

    if key_states
        .is_pressed(KeyCode::VK_LWIN)
        .expect("left windows key")
        == KeyAction::Down
        || key_states
            .is_pressed(KeyCode::VK_RWIN)
            .expect("right windows key")
            == KeyAction::Down
    {
        modifiers |= KeyModifiers::META;
    }

    modifiers
}
