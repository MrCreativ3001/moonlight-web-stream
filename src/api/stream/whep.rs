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
use moonlight_common::webrtc::launch::WebRtcLaunchRequest;
use moonlight_common::webrtc::sdp::WebRtcClientFeatures;
use moonlight_common::webrtc::sdp::sdp::Session;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::spawn;
use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, Notify};
use tokio::time::sleep;
use tracing::{Instrument, debug, info, instrument, trace, warn};
use webrtc::api::APIBuilder;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtcp::payload_feedbacks::receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate;
use webrtc::rtp::codecs::h264::H264Payloader;
use webrtc::rtp::codecs::h265::RTP_OUTBOUND_MTU;
use webrtc::rtp::extension::HeaderExtension;
use webrtc::rtp::extension::playout_delay_extension::PlayoutDelayExtension;
use webrtc::rtp::header::Header;
use webrtc::rtp::packet::Packet;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTCRtpHeaderExtensionCapability, RTPCodecType,
};
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;

use crate::api::stream::whep::dynamic_ice_servers::load_dynamic_ice_servers;
use crate::api::stream::whep::video::{codec_to_video_format, video_format_to_codec};
use crate::app::App;
use crate::app::host::HostId;
use crate::app::{AppError, user::AuthenticatedUser};

mod control;
mod dynamic_ice_servers;
mod video;
mod webrtc_wrapper;

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
            RTPCodecType::Video,
            None,
        )
        .expect("register playout delay extension");
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
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                    ..Default::default()
                },
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

struct StreamHandler {
    audio_track: Mutex<Option<Arc<TrackLocalStaticRTP>>>,
    audio_sequence_number: AtomicU16,
    video_formats: VideoFormats,
    video_track: Mutex<Option<Arc<TrackLocalStaticRTP>>>,
    video_sequence_number: AtomicU16,
    peer: Arc<RTCPeerConnection>,
    control_peer_sender: Sender<ControlPacket>,
}

