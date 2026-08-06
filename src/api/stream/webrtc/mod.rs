use crate::api::stream::webrtc::audio::AudioChannel;
use crate::api::stream::webrtc::control::ControlChannel;
use crate::api::stream::webrtc::ext_color_space::COLOR_SPACE_URI;
use crate::api::stream::webrtc::stream::webrtc_loop;
use crate::api::stream::webrtc::video::VideoChannel;
use crate::config::PortRange;
use actix_web::HttpRequest;
use actix_web::body::{BoxBody, MessageBody};
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::web::{Data, Path};
use actix_web::{
    HttpResponse, HttpResponseBuilder, delete, get, http::StatusCode, http::header, options, patch,
    post,
};
use moonlight_common::AppId;
use moonlight_common::crypto::rustcrypto::RustCryptoBackend;
use moonlight_common::stream::audio::AudioConfig;
use moonlight_common::stream::control::ActiveGamepads;
use moonlight_common::stream::proto::MoonlightStreamSetup;
use moonlight_common::stream::tokio::MoonlightStream;
use moonlight_common::stream::video::{
    ColorRange, ColorSpace, VideoCapabilities, VideoFormat, VideoFormats,
};
use moonlight_common::stream::{
    AesIv, AesKey, EncryptionFlags, MoonlightStreamSettings, StreamingConfig,
};
use moonlight_common::webrtc::WebRTCParseError;
use moonlight_common::webrtc::answer::WebRTCSessionAnswer;
use moonlight_common::webrtc::header::WebRTCLinkHeader;
use moonlight_common::webrtc::offer::WebRTCSessionOffer;
use moonlight_common::webrtc::sdp::Session;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{self};
use tokio::time::sleep;
use tokio::{select, spawn};
use tracing::{Instrument, debug, debug_span, error, info, instrument, warn};
use webrtc::api::APIBuilder;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::ice::udp_network::{EphemeralUDP, UDPNetwork};
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::RTCPFeedback;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTCRtpHeaderExtensionCapability, RTPCodecType,
};

use crate::api::stream::apply_role_restrictions;
use crate::api::stream::webrtc::convert::{into_webrtc_ice_candidate, into_webrtc_network_type};
use crate::api::stream::webrtc::ice_servers::generate_ice_servers;
use crate::app::App;
use crate::app::host::HostId;
use crate::app::stream::{ExternalStreamEvent, Stream, StreamId};
use crate::app::{AppError, user::AuthenticatedUser};

mod audio;
mod control;
mod convert;
mod ext_color_space;
mod ice_servers;
mod stream;
mod video;

pub async fn webrtc_middleware(
    mut req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let mut user = req.extract::<AuthenticatedUser>().await?;

    if !user
        .role()
        .await?
        .permissions()
        .await?
        .allow_transport_webrtc
    {
        return Err(AppError::Forbidden.into());
    }

    next.call(req).await
}

#[options("")]
pub async fn webrtc_options(
    app: Data<App>,
    _user: AuthenticatedUser,
) -> Result<HttpResponse, AppError> {
    let mut response = HttpResponseBuilder::new(StatusCode::OK);
    response.append_header(("Accept-Post", "application/sdp"));

    // Add ice servers
    let ice_servers = generate_ice_servers(&app).await?;
    for ice_server in ice_servers {
        let username = if ice_server.username.is_empty() {
            None
        } else {
            Some(ice_server.username)
        };
        let credential = if ice_server.credential.is_empty() {
            None
        } else {
            Some(ice_server.credential)
        };

        for url in ice_server.urls {
            let header_value = WebRTCLinkHeader::IceServer {
                url,
                username: username.clone(),
                credential: credential.clone(),
            };

            response.append_header((header::LINK, header_value.to_string()));
        }
    }

    Ok(response.finish())
}

#[get("")]
pub async fn webrtc_get(_user: AuthenticatedUser) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::MethodNotAllowed().finish())
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

