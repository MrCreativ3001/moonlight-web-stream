//! See https://www.ietf.org/archive/id/draft-murillo-whep-03.html

use actix_web::HttpRequest;
use actix_web::web::{Bytes, Data, Query};
use actix_web::{
    HttpResponse, HttpResponseBuilder, delete, get, http::StatusCode, http::header, options, patch,
    post,
};
use async_trait::async_trait;
use moonlight_common::ServerVersion;
use moonlight_common::crypto::disabled::DisabledCryptoBackend;
use moonlight_common::crypto::rustcrypto::RustCryptoBackend;
use moonlight_common::http::Request;
use moonlight_common::http::pair::PairingCryptoBackend;
use moonlight_common::stream::audio::{AudioConfig, AudioFrame, OpusMultistreamConfig};
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
use moonlight_common::webrtc::launch::WebRtcLaunchRequest;
use moonlight_common::webrtc::sdp::WebRtcClientFeatures;
use moonlight_common::webrtc::sdp::sdp::Session;
use rtc::data_channel::RTCDataChannelInit;
use rtc::interceptor::Registry;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::MIME_TYPE_OPUS;
use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use rtc::rtp::codec::h264::H264Payloader;
use rtc::rtp::codec::h265::RTP_OUTBOUND_MTU;
use rtc::rtp::extension::HeaderExtension;
use rtc::rtp::extension::playout_delay_extension::PlayoutDelayExtension;
use rtc::rtp::packetizer::Payloader;
use rtc::rtp::{Header, Packet};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RTCRtpHeaderExtensionCapability, RtpCodecKind,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::spawn;
use tokio::sync::{Mutex, Notify};
use tokio::time::sleep;
use tracing::{debug, info, instrument, trace, warn};
use webrtc::data_channel::DataChannelEvent;
use webrtc::media_stream::Track;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder, RTCIceGatheringState, RTCIceServer, RTCPeerConnectionState,
    RTCSessionDescription, SettingEngine, register_default_interceptors,
};

use crate::api::stream::whep::dynamic_ice_servers::load_dynamic_ice_servers;
use crate::api::stream::whep::video::{codec_to_video_format, video_format_to_codec};
use crate::app::App;
use crate::app::host::HostId;
use crate::app::{AppError, user::AuthenticatedUser};

mod dynamic_ice_servers;
mod video;

// This works very well for testing: https://webrtc.player.eyevinn.technology/?type=whep

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
        // Insert accept post, like the spec says
        .append_header(("Accept-Post", "application/sdp"))
        // This server supports microphone
        // advertise this here so that the client can include the microphone track in it's offer
        .append_header(("X-Moonlight-Microphone", "true"))
        .finish())
}

#[get("")]
pub async fn whep_get() -> HttpResponse {
    HttpResponseBuilder::new(StatusCode::METHOD_NOT_ALLOWED).finish()
}

fn create_media_engine() -> MediaEngine {
    // The media engine contains all supported codecs this peer has
    let mut media_engine = MediaEngine::default();

    // register extensions
    const PLAYOUT_DELAY_URI: &str = "http://www.webrtc.org/experiments/rtp-hdrext/playout-delay";

    media_engine
        .register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: PLAYOUT_DELAY_URI.to_string(),
            },
            RtpCodecKind::Video,
            None,
        )
        .expect("register playout delay extension");
    media_engine
        .register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: PLAYOUT_DELAY_URI.to_string(),
            },
            RtpCodecKind::Audio,
            None,
        )
        .expect("register playout delay extension");

    // register audio
    media_engine
        .register_codec(
            RTCRtpCodecParameters {
                rtp_codec: RTCRtpCodec {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 111,
            },
            RtpCodecKind::Audio,
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
                    rtp_codec: codec,
                    payload_type: 96 + i as u8,
                },
                RtpCodecKind::Video,
            )
            .expect("register video codec");
    }

    media_engine
}

