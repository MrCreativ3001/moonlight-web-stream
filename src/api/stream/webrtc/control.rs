use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::{net::SocketAddr, sync::Arc};

use actix_web::web::Bytes;
use futures::future::{Either, pending};
use moonlight_common::crypto::rustcrypto::RustCryptoBackend;
use moonlight_common::stream::control::DEFAULT_CONTROL_PORT;
use moonlight_common::stream::proto::Instant;
use moonlight_common::stream::proto::control::peer::{
    ControlHostEvent, ControlPeerConfig, ControlPeerRole,
};
use moonlight_common::stream::proto::control::{
    packet::{ControlPacket, ControlPacketConfig, EnetChannel, PacketDirection},
    peer::{ControlHost, ControlHostConfig},
};
use moonlight_common::stream::proto::runtime::UdpStream;
use moonlight_common::stream::tokio::MoonlightStreamError;
use tokio::select;
use tokio::sync::mpsc;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::oneshot;
use tokio::time::{Instant as StdInstant, sleep_until};
use tracing::{debug, warn};
use webrtc::data_channel::RTCDataChannel;
use webrtc::{
    data_channel::{
        data_channel_init::RTCDataChannelInit, data_channel_state::RTCDataChannelState,
    },
    peer_connection::RTCPeerConnection,
};

use crate::api::stream::create_control_packet_config;
use crate::app::AppError;

pub enum ControlChannelEvent {
    Active,
    Inactive,
    Packet(ControlPacket),
}

enum State {
    Simple {
        config: ControlPacketConfig,
        send_queue: VecDeque<Bytes>,
    },
    Enet {
        base_time: StdInstant,
        send_queue: VecDeque<Bytes>,
        host: ControlHost,
    },
}

pub struct ControlChannel {
    channel: Arc<RTCDataChannel>,
    on_open: oneshot::Receiver<()>,
    on_receive: mpsc::UnboundedReceiver<Bytes>,
    state: State,
}

impl ControlChannel {
    fn new(state: State, channel: Arc<RTCDataChannel>) -> Self {
        let (send_open, on_open) = oneshot::channel();

        channel.on_open(Box::new(move || {
            Box::pin(async move {
                debug!("webrtc control channel opened");
                let _ = send_open.send(());
            })
        }));

        let (send_receive, on_receive) = unbounded_channel();
        channel.on_message(Box::new(move |message| {
            let send_receive = send_receive.clone();

            Box::pin(async move {
                send_receive.send(message.data);
            })
        }));

        Self {
            channel,
            on_open,
            on_receive,
            state,
        }
    }
    pub async fn new_simple(peer: &RTCPeerConnection) -> Result<Self, AppError> {
        let channel = peer.create_data_channel("moonlight.control", None).await?;

        Ok(Self::new(
            State::Simple {
                config: create_control_packet_config(),
                send_queue: Default::default(),
            },
            channel,
        ))
    }
    pub async fn new_enet(peer: &RTCPeerConnection) -> Result<Self, AppError> {
        let channel = peer
            .create_data_channel(
                "moonlight.control",
                Some(RTCDataChannelInit {
                    ordered: Some(false),
                    max_retransmits: Some(0),
                    protocol: Some("enet".to_string()),
                    ..Default::default()
                }),
            )
            .await?;

        let base_time = StdInstant::now();

        Ok(Self::new(
            State::Enet {
                base_time,
                send_queue: Default::default(),
                host: ControlHost::new(
                    Instant::from_std(base_time.into_std()),
                    ControlHostConfig {
                        peer_count: 1,
                        peer_channel_count: EnetChannel::CHANNEL_COUNT,
                    },
                    Arc::new(RustCryptoBackend),
                )
                .map_err(MoonlightStreamError::from)?,
            },
            channel,
        ))
    }

    pub fn send(&mut self, packet: ControlPacket) {
        match &mut self.state {
            State::Simple {
                config,
                send_queue: queue,
            } => {
                let mut buffer = [0; ControlPacket::MAX_SIZE];

                let len = match packet.serialize(config, &mut buffer) {
                    Ok(value) => value,
                    Err(err) => {
                        warn!(error = %err, "failed to relay control packet from server to client");
                        return;
                    }
                };
                let buffer = &buffer[0..len];

                queue.push_front(Bytes::copy_from_slice(buffer));
            }
            _ => todo!(),
        }
    }