fn create_media_engine(video_formats: &HashMap<VideoFormat, RTCRtpCodecParameters>) -> MediaEngine {
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
    for codec in video_formats.values() {
        media_engine
            .register_codec(codec.clone(), RTPCodecType::Video)
            .expect("register video codec");
    }

    media_engine
}

#[post("")]
#[instrument(skip(app, user, req, session_description), fields(user = %user.id()))]
pub async fn webrtc_post(
    app: Data<App>,
    mut user: AuthenticatedUser,
    req: HttpRequest,
    session_description: String,
) -> Result<HttpResponse, AppError> {
    if !user
        .role()
        .await?
        .permissions()
        .await?
        .allow_transport_webrtc
    {
        return Err(AppError::Forbidden);
    }

    debug!(req = ?req, session_description = ?session_description, "webrtc request");

    let offer_sdp =
        Session::parse(session_description.as_bytes()).map_err(WebRTCParseError::from)?;
    let session = WebRTCSessionOffer::from_sdp(&offer_sdp)?;
    debug!(moonlight_session = ?session, "moonlight session extensions", );

    let Some(host_id) = session.host_id else {
        return Err(AppError::HostNotFound);
    };
    let host_id = HostId(host_id);

    // Get host
    let mut host = user.host(host_id).await?;
    let host = host.use_host(&mut user).await?;

    if !host.is_paired().await? {
        return Err(AppError::HostNotPaired);
    }

    // Get app
    let app_id = AppId(session.app_id);

    // Create offer based on the sdp
    let offer = RTCSessionDescription::offer(session_description)?;

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

    setting_engine.set_ice_timeouts(None, Some(Duration::from_secs(10)), None);

    // Create video
    let mut video_channel = VideoChannel::new(
        &offer_sdp,
        session.preferred_codecs.unwrap_or(VideoFormats::all()),
    )?;

    // Create media engine
    let mut media_engine = create_media_engine(video_channel.supported_video_formats());

    let ice_servers = generate_ice_servers(&app).await?;

    // Interceptor Registry
    let interceptor_registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .expect("register default interceptors");

    let api = APIBuilder::new()
        .with_interceptor_registry(interceptor_registry)
        .with_media_engine(media_engine)
        .with_setting_engine(setting_engine)
        .build();

    // Configure peer
    let peer = api
        .new_peer_connection(RTCConfiguration {
            ice_servers: ice_servers
                .iter()
                .map(|x| RTCIceServer {
                    username: x.username.clone(),
                    credential: x.credential.clone(),
                    urls: x.urls.clone(),
                })
                .collect(),
            ..Default::default()
        })
        .await?;
    let peer = Arc::new(peer);

    info!("created server webrtc peer");

    info!("querying client for supported video and audio codecs");

    // Video Formats
    let supported_video_formats = video_channel
        .supported_video_formats()
        .keys()
        .fold(VideoFormats::empty(), |formats, format| {
            formats | format.into_formats()
        });

    // TODO: query microphone support
    let microphone_enabled = false;

    // TODO: query audio support
    let audio_config = Some(AudioConfig::STEREO);

    debug!(
        supported_video_formats = %supported_video_formats,
        audio_config = ?audio_config,
        "found codecs"
    );

    // Cancel connection if no audio or video format was detected as supported
    let Some(_audio_config) = audio_config else {
        error!("failed to start stream because no audio codec is supported by the client");
        return Err(AppError::WebRtcClientCodecNotSupported);
    };

    if supported_video_formats.is_empty() {
        error!("failed to start stream because no video codec is supported by the client");
        return Err(AppError::WebRtcClientCodecNotSupported);
    }

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

    // -- Get and Apply Permissions
    let permissions = user.role().await?.permissions().await?;
    apply_role_restrictions(&permissions, &mut settings);

    // Adjust settings
    let server_version = host.version().await?;
    let gfe_version = host.gfe_version().await?;
    let server_codec_mode_support = host.server_codec_mode_support().await?;
    settings.adjust_for_server(server_version, &gfe_version, server_codec_mode_support)?;

    // Generate key and iv
    let aes_key = AesKey::new_random(&RustCryptoBackend)?;
    let aes_iv = AesIv::new_random(&RustCryptoBackend)?;

    // Start moonlight stream
    info!(settings = ?settings, "starting stream");

    let apps = host.app_list().await?;
    let app_title = apps
        .into_iter()
        .find(|app| app.id == app_id)
        .map(|app| app.title);

    let config = host
        .start_stream(
            app_id,
            &settings,
            aes_key,
            aes_iv,
            // TODO: replace with normal `MoonlightStream::launch`
            MoonlightStreamSetup::launch_query_parameters(),
        )
        .await?;

    let moonlight_stream = match MoonlightStream::connect(
        config,
        settings,
        Arc::new(RustCryptoBackend),
        VideoCapabilities::default(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            error!(error = %err, "failed to start stream");
            return Err(err.into());
        }
    };

    // Add audio and video track forwarding
    let audio_channel = match AudioChannel::new_track(&moonlight_stream, &peer).await {
        Ok(value) => value,
        Err(err) => {
            error!(error = %err, "failed to add audio track to webrtc peer");

            peer.close().await?;
            return Err(err);
        }
    };
    if let Err(err) = video_channel
        .on_video_format_selected(moonlight_stream.video_setup(), &peer)
        .await
    {
        error!(error = %err, "failed to add video track to webrtc peer");

        peer.close().await?;
        return Err(err);
    }

    info!("started moonlight stream");

    // -- Create control channel based on support
    let result = if session.control_enet {
        ControlChannel::new_enet(&peer).await
    } else {
        ControlChannel::new_simple(&peer).await
    };
    let control_channel = match result {
        Err(err) => {
            error!("failed to add control stream to webrtc peer");

            peer.close().await?;
            return Err(err);
        }
        Ok(value) => value,
    };

    // Set remote description
    if let Err(err) = peer.set_remote_description(offer.clone()).await {
        error!(error = %err, description = %offer, "failed to set remote description");

        peer.close().await?;
        return Err(err.into());
    }

    info!("configured server webrtc peer, waiting for ice gathering to complete");

    // Get ice gathering receiver
    let mut ice_complete = peer.gathering_complete_promise().await;

    // Complete negotiation
    let answer = peer.create_answer(None).await?;

    if let Err(err) = peer.set_local_description(answer.clone()).await {
        error!(error = %err, description = %answer, "failed to set local description");

        peer.close().await?;
        return Err(err.into());
    }

    // Wait for ice gathering to complete
    let _ = ice_complete.recv().await;

    // Use the local description with video and audio tracks, control channel and all ice candidates included
    let answer = peer
        .local_description()
        .await
        .expect("web_post: peer.local_description()");

    // Append additional data to the response
    let mut answer_sdp =
        Session::parse(answer.sdp.as_bytes()).expect("failed to get parse sdp answer");
    let additional_answer = WebRTCSessionAnswer {
        app_name: app_title,
        microphone: false,
    };
    additional_answer.apply(&mut answer_sdp);

    let mut answer = Vec::new();
    answer_sdp
        .write(&mut answer)
        .expect("failed to write sdp answer");
    let answer = String::from_utf8_lossy(&answer).to_string();

    info!("ice gathering completed, sending answer to client");

    spawn({
        let peer = peer.clone();

        async move {
            if let Err(err) = webrtc_loop(
                moonlight_stream,
                &peer,
                audio_channel,
                video_channel,
                control_channel,
            )
            .await
            {
                error!(error = %err, "webrtc main loop errored, closing stream");
            }

            info!("stopped main webrtc loop, cleaning up");

            if let Err(err) = peer.close().await {
                warn!(error = %err, "failed to close webrtc peer");
            }
        }
        .instrument(debug_span!("moonlight stream"))
    });

    // Add stream to the list of streams
    let (event_sender, mut event_receiver) = mpsc::channel(20);
    let stream = match Stream::new(&app, &user, event_sender).await {
        Ok(value) => value,
        Err(err) => {
            // TODO: cleanup the stream

            return Err(err);
        }
    };
    let stream_id = stream.id();
    debug!(stream_id = ?stream_id, "registered stream inside of the app");

    spawn({
        let peer = peer.clone();

        async move {
            loop {
                let event = select! {
                    _ = sleep(Duration::from_secs(10)) => {
                        // Check if the connection was closed
                        if matches!(peer.connection_state(), RTCPeerConnectionState::Closed) {
                            // Close this thread -> drops receiver -> the stream will be cleaned up on the app
                            return;
                        }

                        continue;
                    }
                    event = event_receiver.recv() => event
                };
                let Some(event) = event else {
                    // The channel was closed, shouldn't happen
                    warn!("the external event receiver was closed");
                    return;
                };

                match event {
                    ExternalStreamEvent::WebRTCAddIceCandidate { ice_sdp_frag } => {
                        for line in ice_sdp_frag.lines() {
                            #[allow(clippy::collapsible_if)]
                            if let Some(candidate) = line.strip_prefix("a=") {
                                if let Err(err) = peer.add_ice_candidate(RTCIceCandidateInit {
                                    candidate: candidate.to_string(),
                                    sdp_mid: None,
                                    sdp_mline_index: None,
                                    username_fragment: None,
                                })
                                .await {
                                    warn!(error = %err, candidate = ?candidate, "failed to add trickle ice candidate");
                                } else {
                                    debug!(candidate = ?candidate, "added remote ice candidate");
                                };
                            }
                        }
                    }
                    ExternalStreamEvent::Stop => {
                        info!("closing the stream");

                        if let Err(err) = peer.close().await {
                            warn!(error = %err, "error whilst closing the webrtc peer");
                        }
                    }
                }
            }
        }
        .instrument(debug_span!("external event handler", stream_id = ?stream_id))
    });

    debug!(answer = ?answer, "sending answer to client");

    let mut response = HttpResponse::Created();

    // Set location
    let path_prefix = &app.config().web_server.url_path_prefix;
    response.insert_header((
        "Location",
        format!("{path_prefix}/api/host/stream/webrtc/{}", stream_id.0),
    ));

    Ok(response.content_type("application/sdp").body(answer))
}

