use std::ops::Range;

use async_trait::async_trait;
use common::{
    StreamSettings,
    api_bindings::{
        GeneralClientMessage, GeneralServerMessage, StreamerStatsUpdate, TransportChannelId,
    },
    ipc::{ServerIpcMessage, StreamerIpcMessage},
};
use log::warn;
use moonlight_common::stream::{
    bindings::{
        AudioConfig, ControllerButtons, ControllerCapabilities, ControllerType, DecodeResult,
        KeyAction, KeyFlags, KeyModifiers, MouseButton, MouseButtonAction, OpusMultistreamConfig,
        TouchEventType, VideoDecodeUnit,
    },
    video::VideoSetup,
};
use num::FromPrimitive;
use thiserror::Error;

use crate::buffer::ByteBuffer;

pub mod web_socket;
pub mod webrtc;

/// Look at TransportChannelId
#[derive(Debug, Clone, Copy)]
pub struct TransportChannel(pub u8);

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("the channel was closed")]
    ChannelClosed,
    #[error("the transport was closed")]
    Closed,
    #[error("implementation: {0}")]
    Implementation(anyhow::Error),
}

#[derive(Debug)]
pub enum InboundPacket {
    General {
        message: GeneralClientMessage,
    },
    MouseMove {
        delta_x: i16,
        delta_y: i16,
    },
    MousePosition {
        x: i16,
        y: i16,
        reference_width: i16,
        reference_height: i16,
    },
    MouseButton {
        action: MouseButtonAction,
        button: MouseButton,
    },
    HighResScroll {
        delta_x: i16,
        delta_y: i16,
    },
    Scroll {
        delta_x: i8,
        delta_y: i8,
    },
    Key {
        action: KeyAction,
        modifiers: KeyModifiers,
        key: u16,
        flags: KeyFlags,
    },
    Text {
        text: String,
    },
    ControllerConnected {
        id: u8,
        ty: ControllerType,
        supported_buttons: ControllerButtons,
        capabilities: ControllerCapabilities,
    },
    ControllerDisconnected {
        id: u8,
    },
    ControllerState {
        id: u8,
        buttons: ControllerButtons,
        left_trigger: u8,
        right_trigger: u8,
        left_stick_x: i16,
        left_stick_y: i16,
        right_stick_x: i16,
        right_stick_y: i16,
    },
    Touch {
        pointer_id: u32,
        x: f32,
        y: f32,
        pressure_or_distance: f32,
        contact_area_major: f32,
        contact_area_minor: f32,
        rotation: Option<u16>,
        event_type: TouchEventType,
    },
    Rtt {
        sequence_number: u16,
    },
    RequestVideoIdr,
}

impl InboundPacket {
    const DEFAULT_CONTROLLER_BUTTONS: ControllerButtons = ControllerButtons::all();
    const DEFAULT_CONTROLLER_CAPABILITIES: ControllerCapabilities = ControllerCapabilities::empty();

    pub const CONTROLLER_CHANNELS: [u8; 16] = [
        TransportChannelId::CONTROLLER0,
        TransportChannelId::CONTROLLER1,
        TransportChannelId::CONTROLLER2,
        TransportChannelId::CONTROLLER3,
        TransportChannelId::CONTROLLER4,
        TransportChannelId::CONTROLLER5,
        TransportChannelId::CONTROLLER6,
        TransportChannelId::CONTROLLER7,
        TransportChannelId::CONTROLLER8,
        TransportChannelId::CONTROLLER9,
        TransportChannelId::CONTROLLER10,
        TransportChannelId::CONTROLLER11,
        TransportChannelId::CONTROLLER12,
        TransportChannelId::CONTROLLER13,
        TransportChannelId::CONTROLLER14,
        TransportChannelId::CONTROLLER15,
    ];

