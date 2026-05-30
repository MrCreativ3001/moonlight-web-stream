//! See https://www.ietf.org/archive/id/draft-murillo-whep-03.html

use actix_web::HttpRequest;
use actix_web::web::{Bytes, Data, Query};
use actix_web::{
    HttpResponse, HttpResponseBuilder, delete, get, http::StatusCode, http::header, options, patch,
    post,
};
use async_trait::async_trait;
use common::config::PortRange;
use moonlight_common::ServerVersion;
use moonlight_common::crypto::disabled::DisabledCryptoBackend;
use moonlight_common::crypto::rustcrypto::RustCryptoBackend;
use moonlight_common::http::Request;
use moonlight_common::http::pair::PairingCryptoBackend;
use moonlight_common::stream::audio::{AudioConfig, AudioFrame, OpusMultistreamConfig};
use moonlight_common::stream::control::ActiveGamepads;
use moonlight_common::stream::proto::control::packet::{
    ControlPacket, ControlPacketConfig, EnetChannel, PacketDirection,
};
use moonlight_common::stream::proto::control::peer::{ControlHost, ControlHostConfig};
use moonlight_common::stream::tokio::{
    MoonlightStream, MoonlightStreamError, MoonlightStreamHandler,
};
use moonlight_common::stream::video::{
    ColorRange, ColorSpace, DecodeResult, VideoDecodeUnit, VideoFormat, VideoFormats, VideoSetup,
};
use moonlight_common::stream::{
    AesIv, AesKey, EncryptionFlags, MoonlightStreamSettings, StreamingConfig,
};
use moonlight_common::webrtc::MoonlightWebRtcSession;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::spawn;
use tokio::sync::mpsc::{Sender, channel};
use tokio::sync::{Mutex, Notify};
use tokio::time::sleep;
use tracing::{Instrument, debug, info, instrument, trace, warn};
use webrtc::api::APIBuilder;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::data_channel_state::RTCDataChannelState;
use webrtc::ice::udp_network::{EphemeralUDP, UDPNetwork};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtcp::payload_feedbacks::receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate;
use webrtc::rtp::codecs::h264::H264Payloader;
use webrtc::rtp::codecs::h265::{HevcPayloader, RTP_OUTBOUND_MTU};
use webrtc::rtp::extension::HeaderExtension;
use webrtc::rtp::extension::playout_delay_extension::PlayoutDelayExtension;
use webrtc::rtp::header::Header;
use webrtc::rtp::packet::Packet;
use webrtc::rtp::packetizer::Payloader;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTCRtpHeaderExtensionCapability, RTPCodecType,
};
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::rtp_transceiver::{RTCPFeedback, RTCRtpTransceiverInit};
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;

use crate::api::stream::whep::convert::{into_webrtc_ice_candidate, into_webrtc_network_type};
use crate::api::stream::whep::dynamic_ice_servers::load_dynamic_ice_servers;
use crate::api::stream::whep::video::{codec_to_video_format, video_format_to_codec};
use crate::app::App;
use crate::app::host::HostId;
use crate::app::{AppError, user::AuthenticatedUser};

mod control;
mod convert;
mod dynamic_ice_servers;
mod video;
mod webrtc_wrapper;

// This works very well for testing: https://webrtc.player.eyevinn.technology/?type=whep
// whep test url, replace appid and hostId: http://localhost:8080/api/host/stream/whep?hostId=4156725524&appid=881448767&mode=1920x1080x60&bitrate=10000

#[options("")]
pub async fn whep_options(_user: AuthenticatedUser) -> Result<HttpResponse, AppError> {
    // https://www.ietf.org/archive/id/draft-murillo-whep-03.html#section-4-10

    Ok(HttpResponseBuilder::new(StatusCode::OK)
        // allow making requests for websites / services
        .insert_header((
            header::ACCESS_CONTROL_ALLOW_METHODS,
            "OPTIONS, GET, POST, PATCH, DELETE",
        ))
        .insert_header((header::ACCESS_CONTROL_ALLOW_HEADERS, "*"))
        .insert_header((
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "Content-Type, Authorization",
        ))
        .insert_header((header::ACCESS_CONTROL_EXPOSE_HEADERS, "Location"))
        // Insert accept post, like the spec says
        .append_header(("Accept-Post", "application/sdp"))
        .finish())
}

