use std::net::Ipv4Addr;
use std::time::Instant as StdInstant;
use std::{net::SocketAddr, sync::Arc};

use actix_web::web::Bytes;
use moonlight_common::stream::proto::control::peer::{
    ControlHostAction, ControlHostEvent, ControlHostOutput, ControlPeerConfig,  ControlPeerRole
};
use moonlight_common::{
    crypto::disabled::DisabledCryptoBackend,
    stream::{
        proto::{
            Instant,
            control::{
                packet::{ControlPacket, ControlPacketConfig, EnetChannel, PacketDirection},
                peer::{ControlHost, ControlHostConfig, ControlHostInput},
            },
        },
        tokio::MoonlightStream,
    },
};
use tokio::select;
use tokio::sync::Mutex;
use tokio::time::sleep_until;
use tokio::{
    spawn,
    sync::{Notify, mpsc::Receiver, oneshot},
};
use tracing::{Instrument, debug, debug_span, error, info, trace, warn};
use webrtc::{
    data_channel::{
        data_channel_init::RTCDataChannelInit, data_channel_message::DataChannelMessage,
        data_channel_state::RTCDataChannelState,
    },
    peer_connection::RTCPeerConnection,
};

pub async fn add_simple_control_channel(
    peer: &RTCPeerConnection,
    moonlight_stream: Arc<MoonlightStream>,
    mut clientbound_control_receiver: Receiver<ControlPacket>,
    control_config: &ControlPacketConfig,
) {
    let control = peer.create_data_channel("control", None).await.unwrap();
    debug!("added simple control channel");

    let stream = moonlight_stream.clone();

    // Spawn from client to host relay
    control.on_message({
        let control_config = control_config.clone();
        let stream = stream.clone();

        Box::new(move |message: DataChannelMessage| {
            let control_config = control_config.clone();
            let stream = stream.clone();

            Box::pin(async move {
                let Some(packet) = ControlPacket::deserialize(
                    PacketDirection::ServerBound,
                    &control_config,
                    &message.data,
                ) else {
                    warn!(packet = ?message.data, "failed to deserialize client packet");
                    return;
                };

                debug!(packet = ?packet, "relaying packet from client to host");

                if let Err(err) = stream.send_input_raw(packet).await {
                    warn!(error = %err, "failed to relay input from client to host");
                }
            })
        })
    });

    // Wait for the channel to open
    let (on_control_open_sender, on_control_open) = oneshot::channel::<()>();
    control.on_open({
        let control = control.clone();
        Box::new(move || {
            let control = control.clone();

            Box::pin(async move {
                let ready_state = control.ready_state();
                debug!(ready_state = ?ready_state, "control channel ready state");

                if ready_state == RTCDataChannelState::Open {
                    debug!("notifying host to client relay that the control channel is open");
                    let _ = on_control_open_sender.send(());
                }
            })
        })
    });

    // Spawn from host to client relay
    spawn({
        let control_config = control_config.clone();
        async move {
            let _ = on_control_open.await;

            while let Some(packet) = clientbound_control_receiver.recv().await
                && !matches!(control.ready_state(), RTCDataChannelState::Closed)
            {
                let mut buffer = [0; _];
                let len = match packet.serialize(&control_config, &mut buffer) {
                    Ok(value) => value,
                    Err(err) => {
                        warn!(error = %err, "failed to relay control packet from host to client");
                        continue;
                    }
                };
                let buffer = &buffer[0..len];

                if let Err(err) = control.send(&Bytes::copy_from_slice(buffer)).await {
                    warn!(error = %err, "failed to relay control packet from host to client");
                }
            }

            debug!("stopping relaying from host to client");
        }.instrument(debug_span!("relay: host to client"))
    });

    debug!("added events for simple control channel");
}

