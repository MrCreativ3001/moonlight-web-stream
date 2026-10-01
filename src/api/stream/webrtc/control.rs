use std::sync::Arc;
use std::time::Duration;

use actix_web::web::Bytes;
use bytes::BytesMut;
use futures::future::pending;
use moonlight_common::stream::proto::control::packet::{
    ControlPacket, ControlPacketConfig, PacketDirection,
};
use rtc::data_channel::RTCDataChannelState;
use tokio::sync::mpsc;
use tokio::sync::mpsc::unbounded_channel;
use tokio::time::sleep;
use tokio::{select, spawn};
use tracing::{Instrument, debug, debug_span, info, trace, warn};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::PeerConnection;

use crate::api::stream::create_control_packet_config;
use crate::app::AppError;

pub enum ControlChannelEvent {
    Packet(ControlPacket),
    Closed,
}

pub struct ControlChannel {
    #[allow(unused)]
    channel: Arc<dyn DataChannel>,
    channel_state: RTCDataChannelState,
    on_main_channel_event: mpsc::UnboundedReceiver<DataChannelEvent>,
    on_receive: mpsc::UnboundedReceiver<Bytes>,
    on_receive_sender: mpsc::UnboundedSender<Bytes>,
    on_send: mpsc::UnboundedSender<BytesMut>,
    config: ControlPacketConfig,
}

impl ControlChannel {
    pub async fn new(peer: &dyn PeerConnection) -> Result<Self, AppError> {
        let channel = peer.create_data_channel("moonlight.control", None).await?;

        let (on_receive_sender, on_receive) = unbounded_channel();
        let (on_send, mut on_send_receiver) = unbounded_channel();
        let (on_event_sender, on_event) = unbounded_channel();

        spawn({
            let channel = channel.clone();
            async move {
                let ready_state = async || match channel.ready_state().await {
                    Ok(value) => value,
                    Err(err) => {
                        warn!(error = %err, "failed to query data channel state");
                        RTCDataChannelState::Closed
                    }
                };
                let mut connected = false;

                while let Some(message) = on_send_receiver.recv().await {
                    while !connected
                        && matches!(ready_state().await, RTCDataChannelState::Connecting)
                    {
                        sleep(Duration::from_millis(100)).await;
                    }
                    connected = true;

                    trace!(message = ?message, "sending control message to webrtc peer over data channel");
                    if let Err(err) = channel.send(message).await {
                        warn!(error = %err, "failed to send message over data channel");
                    }
                }
            }
            .instrument(debug_span!("main data channel sender"))
        });
        spawn({
            let channel = channel.clone();
            async move {
                while let Some(event) = channel.poll().await {
                    let _ = on_event_sender.send(event);
                }
            }
            .instrument(debug_span!("main data channel event receiver"))
        });

        Ok(Self {
            channel,
            channel_state: RTCDataChannelState::Connecting,
            on_main_channel_event: on_event,
            on_receive,
            on_receive_sender,
            on_send,
            config: create_control_packet_config(),
        })
    }

    pub fn try_add_channel(&mut self, label: &str, channel: &Arc<dyn DataChannel>) -> bool {
        if !label.starts_with("moonlight.control.") {
            return false;
        }
        info!(label = %label, "adding control channel");

        let channel = channel.clone();
        let on_receive_sender = self.on_receive_sender.clone();
        spawn(
            async move {
                while let Some(event) = channel.poll().await {
                    if let DataChannelEvent::OnMessage(message) = event {
                        if message.is_string {
                            warn!("received text message on data channel");
                        }

                        let _ = on_receive_sender.send(message.data.freeze());
                    }
                }
            }
            .instrument(debug_span!("data channel", label = %label)),
        );

        true
    }

    pub fn send(&mut self, packet: ControlPacket) {
        let mut buffer = [0; ControlPacket::MAX_SIZE];

        let len = match packet.serialize(&self.config, &mut buffer) {
            Ok(value) => value,
            Err(err) => {
                warn!(error = %err, "failed to relay control packet from server to client");
                return;
            }
        };
        let buffer = &buffer[0..len];

        let mut bytes = BytesMut::with_capacity(buffer.len());
        bytes.extend_from_slice(buffer);

        let _ = self.on_send.send(bytes);
    }

    pub fn is_alive(&self) -> bool {
        !matches!(self.channel_state, RTCDataChannelState::Closed) && !self.on_receive.is_closed()
    }

    /// # Cancel Safety
    /// This function is cancel safe.
    /// If it is cancelled no state is lost.
    pub async fn drive(&mut self) -> Result<ControlChannelEvent, AppError> {
        loop {
            if !self.is_alive() {
                // There's nothing to do
                return pending().await;
            }

            select! {
                result = self.on_receive.recv() => {
                    let Some(packet) = result else {
                        // The channel closed
                        return Ok(ControlChannelEvent::Closed);
                    };

                    let Some(packet) = ControlPacket::deserialize(PacketDirection::ServerBound, &self.config, &packet) else {
                        warn!(packet = ?packet, "failed to deserialize packet from webrtc client");
                        continue;
                    };

                    return Ok(ControlChannelEvent::Packet(packet));
                },
                result = self.on_main_channel_event.recv() => {
                    let Some(event) = result else {
                        debug!("closed because of main channel being closed");
                        return Ok(ControlChannelEvent::Closed);
                    };

                    match event {
                        DataChannelEvent::OnMessage(message) => {
                            if message.is_string {
                                warn!("received text message on main data channel");
                            }
                            let _ = self.on_receive_sender.send(message.data.into());
                        }
                        DataChannelEvent::OnOpen => {
                            self.channel_state = RTCDataChannelState::Open;
                        }
                        DataChannelEvent::OnClosing => {
                            self.channel_state = RTCDataChannelState::Closing;
                        }
                        DataChannelEvent::OnClose => {
                            self.channel_state = RTCDataChannelState::Closed;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