#[get("")]
pub async fn whep_get() -> HttpResponse {
    HttpResponseBuilder::new(StatusCode::METHOD_NOT_ALLOWED).finish()
}

fn opus_codec() -> RTCRtpCodecCapability {
    RTCRtpCodecCapability {
        mime_type: MIME_TYPE_OPUS.to_owned(),
        clock_rate: 48000,
        channels: 2,
        sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
        rtcp_feedback: vec![RTCPFeedback {
            // negative acknowledgement
            typ: "nack".to_string(),
            parameter: "".to_string(),
        }],
    }
}

fn create_media_engine() -> MediaEngine {
    // The media engine contains all supported codecs this peer has
    let mut media_engine = MediaEngine::default();

    // register extensions
    const PLAYOUT_DELAY_URI: &str = "http://www.webrtc.org/experiments/rtp-hdrext/playout-delay";
    const COLOR_SPACE_URI: &str = "http://www.webrtc.org/experiments/rtp-hdrext/color-space";

    media_engine
        .register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: PLAYOUT_DELAY_URI.to_string(),
            },
            RTPCodecType::Video,
            None,
        )
        .expect("register playout delay extension");
    media_engine
        .register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: COLOR_SPACE_URI.to_string(),
            },
            RTPCodecType::Video,
            None,
        )
        .expect("register color space extension");
    media_engine
        .register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: PLAYOUT_DELAY_URI.to_string(),
            },
            RTPCodecType::Audio,
            None,
        )
        .expect("register playout delay extension");

    // register audio
    media_engine
        .register_codec(
            RTCRtpCodecParameters {
                capability: opus_codec(),
                payload_type: 111,
                ..Default::default()
            },
            RTPCodecType::Audio,
        )
        .expect("register audio opus codec");

    // register video
    for (i, format) in VideoFormat::all().into_iter().enumerate() {
        let Some(codec) = video_format_to_codec(format) else {
            debug!(format = ?format, "failed to convert format into codec");
            continue;
        };

        debug!(format = ?format, codec = ?codec, "adding codec to media engine");

        media_engine
            .register_codec(
                RTCRtpCodecParameters {
                    capability: codec,
                    payload_type: 96 + i as u8,
                    ..Default::default()
                },
                RTPCodecType::Video,
            )
            .expect("register video codec");
    }

    media_engine
}

struct VideoTrack {
    track: Arc<TrackLocalStaticRTP>,
    payloader: Box<dyn Payloader + Send + Sync>,
}

struct StreamHandler {
    audio_track: Mutex<Option<Arc<TrackLocalStaticRTP>>>,
    audio_sequence_number: AtomicU16,
    video_formats: VideoFormats,
    video: Mutex<Option<VideoTrack>>,
    video_sequence_number: AtomicU16,
    video_need_idr: Arc<AtomicBool>,
    peer: Arc<RTCPeerConnection>,
    client_control_sender: Sender<ControlPacket>,
}