#[patch("/{stream_id}")]
pub async fn webrtc_patch(
    app: Data<App>,
    mut user: AuthenticatedUser,
    stream_id: Path<u32>,
    request: HttpRequest,
    body: String,
) -> Result<HttpResponse, AppError> {
    let stream_id = StreamId(stream_id.into_inner());

    let stream = app.stream_by_id(stream_id).await?;

    match request.headers().get(header::CONTENT_TYPE) {
        Some(x)
            if x.to_str()
                .map(|x| x.starts_with("application/trickle-ice-sdpfrag"))
                .unwrap_or(false) =>
        {
            // Don't support ice restarts
            // -> ICE restart requests use If-Match: *
            if let Some("*") = request
                .headers()
                .get(header::IF_MATCH)
                .and_then(|v| v.to_str().ok())
            {
                Ok(HttpResponse::UnprocessableEntity().finish())
            } else {
                stream
                    .send_event(
                        &mut user,
                        ExternalStreamEvent::WebRTCAddIceCandidate { ice_sdp_frag: body },
                    )
                    .await?;

                Ok(HttpResponse::NoContent().finish())
            }
        }
        _ => Ok(HttpResponse::UnsupportedMediaType().finish()),
    }
}

#[delete("/{stream_id}")]
#[instrument(skip(app, user), fields(user = %user.id()))]
pub async fn webrtc_delete(
    app: Data<App>,
    mut user: AuthenticatedUser,
    stream_id: Path<u32>,
) -> Result<HttpResponse, AppError> {
    let stream_id = StreamId(stream_id.into_inner());

    let stream = app.stream_by_id(stream_id).await?;

    stream
        .send_event(&mut user, ExternalStreamEvent::Stop)
        .await?;

    Ok(HttpResponse::Ok()
        .finish()
        .set_body(BoxBody::new("stream not found")))
}