#[async_trait]
impl MoonlightStreamHandler for StreamHandler {
    async fn setup_video(&self, setup: VideoSetup) -> Result<(), MoonlightStreamError> {
        // Check video format
        if !self.video_formats.contains(setup.format.into_formats()) {
            todo!();
        }

        // Create video track
        let video_track = Arc::new(TrackLocalStaticRTP::new(
            video_format_to_codec(setup.format).expect("webrtc video codec"),
            "video".to_string(),
            "moonlight".to_string(),
        ));

        let video_sender = self.peer.add_track(video_track.clone()).await.unwrap();

        // Feedback
        spawn(async move {
            let mut buffer = [0; 1500];

            while let Ok((packets, _)) = video_sender.read(&mut buffer).await {
                for packet in packets {
                    let packet = packet.as_any();

                    if let Some(_) = packet.downcast_ref::<PictureLossIndication>() {
                        // TODO
                    } else if let Some(_) = packet.downcast_ref::<ReceiverEstimatedMaximumBitrate>()
                    {
                        // TODO
                    }
                }
            }
        });

        {
            let mut video_guard = self.video_track.lock().await;
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
                    &Packet {
                        header: Header {
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
        // TODO
        Ok(())
    }
    async fn on_audio_frame(&self, frame: AudioFrame<&[u8]>) {
        let timestamp = (frame.timestamp.as_secs_f64() * 48000.0) as u32;

        let audio_guard = self.audio_track.lock().await;
        // The audio track is initialized before the moonlight stream starts
        let audio_track = audio_guard.as_ref().expect("audio track");

        // Opus doesn't need any special payloading: https://github.com/webrtc-rs/webrtc/blob/6b94718e23111df28125f96af4b0de8cbb3dfd0d/rtp/src/codecs/opus/mod.rs#L9-L24
        if let Err(err) = audio_track
            .write_rtp_with_extensions(
                &Packet {
                    header: Header {
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

    // Interceptor Registry
    let interceptor_registry =
        register_default_interceptors(Registry::new(), &mut media_engine).unwrap();

    let api = APIBuilder::new().build();

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

    // Set remote description so that we can query for codecs
    peer.set_remote_description(offer).await.unwrap();

    info!("added video and audio tracks");

    // -- Query sdp about video, audio and potential microphone
    let mut microphone_enabled = false;
    let mut audio_config = None;
    let mut supported_video_formats = VideoFormats::empty();

    let transceivers = peer.get_transceivers().await;
    for transceiver in transceivers {
        let kind = transceiver.kind();

        if kind == RTPCodecType::Audio {
            let direction = transceiver.direction();

            if !microphone_enabled {
                microphone_enabled = matches!(
                    direction,
                    RTCRtpTransceiverDirection::Sendonly | RTCRtpTransceiverDirection::Sendrecv
                );
            }

            if matches!(
                direction,
                RTCRtpTransceiverDirection::Recvonly | RTCRtpTransceiverDirection::Sendrecv
            ) {
                // audio send transceiver
                let sender = transceiver.sender().await;
                let parameters = sender.get_parameters().await;

                for codec in parameters.rtp_parameters.codecs {
                    // TODO: search for the config
                    if codec.capability.mime_type == MIME_TYPE_OPUS {
                        audio_config = Some(AudioConfig::STEREO);
                    }
                }
            }
        } else if kind == RTPCodecType::Video {
            let direction = transceiver.direction();

            if matches!(
                direction,
                RTCRtpTransceiverDirection::Recvonly | RTCRtpTransceiverDirection::Sendrecv
            ) {
                // video send transceiver
                let sender = transceiver.sender().await;
                let parameters = sender.get_parameters().await;

                for codec in parameters.rtp_parameters.codecs {
                    let Some(codec) = codec_to_video_format(&codec.capability) else {
                        continue;
                    };

                    supported_video_formats |= codec.into_formats();
                }
            }
        }
    }

    // Cancel connection if no audio or video format was detected as supported
    let Some(audio_config) = audio_config else {
        // TODO
        todo!();
    };

    if supported_video_formats.is_empty() {
        // TODO
        todo!();
    }

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
        audio_config: query.preferred_audio,
        gamepads_attached: ActiveGamepads::empty(),
        gamepads_persist_after_disconnect: false,
        enable_mic: microphone_enabled,
    };

    let aes_key = AesKey::new_random(&RustCryptoBackend)?;
    let aes_iv = AesIv::new_random(&RustCryptoBackend)?;

    // Start moonlight stream
    info!(settings = ?settings, "starting stream");

    let moonlight_handler = StreamHandler {
        audio_track: Default::default(),
        audio_sequence_number: AtomicU16::new(0),
        video_track: Default::default(),
        video_sequence_number: AtomicU16::new(0),
        peer: peer.clone(),
    };

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
        MoonlightStream::connect(
            config,
            settings,
            RustCryptoBackend,
            Arc::new(moonlight_handler),
        )
        .await
        .unwrap(),
    );

    // IMPORTANT: at this point audio and video tracks are already added to the peer because MoonlightStream::connect will call setup_audio and setup_video
    // -> both tracks are added to the stream

    info!("started moonlight stream");

    // Create control channel based on support
    let control_config = ControlPacketConfig::new(ServerVersion::new(7, 0, 0, 0), true)
        .expect("control packet config");

    match (
        client_features.control_stream_simple,
        client_features.control_stream_enet,
    ) {
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

            let control_host = ControlHost::new(
                Instant::now(),
                ControlHostConfig {
                    peer_channel_count: EnetChannel::CHANNEL_COUNT,
                    peer_count: 1,
                },
                DisabledCryptoBackend,
            )
            .expect("new control host");

            todo!();
        }
        (true, false) => {
            let control = peer.create_data_channel("control", None).await.unwrap();
            let stream = moonlight_stream.clone();

            todo!();
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
    peer.gathering_complete_promise().await;

    // Use the local description with video and audio tracks, control channel and all ice candidates included
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