    pub fn deserialize(channel: TransportChannel, bytes: &[u8]) -> Option<Self> {
        let mut buffer = ByteBuffer::new(bytes);

        match channel {
            TransportChannel(TransportChannelId::GENERAL) => {
                if buffer.remaining() < 2 {
                    warn!("[InboudPacket]: failed to read general message");
                    return None;
                }

                let len = buffer.get_u16();
                let text = match buffer.get_utf8_raw(len as usize) {
                    Ok(text) => text,
                    Err(err) => {
                        warn!("[InboudPacket]: failed to read a general message: {err}");
                        return None;
                    }
                };
                let message = match serde_json::from_str(text) {
                    Ok(message) => message,
                    Err(err) => {
                        warn!("[InboudPacket]: failed to deserialize general message: {err}");
                        return None;
                    }
                };

                Some(Self::General { message })
            }
            TransportChannel(TransportChannelId::STATS) => {
                warn!("[InboundPacket]: tried to deserialize stats packet, this shouldn't happen");
                None
            }
            TransportChannel(TransportChannelId::HOST_VIDEO) => {
                if buffer.remaining() < 1 {
                    warn!("[InboudPacket]: failed to video message");
                    return None;
                }

                let ty = buffer.get_u8();
                if ty == 0 {
                    Some(InboundPacket::RequestVideoIdr)
                } else {
                    warn!("[InboundPacket]: failed to deserialize host video packet");
                    None
                }
            }
            TransportChannel(TransportChannelId::HOST_AUDIO) => {
                warn!(
                    "[InboundPacket]: tried to deserialize host audio packet, this shouldn't happen"
                );
                None
            }
            TransportChannel(
                TransportChannelId::MOUSE_ABSOLUTE
                | TransportChannelId::MOUSE_RELIABLE
                | TransportChannelId::MOUSE_RELATIVE,
            ) => {
                if buffer.remaining() < 1 {
                    warn!("[InboudPacket]: failed to read mouse message");
                    return None;
                }

                let ty = buffer.get_u8();
                if ty == 0 {
                    // Move
                    if buffer.remaining() < 4 {
                        warn!("[InboudPacket]: failed to read mouse move message");
                        return None;
                    }

                    let delta_x = buffer.get_i16();
                    let delta_y = buffer.get_i16();

                    Some(InboundPacket::MouseMove { delta_x, delta_y })
                } else if ty == 1 {
                    // Position
                    if buffer.remaining() < 8 {
                        warn!("[InboudPacket]: failed to read mouse position message");
                        return None;
                    }

                    let x = buffer.get_i16();
                    let y = buffer.get_i16();
                    let reference_width = buffer.get_i16();
                    let reference_height = buffer.get_i16();

                    Some(InboundPacket::MousePosition {
                        x,
                        y,
                        reference_width,
                        reference_height,
                    })
                } else if ty == 2 {
                    // Button Press / Release
                    if buffer.remaining() < 2 {
                        warn!("[InboudPacket]: failed to read mouse press / release message");
                        return None;
                    }

                    let action = if buffer.get_bool() {
                        MouseButtonAction::Press
                    } else {
                        MouseButtonAction::Release
                    };
                    let Some(button) = MouseButton::from_u8(buffer.get_u8()) else {
                        warn!("[InboundPacket]: received invalid mouse button");
                        return None;
                    };

                    Some(InboundPacket::MouseButton { action, button })
                } else if ty == 3 {
                    // Mouse Wheel High Res
                    if buffer.remaining() < 4 {
                        warn!("[InboudPacket]: failed to read mouse wheel high res message");
                        return None;
                    }

                    let delta_x = buffer.get_i16();
                    let delta_y = buffer.get_i16();

                    Some(InboundPacket::HighResScroll { delta_x, delta_y })
                } else if ty == 4 {
                    // Mouse Wheel Normal
                    if buffer.remaining() < 4 {
                        warn!("[InboudPacket]: failed to read mouse wheel normal message");
                        return None;
                    }

                    let delta_x = buffer.get_i8();
                    let delta_y = buffer.get_i8();

                    Some(InboundPacket::Scroll { delta_x, delta_y })
                } else {
                    warn!(
                        "[InboundPacket]: tried to deserialize mouse packet with type {ty}, this shouldn't happen"
                    );
                    None
                }
            }
            TransportChannel(TransportChannelId::KEYBOARD) => {
                if buffer.remaining() < 1 {
                    warn!("[InboudPacket]: failed to read keyboard message");
                    return None;
                }

                let ty = buffer.get_u8();
                if ty == 0 {
                    // Key press / release
                    if buffer.remaining() < 4 {
                        warn!("[InboudPacket]: failed to read key press / release message");
                        return None;
                    }

                    let action = if buffer.get_bool() {
                        KeyAction::Down
                    } else {
                        KeyAction::Up
                    };
                    let modifiers =
                        KeyModifiers::from_bits(buffer.get_u8() as i8).unwrap_or_else(|| {
                            warn!("[InboundPacket]: received invalid key modifiers");
                            KeyModifiers::empty()
                        });
                    let key = buffer.get_u16();

                    Some(InboundPacket::Key {
                        action,
                        modifiers,
                        key,
                        flags: KeyFlags::empty(),
                    })
                } else if ty == 1 {
                    if buffer.remaining() < 1 {
                        warn!("[InboudPacket]: failed to read key as text message");
                        return None;
                    }

                    let len = buffer.get_u8();
                    let Ok(key) = buffer.get_utf8_raw(len as usize) else {
                        warn!("[InboundPacket]: received invalid keyboard text message");
                        return None;
                    };

                    Some(InboundPacket::Text {
                        text: key.to_owned(),
                    })
                } else {
                    warn!(
                        "[InboundPacket]: tried to deserialize keyboard packet with type {ty}, this shouldn't happen"
                    );
                    None
                }
            }
            TransportChannel(TransportChannelId::TOUCH) => {
                if buffer.remaining() < 27 {
                    warn!("[InboudPacket]: failed to read touch message");
                    return None;
                }

                let event_type = match buffer.get_u8() {
                    0 => TouchEventType::Down,
                    1 => TouchEventType::Move,
                    2 => TouchEventType::Cancel,
                    _ => {
                        warn!("[InboundPacket]: received invalid touch event type");
                        return None;
                    }
                };
                let pointer_id = buffer.get_u32();
                let x = buffer.get_f32();
                let y = buffer.get_f32();
                let pressure_or_distance = buffer.get_f32();
                let contact_area_major = buffer.get_f32();
                let contact_area_minor = buffer.get_f32();
                let rotation = buffer.get_u16();

                Some(InboundPacket::Touch {
                    pointer_id,
                    x,
                    y,
                    pressure_or_distance,
                    contact_area_major,
                    contact_area_minor,
                    rotation: Some(rotation),
                    event_type,
                })
            }
            TransportChannel(TransportChannelId::CONTROLLERS) => {
                if buffer.remaining() < 1 {
                    warn!("[InboudPacket]: failed to read controller message");
                    return None;
                }

                let ty = buffer.get_u8();
                if ty == 0 {
                    // add controller
                    if buffer.remaining() < 7 {
                        warn!("[InboudPacket]: failed to controller add message");
                        return None;
                    }

                    let id = buffer.get_u8();
                    let supported_buttons = ControllerButtons::from_bits(buffer.get_u32())
                        .unwrap_or_else(|| {
                            warn!(
                                "[InboundPacket]: received a controller with invalid button layout"
                            );
                            Self::DEFAULT_CONTROLLER_BUTTONS
                        });
                    let capabilities = ControllerCapabilities::from_bits(buffer.get_u16())
                        .unwrap_or_else(|| {
                            warn!(
                                "[InboundPacket]: received a controller with invalid capabilities"
                            );
                            Self::DEFAULT_CONTROLLER_CAPABILITIES
                        });

                    Some(InboundPacket::ControllerConnected {
                        id,
                        ty: ControllerType::Unknown,
                        supported_buttons,
                        capabilities,
                    })
                } else if ty == 1 {
                    // Remove controller
                    if buffer.remaining() < 1 {
                        warn!("[InboudPacket]: failed to read controller remove message");
                        return None;
                    }

                    let id = buffer.get_u8();

                    Some(InboundPacket::ControllerDisconnected { id })
                } else {
                    warn!(
                        "[InboundPacket]: tried to deserialize controllers packet with type {ty}, this shouldn't happen"
                    );
                    None
                }
            }
            TransportChannel(channel_id)
                if let Some((gamepad_id, _)) = Self::CONTROLLER_CHANNELS
                    .iter()
                    .enumerate()
                    .find(|(_, cmp_channel_id)| **cmp_channel_id == channel_id) =>
            {
                if buffer.remaining() < 1 {
                    warn!(
                        "[InboudPacket]: failed to read controller state message {channel_id}, gamepad: {gamepad_id}"
                    );
                    return None;
                }

                let ty = buffer.get_u8();
                if ty == 0 {
                    // State
                    if buffer.remaining() < 14 {
                        warn!(
                            "[InboudPacket]: failed to read controller state message {channel_id}, gamepad: {gamepad_id}"
                        );
                        return None;
                    }

                    let Some(buttons) = ControllerButtons::from_bits(buffer.get_u32()) else {
                        warn!(
                            "[InboundPacket]: received invalid controller buttons for controller {gamepad_id}"
                        );
                        return None;
                    };

                    let left_trigger = buffer.get_u8();
                    let right_trigger = buffer.get_u8();
                    let left_stick_x = buffer.get_i16();
                    let left_stick_y = buffer.get_i16();
                    let right_stick_x = buffer.get_i16();
                    let right_stick_y = buffer.get_i16();

                    Some(InboundPacket::ControllerState {
                        id: gamepad_id as u8,
                        buttons,
                        left_trigger,
                        right_trigger,
                        left_stick_x,
                        left_stick_y,
                        right_stick_x,
                        right_stick_y,
                    })
                } else {
                    warn!(
                        "[InboundPacket]: tried to deserialize controller {gamepad_id} packet with type {ty}, this shouldn't happen"
                    );
                    None
                }
            }
            TransportChannel(TransportChannelId::RTT) => {
                let ty = buffer.get_u8();

                if ty == 0 {
                    if buffer.remaining() < 2 {
                        return None;
                    }
                    let sequence_number = buffer.get_u16();

                    Some(InboundPacket::Rtt { sequence_number })
                } else {
                    warn!(
                        "[InboundPacket]: tried to deserialize rtt packet with type {ty}, this shouldn't happen"
                    );
                    None
                }
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum OutboundPacket {
    General {
        message: GeneralServerMessage,
    },
    Stats(StreamerStatsUpdate),
    ControllerRumble {
        controller_number: u8,
        low_frequency_motor: u16,
        high_frequency_motor: u16,
    },
    ControllerTriggerRumble {
        controller_number: u8,
        left_trigger_motor: u16,
        right_trigger_motor: u16,
    },
    Rtt {
        sequence_number: u16,
    },
}

impl OutboundPacket {
    pub fn serialize(&self, raw_buffer: &mut Vec<u8>) -> Option<(TransportChannel, Range<usize>)> {
        match self {
            Self::General { message } => {
                let Ok(text) = serde_json::to_string(&message) else {
                    warn!("Failed to send general message: {message:?}");
                    return None;
                };
                raw_buffer.resize(text.len() + 2, 0u8);
                let mut buffer = ByteBuffer::new(raw_buffer as &mut [u8]);

                buffer.put_u16(text.len() as u16);
                buffer.put_utf8_raw(&text);

                buffer.flip();
                Some((
                    TransportChannel(TransportChannelId::GENERAL),
                    buffer.into_raw().1,
                ))
            }
            Self::Stats(stats) => {
                let Ok(text) = serde_json::to_string(&stats) else {
                    warn!("Failed to send stats message: {stats:?}");
                    return None;
                };
                raw_buffer.resize(text.len() + 2, 0u8);
                let mut buffer = ByteBuffer::new(raw_buffer as &mut [u8]);

                buffer.put_u16(text.len() as u16);
                buffer.put_utf8_raw(&text);

                buffer.flip();
                Some((
                    TransportChannel(TransportChannelId::STATS),
                    buffer.into_raw().1,
                ))
            }
            Self::ControllerRumble {
                controller_number,
                low_frequency_motor,
                high_frequency_motor,
            } => {
                raw_buffer.resize(6, 0);
                let mut buffer = ByteBuffer::new(raw_buffer as &mut [u8]);

                // Requires 6 bytes
                buffer.put_u8(0);
                buffer.put_u8(*controller_number);
                buffer.put_u16(*low_frequency_motor);
                buffer.put_u16(*high_frequency_motor);

                buffer.flip();
                Some((
                    TransportChannel(TransportChannelId::CONTROLLER0 + controller_number),
                    buffer.into_raw().1,
                ))
            }
            Self::ControllerTriggerRumble {
                controller_number,
                left_trigger_motor,
                right_trigger_motor,
            } => {
                raw_buffer.resize(6, 0);
                let mut buffer = ByteBuffer::new(raw_buffer as &mut [u8]);

                // Requires 6 bytes
                buffer.put_u8(0);
                buffer.put_u8(*controller_number);
                buffer.put_u16(*left_trigger_motor);
                buffer.put_u16(*right_trigger_motor);

                buffer.flip();
                Some((
                    TransportChannel(TransportChannelId::CONTROLLER0 + controller_number),
                    buffer.into_raw().1,
                ))
            }
            Self::Rtt { sequence_number } => {
                raw_buffer.resize(3, 0);
                let mut buffer = ByteBuffer::new(raw_buffer as &mut [u8]);

                buffer.put_u8(0);
                buffer.put_u16(*sequence_number);

                Some((
                    TransportChannel(TransportChannelId::RTT),
                    buffer.into_raw().1,
                ))
            }
        }
    }
}

#[derive(Debug)]
pub enum TransportEvent {
    StartStream { settings: StreamSettings },
    RecvPacket(InboundPacket),
    SendIpc(StreamerIpcMessage),
    Closed,
}

#[async_trait]
pub trait TransportEvents {
    /// Some InboundPackets are not handled by the consumer of this interface -> they must be handled by this Transport impl:
    /// - RequestIdr -> you should request an idr via the send_video_unit fn
    async fn poll_event(&mut self) -> Result<TransportEvent, TransportError>;
}
#[async_trait]
pub trait TransportSender {
    async fn setup_video(&self, setup: VideoSetup) -> i32;
    async fn send_video_unit<'a>(
        &'a self,
        unit: &'a VideoDecodeUnit<'a>,
    ) -> Result<DecodeResult, TransportError>;

    async fn setup_audio(
        &self,
        audio_config: AudioConfig,
        stream_config: OpusMultistreamConfig,
    ) -> i32;
    async fn send_audio_sample(&self, data: &[u8]) -> Result<(), TransportError>;

    async fn send(&self, packet: OutboundPacket) -> Result<(), TransportError>;

    async fn on_ipc_message(&self, message: ServerIpcMessage) -> Result<(), TransportError>;

    async fn close(&self) -> Result<(), TransportError>;
}
