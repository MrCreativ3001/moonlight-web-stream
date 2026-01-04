use std::{
    future::ready,
    pin::Pin,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bytes::Bytes;
use common::{
    StreamSettings,
    api_bindings::{
        RtcIceCandidate, RtcSdpType, RtcSessionDescription, StreamClientMessage,
        StreamServerMessage, StreamSignalingMessage, TransportChannelId,
    },
    config::{PortRange, WebRtcConfig},
    ipc::{ServerIpcMessage, StreamerIpcMessage},
};
use log::{debug, error, info, trace, warn};
use moonlight_common::stream::{
    bindings::{
        AudioConfig, DecodeResult, OpusMultistreamConfig, SupportedVideoFormats, VideoDecodeUnit,
    },
    video::VideoSetup,
};
use tokio::{
    runtime::Handle,
    spawn,
    sync::{
        Mutex,
        mpsc::{Receiver, Sender, channel},
    },
    time::sleep,
};
use webrtc::{
    api::{
        APIBuilder, interceptor_registry::register_default_interceptors, media_engine::MediaEngine,
        setting_engine::SettingEngine,
    },
    data_channel::{RTCDataChannel, data_channel_message::DataChannelMessage},
    ice::udp_network::{EphemeralUDP, UDPNetwork},
    ice_transport::{
        ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
        ice_connection_state::RTCIceConnectionState,
    },
    interceptor::registry::Registry,
    peer_connection::{
        RTCPeerConnection,
        configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::{sdp_type::RTCSdpType, session_description::RTCSessionDescription},
    },
};

use crate::{
    convert::{
        from_webrtc_sdp, into_webrtc_ice, into_webrtc_ice_candidate, into_webrtc_network_type,
    },
    transport::{
        InboundPacket, OutboundPacket, TransportChannel, TransportError, TransportEvent,
        TransportEvents, TransportSender,
        webrtc::{
            audio::{WebRtcAudio, register_audio_codecs},
            sender::register_header_extensions,
            video::{WebRtcVideo, register_video_codecs},
        },
    },
};

pub const TIMEOUT_DURATION: Duration = Duration::from_secs(10);

mod audio;
mod sender;
mod video;

struct WebRtcInner {
    peer: Arc<RTCPeerConnection>,
    event_sender: Sender<TransportEvent>,
    general_channel: Arc<RTCDataChannel>,
    stats_channel: Mutex<Option<Arc<RTCDataChannel>>>,
    video: Mutex<WebRtcVideo>,
    audio: Mutex<WebRtcAudio>,
    // Timeout / Terminate
    pub timeout_terminate_request: Mutex<Option<Instant>>,
}

pub async fn new(
    config: &WebRtcConfig,
    video_frame_queue_size: usize,
    audio_sample_queue_size: usize,
) -> Result<(WebRTCTransportSender, WebRTCTransportEvents), anyhow::Error> {
    // -- Configure WebRTC
    let rtc_config = RTCConfiguration {
        ice_servers: config
            .ice_servers
            .clone()
            .into_iter()
            .map(into_webrtc_ice)
            .collect(),
        ..Default::default()
    };
    let mut api_settings = SettingEngine::default();

    if let Some(PortRange { min, max }) = config.port_range {
        match EphemeralUDP::new(min, max) {
            Ok(udp) => {
                api_settings.set_udp_network(UDPNetwork::Ephemeral(udp));
            }
            Err(err) => {
                warn!("[Stream]: Invalid port range in config: {err:?}");
            }
        }
    }
    if let Some(mapping) = config.nat_1to1.as_ref() {
        api_settings.set_nat_1to1_ips(
            mapping.ips.clone(),
            into_webrtc_ice_candidate(mapping.ice_candidate_type),
        );
    }
    api_settings.set_network_types(
        config
            .network_types
            .iter()
            .copied()
            .map(into_webrtc_network_type)
            .collect(),
    );

    api_settings.set_include_loopback_candidate(config.include_loopback_candidates);

    // -- Register media codecs
    // TODO: register them based on the sdp
    let mut api_media = MediaEngine::default();
    register_audio_codecs(&mut api_media).expect("failed to register audio codecs");
    register_video_codecs(&mut api_media).expect("failed to register video codecs");
    register_header_extensions(&mut api_media).expect("failed to register header extensions");

    // -- Build Api
    let mut api_registry = Registry::new();

    // Use the default set of Interceptors
    api_registry = register_default_interceptors(api_registry, &mut api_media)
        .expect("failed to register webrtc default interceptors");

    let api = APIBuilder::new()
        .with_setting_engine(api_settings)
        .with_media_engine(api_media)
        .with_interceptor_registry(api_registry)
        .build();

    let (event_sender, event_receiver) = channel::<TransportEvent>(20);

    let peer = Arc::new(api.new_peer_connection(rtc_config).await?);

    let general_channel = peer.create_data_channel("general", None).await?;

    let runtime = Handle::current();
    let this_owned = Arc::new(WebRtcInner {
        peer: peer.clone(),
        event_sender,
        general_channel,
        stats_channel: Mutex::new(None),
        video: Mutex::new(WebRtcVideo::new(
            runtime.clone(),
            Arc::downgrade(&peer),
            video_frame_queue_size,
        )),
        audio: Mutex::new(WebRtcAudio::new(
            runtime,
            Arc::downgrade(&peer),
            audio_sample_queue_size,
        )),
        timeout_terminate_request: Mutex::new(None),
    });

    let this = Arc::downgrade(&this_owned);

    // -- Connection state
    peer.on_ice_connection_state_change(create_event_handler(
        this.clone(),
        async move |this, state| {
            this.on_ice_connection_state_change(state).await;
        },
    ));
    peer.on_peer_connection_state_change(create_event_handler(
        this.clone(),
        async move |this, state| {
            this.on_peer_connection_state_change(state).await;
        },
    ));

    // -- Signaling
    peer.on_ice_candidate(create_event_handler(
        this.clone(),
        async move |this, candidate| {
            this.on_ice_candidate(candidate).await;
        },
    ));

    // -- Data Channels
    peer.on_data_channel(create_event_handler(
        this.clone(),
        async move |this, channel| {
            this.on_data_channel(channel).await;
        },
    ));

    drop(peer);

    Ok((
        WebRTCTransportSender {
            inner: this_owned.clone(),
        },
        WebRTCTransportEvents { event_receiver },
    ))
}

// It compiling...
#[allow(clippy::complexity)]
fn create_event_handler<F, Args>(
    inner: Weak<WebRtcInner>,
    f: F,
) -> Box<
    dyn FnMut(Args) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> + Send + Sync + 'static,
>
where
    Args: Send + 'static,
    F: AsyncFn(Arc<WebRtcInner>, Args) + Send + Sync + Clone + 'static,
    for<'a> F::CallRefFuture<'a>: Send,
{
    Box::new(move |args: Args| {
        let inner = inner.clone();
        let Some(inner) = inner.upgrade() else {
            debug!("Called webrtc event handler while the main type is already deallocated");
            return Box::pin(ready(())) as Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
        };

        let future = f.clone();
        Box::pin(async move {
            future(inner, args).await;
        }) as Pin<Box<dyn Future<Output = ()> + Send + 'static>>
    })
        as Box<
            dyn FnMut(Args) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>
                + Send
                + Sync
                + 'static,
        >
}
#[allow(clippy::complexity)]
fn create_channel_message_handler(
    inner: Weak<WebRtcInner>,
    channel: TransportChannel,
) -> Box<
    dyn FnMut(DataChannelMessage) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>
        + Send
        + Sync
        + 'static,
> {
    create_event_handler(inner, async move |inner, message: DataChannelMessage| {
        let Some(packet) = InboundPacket::deserialize(channel, &message.data) else {
            return;
        };

        if let Err(err) = inner
            .event_sender
            .send(TransportEvent::RecvPacket(packet))
            .await
        {
            warn!("Failed to dispatch RecvPacket event: {err:?}");
        };
    })
}

impl WebRtcInner {
    // -- Handle Connection State
    async fn on_ice_connection_state_change(self: &Arc<Self>, _state: RTCIceConnectionState) {}
    async fn on_peer_connection_state_change(self: Arc<Self>, state: RTCPeerConnectionState) {
        #[allow(clippy::collapsible_if)]
        if matches!(state, RTCPeerConnectionState::Closed) {
            if let Err(err) = self.event_sender.send(TransportEvent::Closed).await {
                warn!("Failed to send peer closed event to stream: {err:?}");
                self.request_terminate().await;
            };
        } else if matches!(
            state,
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Disconnected
        ) {
            self.request_terminate().await;
        } else {
            self.clear_terminate_request().await;
        }
    }

    // -- Handle Signaling
    async fn send_answer(&self) -> bool {
        let local_description = match self.peer.create_answer(None).await {
            Err(err) => {
                warn!("[Signaling]: failed to create answer: {err:?}");
                return false;
            }
            Ok(value) => value,
        };

        if let Err(err) = self
            .peer
            .set_local_description(local_description.clone())
            .await
        {
            warn!("[Signaling]: failed to set local description: {err:?}");
            return false;
        }

        debug!(
            "[Signaling] Sending Local Description as Answer: {:?}",
            local_description.sdp
        );

        if let Err(err) = self
            .event_sender
            .send(TransportEvent::SendIpc(StreamerIpcMessage::WebSocket(
                StreamServerMessage::WebRtc(StreamSignalingMessage::Description(
                    RtcSessionDescription {
                        ty: from_webrtc_sdp(local_description.sdp_type),
                        sdp: local_description.sdp,
                    },
                )),
            )))
            .await
        {
            warn!("Failed to send local description (answer) via web socket from peer: {err:?}");
        }

        true
    }
    async fn send_offer(&self) -> bool {
        let local_description = match self.peer.create_offer(None).await {
            Err(err) => {
                error!("[Signaling]: failed to create offer: {err:?}");
                return false;
            }
            Ok(value) => value,
        };

        if let Err(err) = self
            .peer
            .set_local_description(local_description.clone())
            .await
        {
            error!("[Signaling]: failed to set local description: {err:?}");
            return false;
        }

        debug!(
            "[Signaling] Sending Local Description as Offer: {:?}",
            local_description.sdp
        );

        if let Err(err) = self
            .event_sender
            .send(TransportEvent::SendIpc(StreamerIpcMessage::WebSocket(
                StreamServerMessage::WebRtc(StreamSignalingMessage::Description(
                    RtcSessionDescription {
                        ty: from_webrtc_sdp(local_description.sdp_type),
                        sdp: local_description.sdp,
                    },
                )),
            )))
            .await
        {
            warn!("Failed to send local description (offer) via web socket from peer: {err:?}");
        };

        true
    }

    async fn on_ws_message(&self, message: StreamClientMessage) {
        match message {
            StreamClientMessage::StartStream {
                bitrate,
                packet_size,
                fps,
                width,
                height,
                play_audio_local,
                video_supported_formats,
                video_colorspace,
                video_color_range_full,
                hdr,
            } => {
                let video_supported_formats = SupportedVideoFormats::from_bits(video_supported_formats).unwrap_or_else(|| {
                    warn!("Failed to deserialize SupportedVideoFormats: {video_supported_formats}, falling back to only H264");
                    SupportedVideoFormats::H264
                });
                {
                    let mut video = self.video.lock().await;
                    video.set_codecs(video_supported_formats).await;
                }

                // TODO: check peer for supported formats via sdp

                if let Err(err) = self
                    .event_sender
                    .send(TransportEvent::StartStream {
                        settings: StreamSettings {
                            bitrate,
                            packet_size,
                            fps,
                            width,
                            height,
                            video_supported_formats,
                            video_color_range_full,
                            video_colorspace: video_colorspace.into(),
                            play_audio_local,
                            hdr,
                        },
                    })
                    .await
                {
                    error!("Failed to send start stream: {err}");
                }
            }
            StreamClientMessage::WebRtc(StreamSignalingMessage::Description(description)) => {
                debug!("[Signaling] Received Remote Description: {:?}", description);

                let description = match &description.ty {
                    RtcSdpType::Offer => RTCSessionDescription::offer(description.sdp),
                    RtcSdpType::Answer => RTCSessionDescription::answer(description.sdp),
                    RtcSdpType::Pranswer => RTCSessionDescription::pranswer(description.sdp),
                    _ => {
                        error!(
                            "[Signaling]: failed to handle RTCSdpType {:?}",
                            description.ty
                        );
                        return;
                    }
                };

                let Ok(description) = description else {
                    error!("[Signaling]: Received invalid RTCSessionDescription");
                    return;
                };

                let remote_ty = description.sdp_type;
                if let Err(err) = self.peer.set_remote_description(description).await {
                    error!("[Signaling]: failed to set remote description: {err:?}");
                    return;
                }

                // Send an answer (local description) if we got an offer
                if remote_ty == RTCSdpType::Offer {
                    self.send_answer().await;
                }
            }
            StreamClientMessage::WebRtc(StreamSignalingMessage::AddIceCandidate(description)) => {
                debug!("[Signaling] Received Ice Candidate");

                if let Err(err) = self
                    .peer
                    .add_ice_candidate(RTCIceCandidateInit {
                        candidate: description.candidate,
                        sdp_mid: description.sdp_mid,
                        sdp_mline_index: description.sdp_mline_index,
                        username_fragment: description.username_fragment,
                    })
                    .await
                {
                    warn!("[Signaling]: failed to add ice candidate: {err:?}");
                }
            }
            _ => {}
        }
    }

    async fn on_ice_candidate(&self, candidate: Option<RTCIceCandidate>) {
        let Some(candidate) = candidate else {
            return;
        };

        let Ok(candidate_json) = candidate.to_json() else {
            return;
        };

        debug!(
            "[Signaling] Sending Ice Candidate: {}",
            candidate_json.candidate
        );

        let message =
            StreamServerMessage::WebRtc(StreamSignalingMessage::AddIceCandidate(RtcIceCandidate {
                candidate: candidate_json.candidate,
                sdp_mid: candidate_json.sdp_mid,
                sdp_mline_index: candidate_json.sdp_mline_index,
                username_fragment: candidate_json.username_fragment,
            }));

        if let Err(err) = self
            .event_sender
            .send(TransportEvent::SendIpc(StreamerIpcMessage::WebSocket(
                message,
            )))
            .await
        {
            error!("Failed to send web socket message from peer: {err:?}");
        };
    }

    async fn on_data_channel(self: Arc<Self>, channel: Arc<RTCDataChannel>) {
        let label = channel.label();
        debug!("adding data channel: \"{label}\"");

        let inner = Arc::downgrade(&self);

        match label {
            "stats" => {
                let mut stats = self.stats_channel.lock().await;

                channel.on_close({
                    let this = Arc::downgrade(&self);

                    Box::new(move ||{
                        let this = this.clone();

                        Box::pin(async move {
                            let Some(this) = this.upgrade() else {
                                warn!("Failed to close stats channel because the main type is already deallocated");
                                return;
                            };

                            this.close_stats().await;
                        })
                    })
                });

                *stats = Some(channel);
            }
            "mouse_reliable" | "mouse_absolute" | "mouse_relative" => {
                channel.on_message(create_channel_message_handler(
                    inner,
                    TransportChannel(TransportChannelId::MOUSE_ABSOLUTE),
                ));
            }
            "touch" => {
                channel.on_message(create_channel_message_handler(
                    inner,
                    TransportChannel(TransportChannelId::TOUCH),
                ));
            }
            "keyboard" => {
                channel.on_message(create_channel_message_handler(
                    inner,
                    TransportChannel(TransportChannelId::KEYBOARD),
                ));
            }
            "controllers" => {
                channel.on_message(create_channel_message_handler(
                    inner,
                    TransportChannel(TransportChannelId::CONTROLLERS),
                ));
            }
            _ if let Some(number) = label.strip_prefix("controller")
                && let Ok(id) = number.parse::<usize>()
                && id < InboundPacket::CONTROLLER_CHANNELS.len() =>
            {
                channel.on_message(create_channel_message_handler(
                    inner,
                    TransportChannel(InboundPacket::CONTROLLER_CHANNELS[id]),
                ));
            }
            _ => {}
        };
    }

    async fn close_stats(&self) {
        let mut stats = self.stats_channel.lock().await;

        *stats = None;
    }

    // -- Termination
    async fn request_terminate(self: &Arc<Self>) {
        let this = self.clone();

        let mut terminate_request = self.timeout_terminate_request.lock().await;
        *terminate_request = Some(Instant::now());
        drop(terminate_request);

        spawn(async move {
            sleep(TIMEOUT_DURATION + Duration::from_millis(200)).await;

            let now = Instant::now();

            let terminate_request = this.timeout_terminate_request.lock().await;
            if let Some(terminate_request) = *terminate_request
                && (now - terminate_request) > TIMEOUT_DURATION
            {
                info!("Stopping because of timeout");
                if let Err(err) = this.event_sender.send(TransportEvent::Closed).await {
                    warn!("Failed to send that the peer should close: {err:?}");
                };
            }
        });
    }
    async fn clear_terminate_request(&self) {
        let mut request = self.timeout_terminate_request.lock().await;

        *request = None;
    }
}

pub struct WebRTCTransportEvents {
    event_receiver: Receiver<TransportEvent>,
}

#[async_trait]
impl TransportEvents for WebRTCTransportEvents {
    async fn poll_event(&mut self) -> Result<TransportEvent, TransportError> {
        trace!("Polling WebRTCEvents");
        self.event_receiver
            .recv()
            .await
            .ok_or(TransportError::Closed)
    }
}

pub struct WebRTCTransportSender {
    inner: Arc<WebRtcInner>,
}

#[async_trait]
impl TransportSender for WebRTCTransportSender {
    async fn setup_video(&self, setup: VideoSetup) -> i32 {
        let mut video = self.inner.video.lock().await;
        if video.setup(&self.inner, setup).await {
            0
        } else {
            -1
        }
    }
    async fn send_video_unit<'a>(
        &'a self,
        unit: &'a VideoDecodeUnit<'a>,
    ) -> Result<DecodeResult, TransportError> {
        let mut video = self.inner.video.lock().await;
        Ok(video.send_decode_unit(unit).await)
    }

    async fn setup_audio(
        &self,
        audio_config: AudioConfig,
        stream_config: OpusMultistreamConfig,
    ) -> i32 {
        let mut audio = self.inner.audio.lock().await;

        audio.setup(&self.inner, audio_config, stream_config).await
    }
    async fn send_audio_sample(&self, data: &[u8]) -> Result<(), TransportError> {
        let mut audio = self.inner.audio.lock().await;

        audio.send_audio_sample(data).await;

        Ok(())
    }

    async fn send(&self, packet: OutboundPacket) -> Result<(), TransportError> {
        let mut buffer = Vec::new();

        let Some((channel, range)) = packet.serialize(&mut buffer) else {
            warn!("Failed to serialize packet: {packet:?}");
            return Ok(());
        };

        let bytes = Bytes::from(buffer);
        let bytes = bytes.slice(range);

        match channel.0 {
            TransportChannelId::GENERAL => match self.inner.general_channel.send(&bytes).await {
                Ok(_) => {}
                Err(webrtc::Error::ErrDataChannelNotOpen) => {
                    return Err(TransportError::ChannelClosed);
                }
                _ => {}
            },
            TransportChannelId::STATS => {
                let stats = self.inner.stats_channel.lock().await;
                if let Some(stats) = stats.as_ref() {
                    match stats.send(&bytes).await {
                        Ok(_) => {}
                        Err(webrtc::Error::ErrDataChannelNotOpen) => {
                            return Err(TransportError::ChannelClosed);
                        }
                        _ => {}
                    }
                } else {
                    return Err(TransportError::ChannelClosed);
                }
            }
            _ => {
                warn!("Cannot send data on channel {channel:?}");
                return Err(TransportError::ChannelClosed);
            }
        }
        Ok(())
    }

    async fn on_ipc_message(&self, message: ServerIpcMessage) -> Result<(), TransportError> {
        if let ServerIpcMessage::WebSocket(message) = message {
            self.inner.on_ws_message(message).await;
        }
        Ok(())
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.inner
            .peer
            .close()
            .await
            .map_err(|err| TransportError::Implementation(err.into()))?;

        Ok(())
    }
}
