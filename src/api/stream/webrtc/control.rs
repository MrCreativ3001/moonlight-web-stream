use std::collections::VecDeque;
use std::sync::Arc;

use actix_web::web::Bytes;
use futures::future::{Either, pending};
use moonlight_common::stream::proto::control::packet::{
    ControlPacket, ControlPacketConfig, PacketDirection,
};
use tokio::select;
use tokio::sync::mpsc;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};
use webrtc::data_channel::RTCDataChannel;
use webrtc::{
    data_channel::data_channel_state::RTCDataChannelState, peer_connection::RTCPeerConnection,
};

use crate::api::stream::create_control_packet_config;
use crate::app::AppError;

pub enum ControlChannelEvent {
    Packet(ControlPacket),
    Closed,
}

pub struct ControlChannel {
    channel: Arc<RTCDataChannel>,
    on_open: oneshot::Receiver<()>,
    on_receive: mpsc::UnboundedReceiver<Bytes>,
    on_receive_sender: mpsc::UnboundedSender<Bytes>,
    config: ControlPacketConfig,
    send_queue: VecDeque<Bytes>,
}

impl ControlChannel {
    pub async fn new(peer: &RTCPeerConnection) -> Result<Self, AppError> {
        let channel = peer.create_data_channel("moonlight.control", None).await?;

        let (send_open, on_open) = oneshot::channel();

        channel.on_open(Box::new(move || {
            Box::pin(async move {
                debug!("webrtc control channel opened");
                let _ = send_open.send(());
            })
        }));

        let (on_receive_sender, on_receive) = unbounded_channel();

        // The browser uses the primary control channel for reliable packets such
        // as mouse wheel input. Subchannels are only used for latency-sensitive
        // input, so the primary channel must receive messages too.
        let on_receive_sender_clone = on_receive_sender.clone();
        channel.on_message(Box::new(move |message| {
            let on_receive_sender = on_receive_sender_clone.clone();

            Box::pin(async move {
                let _ = on_receive_sender.send(message.data);
            })
        }));

        Ok(Self {
            channel,
            on_open,
            on_receive,
            on_receive_sender,
            config: create_control_packet_config(),
            send_queue: Default::default(),
        })
    }

    pub fn try_add_channel(&mut self, channel: &RTCDataChannel) -> bool {
        if !channel.label().starts_with("moonlight.control.") {
            return false;
        }
        info!(label = %channel.label(), "adding control channel");

        let on_receive_sender = self.on_receive_sender.clone();
        channel.on_message(Box::new(move |message| {
            let send_receive = on_receive_sender.clone();

            Box::pin(async move {
                let _ = send_receive.send(message.data);
            })
        }));

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

        self.send_queue.push_front(Bytes::copy_from_slice(buffer));
    }

    pub fn is_alive(&self) -> bool {
        !matches!(self.channel.ready_state(), RTCDataChannelState::Closed)
            && !self.on_receive.is_closed()
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

            let send_future = if let Some(transmit) = self.send_queue.front()
                && matches!(self.channel.ready_state(), RTCDataChannelState::Open)
            {
                // This send function implementation seems cancel safe
                Either::Left(self.channel.send(transmit))
            } else {
                Either::Right(pending::<_>())
            };

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
                result = send_future => {
                    self.send_queue.pop_front();

                    if let Err(err) = result {
                        warn!(error = %err, "failed to send packet on data channel");
                    }
                },
                // Wake up on channel open
                _ = &mut self.on_open, if !self.on_open.is_terminated() => {}
            }
        }
    }
}