struct StreamHandler {
    client_features: WebRtcClientFeatures,
    audio_track: Mutex<Option<Arc<TrackLocalStaticRTP>>>,
    audio_ssrc: u32,
    audio_sequence_number: AtomicU16,
    video_track: Mutex<Option<Arc<TrackLocalStaticRTP>>>,
    video_ssrc: u32,
    video_sequence_number: AtomicU16,
    ice_gathering_complete: Notify,
    peer: Mutex<Option<Arc<dyn PeerConnection>>>,
    stream: Mutex<Option<Arc<MoonlightStream>>>,
}

#[async_trait]
impl PeerConnectionEventHandler for StreamHandler {
    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let label = track.label().await;
        let kind = track.kind().await;
        debug!(label = label, kind = ?kind, "on track");

        if let Some((mic_stream_id, mic_track_id)) = &self.client_features.microphone_msid
            && &track.stream_id().await == mic_stream_id
            && &track.track_id().await == mic_track_id
        {
            todo!();
            // TODO: microphone?
        }
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        info!(state = %state, "changing ice gathering state");

        if matches!(state, RTCIceGatheringState::Complete) {
            self.ice_gathering_complete.notify_one();
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        // TODO: stop session if needed
        info!(state = %state, "changing connection state");
    }
}

#[async_trait]
impl MoonlightStreamHandler for StreamHandler {
    async fn setup_video(&self, setup: VideoSetup) -> Result<(), MoonlightStreamError> {
        let peer_guard = self.peer.lock().await;
        let peer = peer_guard.as_ref().expect("webrtc peer");

        // Supported video formats
        let mut supported_video_formats = VideoFormats::empty();
        let transceivers = peer.get_transceivers();
        for codec in video_parameters.rtp_parameters.codecs {
            if let Some(format) = codec_to_video_format(&codec.rtp_codec) {
                supported_video_formats |= format.into_formats();
            } else {
                warn!(codec = ?codec, "unknown negotiated video codec");
            }
        }
        debug!(
            formats = %supported_video_formats,
            "collected negotiated video formats"
        );
        if supported_video_formats.is_empty() {
            if let Err(err) = peer.close().await {
                warn!(error = %err, "failed to close peer");
            }
            return Err(AppError::WebRtcClientCodecNotSupported);
        }

        // Create video track
        let video_track = Arc::new(TrackLocalStaticRTP::new({
            MediaStreamTrack::new(
                "moonlight".to_string(),
                "video".to_string(),
                "video".to_string(),
                RtpCodecKind::Video,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(self.video_ssrc),
                        ..Default::default()
                    },
                    codec: video_format_to_codec(setup.format)
                        .expect("failed to get video codec for webrtc peer"),
                    ..Default::default()
                }],
            )
        }));

        let video_sender = peer.add_track(video_track.clone()).await.unwrap();

        {
            let mut video_guard = handler.video_track.lock().await;
            *video_guard = Some(video_track.clone());
        }

        Ok(())
    }
    async fn on_video_frame(&self, frame: VideoDecodeUnit<&[u8]>) -> DecodeResult {
        let timestamp = (frame.timestamp.as_secs_f64() * 90000.0) as u32;

        let mut video_guard = self.video_track.lock().await;
        let video_track = video_guard.as_mut().expect("video track");

        let mut payloads = Vec::with_capacity(10);

        // Each buffer is one nal
        // TODO: move payloader into StreamHandler
        let mut payloader = H264Payloader::default();

        for buffer in &frame.buffers {
            let nal_payloads = payloader
                .payload(RTP_OUTBOUND_MTU, &Bytes::copy_from_slice(buffer.data))
                .unwrap();

            payloads.extend(nal_payloads);
        }

        let len = payloads.len();
        for (i, payload) in payloads.into_iter().enumerate() {
            if let Err(err) = video_track
                .write_rtp_with_extensions(
                    Packet {
                        header: Header {
                            ssrc: self.video_ssrc,
                            // TODO: select correct payload type
                            payload_type: 96,
                            // Marker needs to mark the end of one frame
                            marker: i == len - 1,
                            sequence_number: self
                                .video_sequence_number
                                .fetch_add(1, Ordering::Acquire),
                            timestamp,
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

        DecodeResult::Ok
    }

    async fn setup_audio(
        &self,
        audio_config: AudioConfig,
        opus_config: OpusMultistreamConfig,
    ) -> Result<(), MoonlightStreamError> {
        let audio_sender = peer.add_track(audio_track.clone()).await.unwrap();
        Ok(())
    }
    async fn on_audio_frame(&self, frame: AudioFrame<&[u8]>) {
        let timestamp = (frame.timestamp.as_secs_f64() * 48000.0) as u32;

        let audio_guard = self.audio_track.lock().await;
        // The audio track is initialized before the moonlight stream starts
        let audio_track = audio_guard.as_mut().expect("audio track");

        // Opus doesn't need any special payloading: https://github.com/webrtc-rs/webrtc/blob/6b94718e23111df28125f96af4b0de8cbb3dfd0d/rtp/src/codecs/opus/mod.rs#L9-L24
        if let Err(err) = audio_track
            .write_rtp_with_extensions(
                Packet {
                    header: Header {
                        ssrc: self.audio_ssrc,
                        // TODO: select correct payload type
                        payload_type: 111,
                        sequence_number: self.audio_sequence_number.fetch_add(1, Ordering::Acquire),
                        timestamp,
                        ..Default::default()
                    },
                    payload: Bytes::copy_from_slice(frame.buffer),
                },
                &[HeaderExtension::PlayoutDelay(PlayoutDelayExtension::new(
                    0, 0,
                ))],
            )
            .await
        {
            warn!(error = %err, "failed to send audio frame");
        }
    }

    async fn on_control_packet(&self, packet: ControlPacket) {
        // TODO: send packets over data channel
    }

    async fn on_stop(&self) {
        // TODO: close the peer
    }
}

#[post("")]
#[instrument(skip(app, user), fields(user = %user.id()))]
pub async fn whep_post(
    app: Data<App>,
    mut user: AuthenticatedUser,
    req: HttpRequest,
    session_description_raw: Bytes,
) -> Result<HttpResponse, AppError> {
    let query = req.query_string();
    let query = match WebRtcLaunchRequest::from_query_params(&query) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                error = %err,
                "failed to parse query parameters for launch whep endpoint"
            );
            return Err(AppError::BadRequest);
        }
    };

    let Some(host_id) = query.web_host_id else {
        return Err(AppError::HostNotFound);
    };
    let host_id = HostId(host_id);

    // Get host
    let mut host = user.host(host_id).await?;
    let host = host.use_host(&mut user).await?;

    if !host.is_paired().await.unwrap() {
        return Err(AppError::HostNotPaired);
    }

    // Check Session
    let mut session_description = match Session::parse(&session_description_raw) {
        Ok(value) => value,
        Err(err) => {
            warn!(error = %err, "failed to parse session description");
            return Err(AppError::BadRequest);
        }
    };

    let client_features = WebRtcClientFeatures::from_session(&session_description);
    WebRtcClientFeatures::remove_from_session(&mut session_description);
    info!(client_features = ?client_features, "client features");

    // Create offer based on the modified sdp
    let mut session_description_raw = Vec::new();
    session_description
        .write(&mut session_description_raw)
        .unwrap();
    let offer = RTCSessionDescription::offer(
        String::from_utf8(session_description_raw).expect("valid utf8 session description"),
    )
    .unwrap();

    // Generate ssrc's
    let mut video_ssrc = [0; _];
    RustCryptoBackend.random_bytes(&mut video_ssrc)?;
    let video_ssrc = u32::from_ne_bytes(video_ssrc);

    let mut audio_ssrc = [0; _];
    RustCryptoBackend.random_bytes(&mut audio_ssrc)?;
    let audio_ssrc = u32::from_ne_bytes(audio_ssrc);

    // Initialize stream handler
    let handler = Arc::new(StreamHandler {
        client_features,
        audio_track: Default::default(),
        audio_ssrc,
        audio_sequence_number: AtomicU16::new(0),
        video_track: Default::default(),
        video_ssrc,
        video_sequence_number: AtomicU16::new(0),
        ice_gathering_complete: Notify::new(),
        peer: Default::default(),
        stream: Default::default(),
    });

    // -- Create WebRtc peer
    // Create settings
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_include_loopback_candidate(app.config().webrtc.include_loopback_candidates);
    // TODO: finish settings

    // Create media engine
    let mut media_engine = create_media_engine();

    // Load ice servers
    let mut ice_servers = app.config().webrtc.ice_servers.clone();

    // Load dynamic ice servers and append them to the current ice servers
    let dynamic_ice_servers = load_dynamic_ice_servers(&app.config().webrtc).await;
    ice_servers.extend_from_slice(&dynamic_ice_servers);
    // TODO: turn / stun creds: https://www.ietf.org/archive/id/draft-murillo-whep-03.html#section-4.4

    // List all available udp ports
    // TODO: is there some way in which we don't need to check which ports to use?
    // TODO: check for ipv4 / ipv6
    let udp_addr = SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0);
    if let Some(port_range) = &app.config().webrtc.port_range {
        // check for available port
        todo!();
    }

    // Interceptor Registry
    let interceptor_registry =
        register_default_interceptors(Registry::new(), &mut media_engine).unwrap();

    // Configure peer
    let peer = PeerConnectionBuilder::<SocketAddr, _>::new()
        .with_setting_engine(setting_engine)
        .with_media_engine(media_engine)
        .with_interceptor_registry(interceptor_registry)
        .with_configuration(
            RTCConfigurationBuilder::new()
                .with_ice_servers(
                    ice_servers
                        .into_iter()
                        .map(|x| RTCIceServer {
                            username: x.username,
                            credential: x.credential,
                            urls: x.urls,
                        })
                        .collect(),
                )
                .build(),
        )
        .with_udp_addrs(vec![udp_addr])
        .with_handler(handler.clone())
        .build()
        .await
        .unwrap();

    info!("created server webrtc peer");

    // Set remote description so that we can query for codecs
    peer.set_remote_description(offer).await.unwrap();

    info!("added video and audio tracks");

    // -- Collect / generate settings

    // Check for opus codecs
    // TODO: get the opus config
    let audio_parameters = audio_sender.get_parameters().await.unwrap();
    debug!(webrtc_audio_parameters = ?audio_parameters, "webrtc audio parameters");
    let mut opus_supported = false;
    for codec in audio_parameters.rtp_parameters.codecs {
        opus_supported = true;
    }
    if !opus_supported {
        if let Err(err) = peer.close().await {
            warn!(error = %err, "failed to close peer");
        }
        return Err(AppError::WebRtcClientCodecNotSupported);
    }

    // Create audio track
    let audio_track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        "moonlight".to_string(),
        "audio".to_string(),
        "audio".to_string(),
        RtpCodecKind::Audio,
        vec![
            // Stereo
            RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(audio_ssrc),
                    ..Default::default()
                },
                codec: RTCRtpCodec {
                    mime_type: MIME_TYPE_OPUS.to_string(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "".to_string(),
                    rtcp_feedback: vec![],
                },
                ..Default::default()
            },
        ],
    )));

    // Set tracks on the Handler
    {
        let mut audio_guard = handler.audio_track.lock().await;
        *audio_guard = Some(audio_track.clone());
    }

    // Microphone support
    let microphone_enabled = handler.client_features.microphone_msid.is_some();

    let settings = MoonlightStreamSettings {
        width: query.mode_width,
        height: query.mode_height,
        fps: query.mode_fps,
        fps_x100: query.mode_fps,
        bitrate: query.bitrate_kbps,
        packet_size: 2048,
        // There's not need to encrypt video
        encryption_flags: EncryptionFlags::AUDIO | EncryptionFlags::FOUNDATION_MICROPHONE,
        streaming_remotely: StreamingConfig::Auto,
        sops: true,
        hdr: query.hdr,
        supported_video_formats,
        // TODO: what color space / range? is this in the sdp?
        color_space: ColorSpace::Rec709,
        color_range: ColorRange::Limited,
        local_audio_play_mode: query.local_audio_play_mode,
        audio_config: query.surround_audio_info,
        gamepads_attached: query.gamepads_attached,
        gamepads_persist_after_disconnect: query.gamepads_persist_after_disconnect,
        enable_mic: microphone_enabled,
    };

    let aes_key = AesKey::new_random(&RustCryptoBackend)?;
    let aes_iv = AesIv::new_random(&RustCryptoBackend)?;

    // Start moonlight stream
    info!(settings = ?settings, "starting stream");

    let config = host
        .start_stream(
            query.app_id,
            &settings,
            aes_key,
            aes_iv,
            MoonlightStream::launch_query_parameters(),
        )
        .await?;

    let moonlight_stream = Arc::new(
        MoonlightStream::connect(config, settings, RustCryptoBackend, handler.clone())
            .await
            .unwrap(),
    );

    // Set the stream
    {
        let mut stream_guard = handler.stream.lock().await;
        *stream_guard = Some(moonlight_stream.clone());
    }

    info!("started moonlight stream");

    // Create control channel based on support
    let control_config = ControlPacketConfig::new(ServerVersion::new(7, 0, 0, 0), true)
        .expect("control packet config");

    match (
        handler.client_features.control_stream_simple,
        handler.client_features.control_stream_enet,
    ) {
        (_, true) => {
            let control = peer
                .create_data_channel(
                    "control",
                    Some(RTCDataChannelInit {
                        ordered: false,
                        max_retransmits: Some(0),
                        protocol: "enet".to_string(),
                        ..Default::default()
                    }),
                )
                .await
                .unwrap();

            let control_host = ControlHost::new(
                Instant::now(),
                ControlHostConfig {
                    peer_channel_count: EnetChannel::CHANNEL_COUNT,
                    peer_count: 1,
                },
                DisabledCryptoBackend,
            )
            .expect("new control host");

            // TODO
        }
        (true, false) => {
            let control = peer.create_data_channel("control", None).await.unwrap();
            let stream = moonlight_stream.clone();

            spawn({
                let control = control.clone();

                async move {
                    while let Some(event) = control.poll().await {
                        #[allow(clippy::single_match)]
                        match event {
                            DataChannelEvent::OnMessage(message) => {
                                if message.is_string {
                                    continue;
                                }
                                let Some(packet) = ControlPacket::deserialize(
                                    PacketDirection::ServerBound,
                                    &control_config,
                                    &message.data,
                                ) else {
                                    continue;
                                };

                                if let Err(err) = stream.send_input_raw(packet.clone()).await {
                                    warn!(packet = ?packet, error = %err, "failed to send packet");
                                }
                            }
                            _ => {}
                        }
                    }
                }
            });
        }
        (false, false) => {
            // do nothing because the peer doesn't support control channel
        }
    };

    // Complete negotiation
    let answer = peer.create_answer(None).await.unwrap();
    peer.set_local_description(answer.clone()).await.unwrap();

    info!("configured server webrtc peer, waiting for ice gathering to complete");

    // Wait for ice gathering to complete
    handler.ice_gathering_complete.notified().await;

    // Use the local description with all ice candidates included
    let answer = peer.local_description().await.unwrap();

    info!("ice gathering completed, sending answer to client");

    debug!(answer = ?answer, "sending answer to client");

    // continue in a new thread
    spawn(async move {
        // TODO: wait until stop signal

        sleep(Duration::from_secs(5)).await;

        moonlight_stream.stop().await;
        peer.close().await.unwrap();
    });

    Ok(HttpResponse::Created()
        // TODO: add session location
        // TODO: add ice servers / configuration, see whep
        .insert_header(("Location", "TODO"))
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