#[async_trait]
impl MoonlightStreamHandler for StreamHandler {
    async fn setup_video(&self, setup: VideoSetup) -> Result<(), MoonlightStreamError> {
        // Check video format
        if !self.video_formats.contains(setup.format.into_formats()) {
            todo!();
        }

        // Create video track
        let codec = video_format_to_codec(setup.format).expect("webrtc video codec");
        let video_track = Arc::new(TrackLocalStaticRTP::new(
            codec.clone(),
            "video".to_string(),
            "moonlight".to_string(),
        ));

        let video_sender = self.peer.add_track(video_track.clone()).await.unwrap();

        // Feedback
        spawn({
            let need_idr = self.video_need_idr.clone();

            async move {
                let mut buffer = [0; 1500];

                while let Ok((packets, _)) = video_sender.read(&mut buffer).await {
                    for packet in packets {
                        let packet = packet.as_any();

                        if let Some(_) = packet.downcast_ref::<PictureLossIndication>() {
                            debug!("got picture loss indication, set need idr flag");
                            need_idr.store(true, Ordering::Release);
                        } else if let Some(_) =
                            packet.downcast_ref::<ReceiverEstimatedMaximumBitrate>()
                        {
                            // TODO
                        }
                    }
                }
            }
        });

        let payloader = if setup.format.contained_in(VideoFormats::MASK_H264) {
            Box::new(H264Payloader::default()) as Box<dyn Payloader + Send + Sync>
        } else if setup.format.contained_in(VideoFormats::MASK_H265) {
            Box::new(HevcPayloader::default()) as Box<dyn Payloader + Send + Sync>
        } else {
            todo!()
        };

        {
            let mut video_guard = self.video.lock().await;
            *video_guard = Some(VideoTrack {
                track: video_track.clone(),
                payloader,
            });
        }

        info!(setup = ?setup, codec = ?codec, "finished video track setup");

        Ok(())
    }
    async fn on_video_frame(&self, frame: VideoDecodeUnit<&[u8]>) -> DecodeResult {
        let timestamp = (frame.timestamp.as_secs_f64() * 90000.0) as u32;

        let mut video_guard = self.video.lock().await;
        let video = video_guard.as_mut().expect("video track");

        if video.track.all_binding_paused().await {
            trace!("audio track all binding paused");
            // Don't send any packets when the track is paused because we don't want to increment the sequence number
            return DecodeResult::Ok;
        }

        let mut payloads = Vec::with_capacity(10);

        // Each buffer is one nal
        for buffer in &frame.buffers {
            let nal_payloads = video
                .payloader
                .payload(RTP_OUTBOUND_MTU, &Bytes::copy_from_slice(buffer.data))
                .unwrap();

            payloads.extend(nal_payloads);
        }

        let len = payloads.len();
        for (i, payload) in payloads.into_iter().enumerate() {
            if let Err(err) = video
                .track
                .write_rtp_with_extensions(
                    &Packet {
                        header: Header {
                            version: 2,
                            // Marker needs to mark the end of one frame
                            marker: i == len - 1,
                            sequence_number: self
                                .video_sequence_number
                                .fetch_add(1, Ordering::Acquire),
                            timestamp,
                            // TODO: this needs to match
                            payload_type: 96,
                            ..Default::default()
                        },
                        payload,
                    },
                    &[HeaderExtension::PlayoutDelay(PlayoutDelayExtension {
                        min_delay: 0,
                        max_delay: 0,
                    })],
                )
                .await
            {
                warn!(error = %err, "failed to send video packet");
            }
        }

        // Check if idr is needed
        if let Ok(_) =
            self.video_need_idr
                .compare_exchange(true, false, Ordering::Acquire, Ordering::Acquire)
        {
            info!("requesting idr");
            DecodeResult::NeedIdr
        } else {
            DecodeResult::Ok
        }
    }

    async fn setup_audio(
        &self,
        audio_config: AudioConfig,
        opus_config: OpusMultistreamConfig,
    ) -> Result<(), MoonlightStreamError> {
        // TODO: what audio format is used?

        // Create audio track
        let mut codec = opus_codec();
        codec.clock_rate = opus_config.sample_rate;
        codec.channels = opus_config.channel_count as u16;

        let audio_track = Arc::new(TrackLocalStaticRTP::new(
            codec.clone(),
            "audio".to_string(),
            "moonlight".to_string(),
        ));

        let audio_sender = self.peer.add_track(audio_track.clone()).await.unwrap();

        // Feedback
        spawn(async move {
            let mut buffer = [0; 1500];

            while let Ok((_packets, _)) = audio_sender.read(&mut buffer).await {
                // do nothing, because we'll just have to poll packets to dequeue them
            }
        });

        {
            let mut audio_guard = self.audio_track.lock().await;
            *audio_guard = Some(audio_track.clone());
        }

        info!(audio_config = ?audio_config, opus_config = ?opus_config, codec = ?codec, "finished audio track setup");

        Ok(())
    }
    async fn on_audio_frame(&self, frame: AudioFrame<&[u8]>) {
        let timestamp = (frame.timestamp.as_secs_f64() * 48000.0) as u32;

        let audio_guard = self.audio_track.lock().await;
        // The audio track is initialized before the moonlight stream starts
        let audio_track = audio_guard.as_ref().expect("audio track");

        if audio_track.all_binding_paused().await {
            trace!("audio track all binding paused");
            // Don't send any packets when the track is paused because we don't want to increment the sequence number
            return;
        }

        trace!(len = ?frame.buffer.len(), timestamp = ?frame.timestamp, "audio frame");

        // Opus doesn't need any special payloading: https://github.com/webrtc-rs/webrtc/blob/6b94718e23111df28125f96af4b0de8cbb3dfd0d/rtp/src/codecs/opus/mod.rs#L9-L24
        if let Err(err) = audio_track
            .write_rtp_with_extensions(
                &Packet {
                    header: Header {
                        version: 2,
                        sequence_number: self.audio_sequence_number.fetch_add(1, Ordering::Acquire),
                        timestamp,
                        payload_type: 111,
                        ..Default::default()
                    },
                    payload: Bytes::copy_from_slice(frame.buffer),
                },
                &[],
            )
            .await
        {
            warn!(error = %err, "failed to send audio frame");
        }
    }