pub async fn add_enet_control_channel(
    peer: &RTCPeerConnection,
    moonlight_stream: Arc<MoonlightStream>,
    mut clientbound_control_receiver: Receiver<ControlPacket>,
    control_config: &ControlPacketConfig,
) {
    let control = peer
        .create_data_channel(
            "control",
            Some(RTCDataChannelInit {
                ordered: Some(false),
                max_retransmits: Some(0),
                protocol: Some("enet".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    let base_time = StdInstant::now();

    let control_host = Arc::new(Mutex::new(
        ControlHost::new(
            Instant::from_std(base_time),
            ControlHostConfig {
                peer_channel_count: EnetChannel::CHANNEL_COUNT,
                peer_count: 1,
            },
            Arc::new(DisabledCryptoBackend) as _,
        )
        .expect("new control host"),
    ));

    let poll_notify = Arc::new(Notify::new());

    let addr = SocketAddr::new(Ipv4Addr::new(192, 168, 178, 1).into(), 47999);

    // Spawn receiving messages
    control.on_message({
        let poll_notify = poll_notify.clone();
        let control_host = control_host.clone();

        Box::new(move |message| {
            let poll_notify = poll_notify.clone();
            let control_host = control_host.clone();

            Box::pin(async move {
                trace!(packet = ?message, "received control channel enet message");

                if message.is_string {
                    warn!(packet = ?message.data, "received string over enet control channel! dropping message");
                    return;
                }
                
                {
                let mut control_host = control_host.lock().await;

                if let Err(err)= control_host.handle_input(ControlHostInput::Receive {
                    now: Instant::from_std(base_time),
                    addr,
                    data: &message.data,
                }) {
                    warn!(error = %err, "failed to call handle_input with Receive on ControlHost");
                }
                }

                poll_notify.notify_one();
            })
        })
    });

    // Wait for the channel to open
    let (on_control_open_sender, on_control_open) = oneshot::channel::<()>();
    control.on_open({
        let control = control.clone();

        Box::new(move || {
            let control = control.clone();

            Box::pin(async move {
                let ready_state = control.ready_state();
                debug!(ready_state = ?ready_state, "control channel ready state");

                if ready_state == RTCDataChannelState::Open {
                    debug!("notifying host to client relay that the control channel is open");
                    let _ = on_control_open_sender.send(());
                }
            })
        })
    });

    // Wait for enet connect, handled in driver
    let (on_enet_connect_sender, on_enet_open) = oneshot::channel::<()>();

    // Spawn host to client messages
    spawn({
        let control_config = control_config.clone();
        let control = control.clone();
        let control_host = control_host.clone();
        let poll_notify = poll_notify.clone();

        async move {
            let _ = on_enet_open.await;
            debug!("starting host to client enet relay");

            while let Some(packet) = clientbound_control_receiver.recv().await
                && !matches!(control.ready_state(), RTCDataChannelState::Closed)
            {
                let (channel_id, kind) = packet.channel(control_config.server_version);

                {
                    let mut control_host = control_host.lock().await;

                    // Broadcast to all configured peers
                    debug!(packet = ?packet, "broadcasting enet packet");
                    for id in control_host.configured_peers().collect::<Vec<_>>() {
                        trace!(peer_id = ?id, packet = ?packet, "relaying packet from host to client");
                        if let Err(err) = control_host.send(id, channel_id, kind, packet.clone()) {
                            warn!(error = %err, "failed to relay control packet from host to client");
                        }
                    }
                }

                poll_notify.notify_one();
            }

            debug!("stopping relaying from host to client");
        }.instrument(debug_span!("relay: host to client"))
    });

    // Spawn ControlHost Driver
    spawn({
        let stream = moonlight_stream.clone();
        let control_config = control_config.clone();
        let control = control.clone();
        let control_host = control_host.clone();
        let mut on_enet_open_sender = Some(on_enet_connect_sender);

        let poll_notify = poll_notify.clone();
        async move {
            let _ = on_control_open.await;

            loop {
                let output = {
                    let mut control_host = control_host.lock().await;

                    match control_host.poll_output() {
                        Ok(value) => value,
                        Err(err) => {
                            warn!(error = %err, "failed to call poll_output on ControlHost. Stopping stream!");

                            // Stop stream
                            stream.stop().await;
                            return;
                        }
                    }
                };

                let timeout = match output {
                    ControlHostOutput::Action(ControlHostAction::SendUdp { addr: _, data }) => {
                        trace!(packet = ?data, "sending enet data");

                        if let Err(err) = control.send(&data.into()).await {
                            warn!(error = %err, "failed to send message on control stream. Stopping stream!");

                            stream.stop().await;
                            return;
                        }
                        continue;
                    }
                    ControlHostOutput::Action(ControlHostAction::Timeout(timeout)) => {
                        timeout.to_std(base_time)
                    }
                    ControlHostOutput::Event(ControlHostEvent::Connected {
                        id,
                        sunshine_connect_data: _,
                    }) => {
                        {
                        // Configure peer
                            let mut control_host = control_host.lock().await;

                            if let Err(err)=  control_host.configure_peer(id, ControlPeerConfig {
                                encryption: None,
                                packets: control_config.clone(),
                                role: ControlPeerRole::Server,
                            }) {
                                error!(peer_id = ?id, error = ?err, "failed to configure peer");
                                // TODO: stop the stream
                                return;
                            }

                            debug!(peer_id = ?id, "enet control stream connected, configured peer");
                        }

                        if let Some(on_open) = on_enet_open_sender.take() {
                            let _ = on_open.send(());
                            debug!("calling on enet open");
                        } else {
                            debug!("on enet open was already called");
                        }

                        continue;
                    }
                    ControlHostOutput::Event(ControlHostEvent::Receive {
                        id,
                        channel_id,
                        packet,
                    }) => {
                        trace!(packet = ?packet, "received packet over enet");
                        if let Err(err)=  stream.send_input_raw(packet.clone()).await{
                            warn!(error = %err, packet = ?packet, "failed to relay packet from client to host");
                        }
                        continue;
                    }
                    ControlHostOutput::Event(ControlHostEvent::Disconnected { id }) => {
                        // TODO: this should stop video traffic and connect should start it again!
                        info!(peer_id = ?id, "enet control stream disconnected.");
                        continue;
                    }
                };

                select! {
                    _ = sleep_until(timeout.into()) => {}
                    _ = poll_notify.notified() => {}
                }

                let timeout = Instant::from_std(base_time);

                {
                    let mut control_host = control_host.lock().await;

                    if let Err(err) = control_host.handle_input(ControlHostInput::Timeout(timeout))
                    {
                        error!(error = %err, "failed to call handle_input with Timeout on ControlHost");
                        // TODO
                        return;
                    }
                }
            }
        }.instrument(debug_span!("relay: enet driver"))
    });
}