    pub fn is_alive(&self) -> bool {
        !matches!(self.channel.ready_state(), RTCDataChannelState::Closed)
            && !self.on_receive.is_closed()
    }

    /// Returns [None] if the data channel was closed
    ///
    /// # Cancel Safety
    /// This function is cancel safe.
    /// If it is cancelled no state is lost.
    pub async fn drive(&mut self) -> Result<ControlChannelEvent, AppError> {
        loop {
            if !self.is_alive() {
                // There's nothing to do
                return pending().await;
            }

            match &mut self.state {
                State::Simple { config, send_queue } => {
                    let send_future = if let Some(transmit) = send_queue.front()
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
                                return Ok(ControlChannelEvent::Inactive);
                            };

                            let Some(packet) = ControlPacket::deserialize(PacketDirection::ServerBound, config, &packet) else {
                                warn!(packet = ?packet, "failed to deserialize packet from webrtc client");
                                continue;
                            };

                            return Ok(ControlChannelEvent::Packet(packet));
                        },
                        result = send_future => {
                            send_queue.pop_front();

                            if let Err(err) = result {
                                warn!(error = %err, "failed to send packet on data channel");
                            }
                        },
                    }
                }
                State::Enet {
                    base_time,
                    send_queue,
                    host,
                } => {
                    // Poll outputs
                    while let Some(event) = host.poll_event() {
                        match event {
                            ControlHostEvent::Connected { id, .. } => {
                                host.configure_peer(
                                    id,
                                    ControlPeerConfig {
                                        role: ControlPeerRole::Client,
                                        encryption: None,
                                        packets: create_control_packet_config(),
                                    },
                                );

                                if host.configured_peers().count() == 1 {
                                    return Ok(ControlChannelEvent::Active);
                                }
                            }
                            ControlHostEvent::Disconnected { .. } => {
                                if host.configured_peers().count() < 1 {
                                    return Ok(ControlChannelEvent::Inactive);
                                }
                            }
                            ControlHostEvent::Receive { packet, .. } => {
                                return Ok(ControlChannelEvent::Packet(packet));
                            }
                        }
                    }

                    // Timeout
                    let timeout = host
                        .poll_timeout()
                        .map(|timeout| {
                            Either::Left(sleep_until(timeout.to_std(base_time.into_std()).into()))
                        })
                        .unwrap_or(Either::Right(pending::<()>()));

                    // Sending
                    while let Some((_, transmit)) = host.pending_send() {
                        send_queue.push_back(Bytes::copy_from_slice(transmit));
                        host.consume_send();
                    }

                    let send_future = if let Some(transmit) = send_queue.front()
                        && matches!(self.channel.ready_state(), RTCDataChannelState::Open)
                    {
                        // This send function implementation seems cancel safe
                        Either::Left(self.channel.send(transmit))
                    } else {
                        Either::Right(pending::<_>())
                    };

                    // Wait for event
                    select! {
                        // Try send
                        result = send_future => {
                            send_queue.pop_front();

                            if let Err(err) = result {
                                warn!(error = %err, "failed to send packet on data channel");
                            }
                        }
                        // Try receive
                        result = self.on_receive.recv() => {
                            let Some(packet) = result else {
                                return Ok(ControlChannelEvent::Inactive);
                            };

                            let now = Instant::from_std(StdInstant::now().into_std());
                            host.handle_receive(now, SocketAddr::new(Ipv4Addr::new(192, 168, 178, 1).into(), DEFAULT_CONTROL_PORT), &packet).map_err(MoonlightStreamError::from)?;
                        }
                        _ = timeout =>  {
                            let now = Instant::from_std(StdInstant::now().into_std());
                            host.handle_timeout(now).map_err(MoonlightStreamError::from)?;
                        }
                    }
                }
            }
        }
    }
}