    async fn on_control_packet(&self, packet: ControlPacket) {
        match &packet {
            ControlPacket::HdrMode { enabled, sunshine } => {
                // TODO: use the color space(hdr) extension, this seems to only be used on keyframes -> request one https://webrtc.googlesource.com/src/+/refs/heads/main/docs/native-code/rtp-hdrext/color-space
            }
            _ => {}
        }

        if let Err(err) = self.client_control_sender.send(packet).await {
            warn!(error = %err, packet = ?err.0, "failed to relay control packet to client");
        }
    }

    async fn on_stop(&self) {
        info!("closing webrtc peer because the stream was closed");

        if let Err(err) = self.peer.close().await {
            warn!(error = %err, "error whilst closing peer because the stream was stopped");
        }
    }
}

#[post("")]
#[instrument(skip(app, user, req, session_description), fields(user = %user.id()))]
pub async fn whep_post(
    app: Data<App>,
    mut user: AuthenticatedUser,
    req: HttpRequest,
    session_description: String,
) -> Result<HttpResponse, AppError> {
    debug!(req = ?req, session_description = ?session_description, "whep request");

    let session = MoonlightWebRtcSession::from_str(&session_description).unwrap();

    let Some(host_id) = session.host_id else {
        return Err(AppError::HostNotFound);
    };
    let host_id = HostId(host_id);

    // Get host
    let mut host = user.host(host_id).await?;
    let host = host.use_host(&mut user).await?;

    if !host.is_paired().await.unwrap() {
        return Err(AppError::HostNotPaired);
    }

    // Create offer based on the sdp
    let offer = RTCSessionDescription::offer(session_description).unwrap();

    // Look for supported Moonlight extensions
    let mut supports_control_stream_simple = false;
    let mut supports_control_stream_enet = false;
    // TODO

    // -- Create WebRtc peer
    // Create settings
    let mut setting_engine = SettingEngine::default();
    if let Some(PortRange { min, max }) = app.config().webrtc.port_range {
        match EphemeralUDP::new(min, max) {
            Ok(udp) => {
                setting_engine.set_udp_network(UDPNetwork::Ephemeral(udp));
            }
            Err(err) => {
                warn!("[Stream]: Invalid port range in config: {err:?}");
            }
        }
    }
    if let Some(mapping) = app.config().webrtc.nat_1to1.as_ref() {
        setting_engine.set_nat_1to1_ips(
            mapping.ips.clone(),
            into_webrtc_ice_candidate(mapping.ice_candidate_type),
        );
    }
    setting_engine.set_network_types(
        app.config()
            .webrtc
            .network_types
            .iter()
            .copied()
            .map(into_webrtc_network_type)
            .collect(),
    );

    setting_engine.set_include_loopback_candidate(app.config().webrtc.include_loopback_candidates);

    // Create media engine
    let mut media_engine = create_media_engine();

    // Load ice servers
    let mut ice_servers = app.config().webrtc.ice_servers.clone();

    // Load dynamic ice servers and append them to the current ice servers
    let dynamic_ice_servers = load_dynamic_ice_servers(&app.config().webrtc).await;
    ice_servers.extend_from_slice(&dynamic_ice_servers);
    // TODO: turn / stun creds: https://www.ietf.org/archive/id/draft-murillo-whep-03.html#section-4.4

    // Interceptor Registry
    let interceptor_registry =
        register_default_interceptors(Registry::new(), &mut media_engine).unwrap();

    let api = APIBuilder::new()
        .with_interceptor_registry(interceptor_registry)
        .with_media_engine(media_engine)
        .with_setting_engine(setting_engine)
        .build();

    // Configure peer
    let peer = api
        .new_peer_connection(RTCConfiguration {
            ice_servers: ice_servers
                .into_iter()
                .map(|x| RTCIceServer {
                    username: x.username,
                    credential: x.credential,
                    urls: x.urls,
                })
                .collect(),
            ..Default::default()
        })
        .await
        .unwrap();
    let peer = Arc::new(peer);

    info!("created server webrtc peer");

    info!("querying client for supported video and audio codecs");

    // -- Query sdp about video, audio and potential microphone
    let mut microphone_enabled = false;
    let mut audio_config = Some(AudioConfig::STEREO);
    let mut supported_video_formats = VideoFormats::empty();

    let offer_parsed = offer.unmarshal().unwrap();

    for media in offer_parsed.media_descriptions {
        let kind = &media.media_name.media;

        let payload_types: Vec<u8> = media
            .media_name
            .formats
            .iter()
            .filter_map(|f| f.parse::<u8>().ok())
            .collect();

        // Map payload type -> codec
        for payload_type in &payload_types {
            if let Some(attr) = media.attributes.iter().find(|a| {
                a.key == "rtpmap"
                    && a.value
                        .as_ref()
                        .map(|v| v.starts_with(&format!("{} ", payload_type)))
                        .unwrap_or(false)
            }) {
                // value example: "111 opus/48000/2"
                if let Some(value) = &attr.value {
                    let mut parts = value.split_whitespace().nth(1).unwrap_or("").split('/');
                    let codec_name = parts.next().unwrap_or("");
                    let clock_rate = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
                    let channels = parts.next().unwrap_or("1").parse::<u16>().unwrap_or(1);

                    if let Some(format) = codec_to_video_format(&RTCRtpCodecCapability {
                        mime_type: format!("{kind}/{}", codec_name.to_uppercase()),
                        clock_rate,
                        channels,
                        ..Default::default()
                    }) {
                        debug!(
                            kind = ?kind, codec_name = ?codec_name, clock_rate = clock_rate, channels = channels,
                            "added video codec",
                        );

                        supported_video_formats |= format.into_formats();
                    } else {
                        debug!(
                            kind = ?kind, codec_name = ?codec_name, clock_rate = clock_rate, channels = channels,
                            "unknown codec",
                        );
                    }
                    // TODO: detect audio
                }
            }
        }
    }

    debug!(
        supported_video_formats = %supported_video_formats,
        audio_config = ?audio_config,
        "found codecs"
    );

    // Cancel connection if no audio or video format was detected as supported
    let Some(audio_config) = audio_config else {
        // TODO
        todo!();
    };

    if supported_video_formats.is_empty() {
        // TODO
        todo!();
    }

    // TODO: remove this limitation
    supported_video_formats &= VideoFormats::H264;

    let mut settings = MoonlightStreamSettings {
        width: session.width,
        height: session.height,
        fps: session.fps,
        fps_x100: session.fps * 100,
        bitrate: session.bitrate,
        packet_size: 2048,
        // There's not need to encrypt video
        encryption_flags: EncryptionFlags::AUDIO | EncryptionFlags::FOUNDATION_MICROPHONE,
        streaming_remotely: StreamingConfig::Auto,
        sops: true,
        hdr: session.hdr,
        supported_video_formats,
        // TODO: what color space / range? is this in the sdp?
        color_space: ColorSpace::Rec709,
        color_range: ColorRange::Limited,
        local_audio_play_mode: session.local_audio_play_mode,
        // TODO: what audio config?
        audio_config: AudioConfig::STEREO,
        gamepads_attached: ActiveGamepads::empty(),
        gamepads_persist_after_disconnect: false,
        enable_mic: microphone_enabled,
    };

    let server_version = host.version().await.unwrap();
    let gfe_version = host.gfe_version().await.unwrap();
    let server_codec_mode_support = host.server_codec_mode_support().await.unwrap();
    settings
        .adjust_for_server(server_version, &gfe_version, server_codec_mode_support)
        .unwrap();

    let aes_key = AesKey::new_random(&RustCryptoBackend)?;
    let aes_iv = AesIv::new_random(&RustCryptoBackend)?;

    // Start moonlight stream
    info!(settings = ?settings, "starting stream");

    let (client_control_sender, mut client_control_receiver) = channel(10);

    let moonlight_handler = StreamHandler {
        audio_track: Default::default(),
        audio_sequence_number: AtomicU16::new(0),
        video: Default::default(),
        video_sequence_number: AtomicU16::new(0),
        video_need_idr: Default::default(),
        peer: peer.clone(),
        video_formats: supported_video_formats,
        client_control_sender,
    };

    let config = host
        .start_stream(
            session.app_id,
            &settings,
            aes_key,
            aes_iv,
            MoonlightStream::launch_query_parameters(),
        )
        .await?;

    let moonlight_stream = Arc::new(
        MoonlightStream::connect(
            config,
            settings,
            Arc::new(RustCryptoBackend) as _,
            Arc::new(moonlight_handler),
        )
        .await
        .unwrap(),
    );

    // IMPORTANT: at this point audio and video tracks are already added to the peer because MoonlightStream::connect will call setup_audio and setup_video
    // -> both tracks are added to the stream

    info!("started moonlight stream");

    // -- Create control channel based on support
    let control_config = ControlPacketConfig::new(ServerVersion::new(7, 0, 0, 0), true)
        .expect("control packet config");

    match (supports_control_stream_simple, supports_control_stream_enet) {
        (_, true) => {
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

            // TODO
            // let control_host = ControlHost::new(
            //     Instant::now(),
            //     ControlHostConfig {
            //         peer_channel_count: EnetChannel::CHANNEL_COUNT,
            //         peer_count: 1,
            //     },
            //     DisabledCryptoBackend,
            // )
            // .expect("new control host");

            todo!();
        }
        (true, false) => {
            let control = peer.create_data_channel("control", None).await.unwrap();
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
                            return;
                        };

                        if let Err(err) = stream.send_input_raw(packet).await {
                            warn!(error = %err, "failed to relay input from client to host");
                        }
                    })
                })
            });

            // Spawn from host to client relay
            spawn({
                let control_config = control_config.clone();
                async move {
                    while let Some(packet) = client_control_receiver.recv().await
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
                }
            });
        }
        (false, false) => {
            // do nothing because the peer doesn't support control channel
        }
    };

    // -- Register webrtc peer listeners
    peer.on_peer_connection_state_change(Box::new({
        let moonlight_stream = moonlight_stream.clone();

        move |state: RTCPeerConnectionState| {
            let moonlight_stream = moonlight_stream.clone();

            Box::pin(async move {
                if matches!(state, RTCPeerConnectionState::Failed) {
                    info!("stopping stream because the webrtc peer state is failed");

                    moonlight_stream.stop().await;
                }
            })
        }
    }));

    // Set remote description
    peer.set_remote_description(offer).await.unwrap();

    info!("configured server webrtc peer, waiting for ice gathering to complete");

    // Get ice gathering receiver
    let mut ice_complete = peer.gathering_complete_promise().await;

    // Complete negotiation
    let answer = peer.create_answer(None).await.unwrap();
    peer.set_local_description(answer.clone()).await.unwrap();

    // Wait for ice gathering to complete
    let _ = ice_complete.recv().await;

    // Use the local description with video and audio tracks, control channel and all ice candidates included
    let answer = peer.local_description().await.unwrap();

    info!("ice gathering completed, sending answer to client");

    debug!(answer = ?answer, "sending answer to client");

    Ok(HttpResponse::Created()
        // TODO: add session location
        // TODO: add ice servers / configuration, see whep
        .insert_header(("Location", "/api/host/stream/whep/SESSION"))
        .content_type("application/sdp")
        .body(answer.sdp))
}

#[patch("")]
pub async fn whep_patch(user: AuthenticatedUser) -> Result<HttpResponse, AppError> {
    // TODO: implement trickle ice

    todo!()
}

#[delete("")]
#[instrument(skip(user), fields(user = %user.id()))]
pub async fn whep_delete(user: AuthenticatedUser) -> Result<HttpResponse, AppError> {
    todo!()
}
