use std::{pin::pin, sync::Arc, time::Duration};

use crate::api::{
    bindings::{
        StreamStatsClientboundMessage, StreamStatsServerboundMessage, WebSocketChannel,
        WebSocketClientboundMessage, WebSocketServerboundMessage, WebSocketStreamResponse,
    },
    stream::{PACKET_SIZE, apply_role_restrictions},
};
use actix_web::{Error, HttpRequest, HttpResponse, get, rt::spawn, web::Payload};
use actix_ws::{Message, MessageStream, Session};
use moonlight_common::{
    AppId,
    crypto::rustcrypto::RustCryptoBackend,
    stream::{
        AesIv, AesKey, EncryptionFlags, MoonlightStreamSettings, StreamingConfig,
        audio::AudioConfig,
        control::ActiveGamepads,
        proto::{
            MoonlightStreamSetup,
            audio::AudioStreamEvent,
            control::{
                ControlStreamEvent,
                packet::{ControlPacket, ControlPacketConfig, PacketDirection},
            },
            video::VideoStreamEvent,
        },
        tokio::{MoonlightStream, MoonlightStreamEvent},
        video::{ColorRange, ColorSpace, VideoCapabilities, VideoFormats},
    },
};
use tokio::{
    select,
    time::{interval, sleep, timeout},
};
use tracing::{Instrument, debug, debug_span, error, info, instrument, trace, warn};

use crate::{
    api::stream::create_control_packet_config,
    app::{AppError, host::HostId, user::AuthenticatedUser},
};

mod send_queue;
use send_queue::{
    MAX_PENDING_BYTES, MAX_PENDING_MESSAGES, MAX_SEND_AGE, Payload as QueuePayload, SendQueue,
    SenderTask,
};

// Keep driving only long enough to deliver this viewer's graceful disconnect.
// This does not quit the running app or stop the host service.
const DISCONNECT_GRACE: Duration = Duration::from_secs(2);

enum WsData {
    Bytes(Vec<u8>),
    Text(String),
}

impl QueuePayload for WsData {
    fn retained_bytes(&self) -> usize {
        match self {
            // Charge spare allocation capacity too, including truncated control
            // buffers. Convert to Bytes only when handing off to Actix.
            Self::Bytes(bytes) => bytes.capacity(),
            Self::Text(text) => text.capacity(),
        }
    }
}

#[get("/host/stream/web_socket")]
#[instrument(skip(user, req, body_stream), fields(user = %user.id()))]
pub async fn web_socket_stream(
    mut user: AuthenticatedUser,
    req: HttpRequest,
    body_stream: Payload,
) -> Result<HttpResponse, Error> {
    if !user
        .role()
        .await?
        .permissions()
        .await?
        .allow_transport_websockets
    {
        return Err(AppError::Forbidden.into());
    }

    // upgrade connection to web socket connection
    let (res, ws_sender, ws_receiver) = actix_ws::handle(&req, body_stream)?;

    spawn(
        async move {
            match handle_ws(user, ws_sender, ws_receiver).await {
                Ok(_) => {}
                Err(err) => {
                    error!(error = %err, "stream failed");
                }
            }
        }
        .instrument(debug_span!("ws handler")),
    );

    Ok(res)
}

async fn handle_ws(
    mut user: AuthenticatedUser,
    ws_sender: Session,
    mut ws_receiver: MessageStream,
) -> Result<(), AppError> {
    let control_config = create_control_packet_config();

    // See if the user is allowed to use web sockets
    let permissions = user.role().await?.permissions().await?;
    if !permissions.allow_transport_websockets {
        return Err(AppError::Forbidden);
    }

    // Wait for stream request
    let stream_request = select! {
        _ = sleep(Duration::from_secs(10)) => {
            return Err(AppError::StreamClosed);
        }
        request = ws_receiver.recv() => request
    };

    // Deserialize message
    let stream_request = match stream_request.expect("stream request") {
        Ok(Message::Text(text)) => text,
        Ok(message) => {
            error!(message = ?message, "web socket received unexpected start message");
            return Err(AppError::StreamClosed);
        }
        Err(err) => {
            error!(error = %err, "web socket protocol error");
            return Err(AppError::StreamClosed);
        }
    };
    let stream_request = match serde_json::from_str::<WebSocketServerboundMessage>(&stream_request)
    {
        Ok(WebSocketServerboundMessage::Request(request)) => request,
        Ok(message) => {
            error!(message = ?message, "expected web socket stream request but got another message");
            return Err(AppError::StreamClosed);
        }
        Err(err) => {
            error!(error = %err, "failed to deserialize json");
            return Err(AppError::StreamClosed);
        }
    };

    // -- Get host
    let host_id = HostId(stream_request.host_id);
    let mut host = user.host(host_id).await?;

    let host = host.use_host(&mut user).await?;

    if !host.is_paired().await.map_err(AppError::from)? {
        return Err(AppError::HostNotPaired);
    }

    // -- Get Apps
    let app_id = AppId(stream_request.app_id);
    let apps = host.app_list().await?;
    let app_title = apps
        .into_iter()
        .find(|app| app.id == app_id)
        .map(|app| app.title);

    // -- Start stream
    // get settings
    let mut settings = MoonlightStreamSettings {
        width: stream_request.width,
        height: stream_request.height,
        fps: stream_request.fps,
        fps_x100: stream_request.fps * 100,
        bitrate: stream_request.bitrate,
        packet_size: PACKET_SIZE,
        encryption_flags: EncryptionFlags::AUDIO | EncryptionFlags::FOUNDATION_MICROPHONE,
        streaming_remotely: StreamingConfig::Auto,
        sops: true,
        hdr: stream_request.hdr,
        supported_video_formats: VideoFormats::from_bits_retain(stream_request.supported_codecs),
        // TODO: color range?
        color_space: ColorSpace::Rec709,
        color_range: ColorRange::Limited,
        local_audio_play_mode: stream_request.local_audio_play_mode,
        audio_config: AudioConfig::STEREO,
        gamepads_attached: ActiveGamepads::empty(),
        gamepads_persist_after_disconnect: false,
        // TODO: mic?
        enable_mic: false,
    };

    // apply permissions
    apply_role_restrictions(&permissions, &mut settings);

    // adjust settings
    let server_version = host.version().await?;
    let gfe_version = host.gfe_version().await?;
    let server_codec_mode_support = host.server_codec_mode_support().await?;
    settings.adjust_for_server(server_version, &gfe_version, server_codec_mode_support)?;

    // encryption
    let aes_key = AesKey::new_random(&RustCryptoBackend)?;
    let aes_iv = AesIv::new_random(&RustCryptoBackend)?;

    info!(settings = ?settings, "starting stream");

    // start stream
    let config = host
        .start_stream(
            app_id,
            &settings,
            aes_key,
            aes_iv,
            MoonlightStreamSetup::launch_query_parameters(),
        )
        .await?;

    let mut stream = MoonlightStream::connect(
        config,
        settings,
        Arc::new(RustCryptoBackend),
        VideoCapabilities::default(),
    )
    .await?;

    // send stream start response
    let audio_setup = stream.audio_setup();
    let video_setup = stream.video_setup();

    let response = WebSocketClientboundMessage::Response(WebSocketStreamResponse {
        video_codec: video_setup.format as u32,
        audio_sample_rate: audio_setup.sample_rate,
        audio_channel_count: audio_setup.channel_count,
        audio_streams: audio_setup.streams,
        audio_coupled_streams: audio_setup.coupled_streams,
        audio_samples_per_frame: audio_setup.samples_per_frame,
        audio_mapping: audio_setup.mapping,
        app_name: app_title,
    });
    info!(response = ?response, "sending response to client");

    let (ws_channel_sender, ws_channel_receiver) =
        send_queue::channel(MAX_PENDING_BYTES, MAX_PENDING_MESSAGES, MAX_SEND_AGE);
    let send_session = ws_sender.clone();
    let mut sender_task = SenderTask::new(spawn(
        ws_channel_receiver
            .run(move |data| {
                let mut session = send_session.clone();
                async move {
                    match data {
                        WsData::Bytes(bytes) => session.binary(bytes).await,
                        WsData::Text(text) => session.text(text).await,
                    }
                }
            })
            .instrument(debug_span!("ws_sender")),
    ));

    // Always run cleanup, including initial-response enqueue failures and errors
    // from stream.drive(). The sender is supervised, never detached on failure.
    let result = match send_ws_message(&ws_channel_sender, response) {
        Ok(()) => {
            ws_loop(
                &ws_channel_sender,
                ws_receiver,
                &mut stream,
                &mut sender_task,
                control_config,
            )
            .await
        }
        Err(err) => Err(err),
    };
    if let Err(err) = result {
        error!(error = %err, "web socket main loop errored, closing stream");
    }

    sender_task.stop().await;
    drop(ws_channel_sender);
    if stream.is_alive() {
        if let Err(err) = stream.disconnect() {
            warn!(error = %err, "failed to disconnect web socket viewer stream");
        }
    }
    // The close frame may itself encounter backpressure. Do not let it delay
    // stream cleanup indefinitely or drain stale media after the viewer failed.
    let (_, disconnected) = tokio::join!(
        timeout(DISCONNECT_GRACE, ws_sender.close(None)),
        timeout(DISCONNECT_GRACE, async {
            while stream.is_alive() {
                if stream.drive().await.is_err() {
                    break;
                }
            }
        }),
    );
    if disconnected.is_err() {
        debug!("web socket viewer disconnect grace expired");
    }

    Ok(())
}

async fn ws_loop(
    ws_sender: &SendQueue<WsData>,
    ws_receiver: MessageStream,
    stream: &mut MoonlightStream,
    sender_task: &mut SenderTask,
    control_config: ControlPacketConfig,
) -> Result<(), AppError> {
    select! {
        biased;
        // Observe sender failure first without biasing capture against controls.
        result = sender_task.wait() => {
            warn!(result = ?result, "web socket sender stopped, disconnecting viewer");
            Err(AppError::StreamClosed)
        }
        result = ws_relay_loop(ws_sender, ws_receiver, stream, control_config) => result,
    }
}

async fn ws_relay_loop(
    ws_sender: &SendQueue<WsData>,
    mut ws_receiver: MessageStream,
    stream: &mut MoonlightStream,
    control_config: ControlPacketConfig,
) -> Result<(), AppError> {
    let mut relay_stats_ticker = pin!(interval(Duration::from_secs(1)));

    loop {
        if !stream.is_alive() {
            break;
        }

        select! {
            // drive the moonlight stream forward
            result = stream.drive() => {
                let event = result?;

                match event {
                    MoonlightStreamEvent::Audio(AudioStreamEvent::OnFrame(frame)) => {
                        let mut buffer = vec![0; 1 + frame.buffer.len()];
                        buffer[1..].copy_from_slice(&frame.buffer);

                        buffer[0] = WebSocketChannel::AUDIO;

                        enqueue_ws(ws_sender, WsData::Bytes(buffer))?;
                    }
                    MoonlightStreamEvent::Video(VideoStreamEvent::SignalIdr) => {
                        if let Err(err)=  stream.send_raw(ControlPacket::RequestIdr) {
                            warn!(error = %err, "failed to request idr after the moonlight video stream requested an idr");
                        }
                    }
                    MoonlightStreamEvent::Video(VideoStreamEvent::OnFrame(frame)) => {
                        // TODO: avoid using payloading and depayloading the frame like this
                        let mut buffer = vec![0; 1 + 5 + frame.raw().len()];
                        buffer[(1 + 5)..].copy_from_slice(frame.raw());

                        buffer[0] = WebSocketChannel::VIDEO;
                        // TODO: make frame type from video packet public, 2==Idr
                        buffer[1] = if frame.metadata().frame_type.serialize() == 2 {
                            1
                        } else {
                            0
                        };
                        buffer[2..6].copy_from_slice(
                            &(frame.metadata().timestamp.as_micros() as u32).to_be_bytes(),
                        );

                        enqueue_ws(ws_sender, WsData::Bytes(buffer))?;
                    }
                    MoonlightStreamEvent::Control(ControlStreamEvent::Packet(packet)) => {
                        let mut buffer = vec![0; ControlPacket::MAX_SIZE + 1];

                        buffer[0] = WebSocketChannel::CONTROL;

                        #[allow(clippy::unwrap_used)]
                        let packet_len = packet
                            .serialize(&control_config, buffer[1..].as_mut_array().unwrap())
                            .unwrap();

                        buffer.truncate(1 + packet_len);
                        enqueue_ws(ws_sender, WsData::Bytes(buffer))?;
                    }
                    _ => {}
                }
            }
            // relay stats
            _ = relay_stats_ticker.tick() => {
                let rtt = match stream.estimated_rtt() {
                    Ok(value) => value,
                    Err(err) => {
                        warn!(error = %err, "failed to send rtt to client");
                        break;
                    }
                };

                send_ws_message(
                    ws_sender,
                    WebSocketClientboundMessage::Stats(StreamStatsClientboundMessage::RelayRtt {
                        rtt_ms: rtt.rtt.as_millis() as u32,
                        rtt_variance_ms: rtt.rtt_variance.as_millis() as u32,
                    })
                )?;
            }
            // Handle incoming ws requests
            result = ws_receiver.recv() => {
                let Some(Ok(message)) = result else {
                    return Ok(());
                };

                match message {
                    Message::Binary(message) => {
                        if message.is_empty() {
                            continue;
                        }

                        if message[0] == WebSocketChannel::CONTROL {
                            let Some(packet) = ControlPacket::deserialize(
                                PacketDirection::ServerBound,
                                &control_config,
                                &message[1..],
                            ) else {
                                warn!(message = ?message, "received unknown control packet");
                                continue;
                            };

                            if let Err(err) = stream.send_raw(packet) {
                                warn!(error = %err, "failed to send control packet");
                            }
                        }
                    }
                    Message::Text(text) => {
                        let message = match serde_json::from_str::<WebSocketServerboundMessage>(&text) {
                            Ok(value) => value,
                            Err(err) => {
                                warn!(error = %err, "failed to deserialize serverbound web socket message");
                                continue;
                            }
                        };

                        if let WebSocketServerboundMessage::Stats(StreamStatsServerboundMessage::Ping(id)) =
                            message
                        {
                            send_ws_message(ws_sender, WebSocketClientboundMessage::Stats(StreamStatsClientboundMessage::Pong(id)))?;
                        }
                    }
                    Message::Close(_) => {
                        info!("client closed the web socket, disconnecting viewer stream");
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn send_ws_message(
    sender: &SendQueue<WsData>,
    message: WebSocketClientboundMessage,
) -> Result<(), AppError> {
    trace!(message = ?message, "sending text message to client");

    let text = match serde_json::to_string(&message) {
        Ok(value) => value,
        Err(err) => {
            warn!(error = %err, "failed to send web socket message");
            return Err(AppError::StreamClosed);
        }
    };

    enqueue_ws(sender, WsData::Text(text))
}

fn enqueue_ws(sender: &SendQueue<WsData>, data: WsData) -> Result<(), AppError> {
    sender.try_send(data).map_err(|err| {
        warn!(error = %err, "web socket queue failed, disconnecting viewer");
        AppError::StreamClosed
    })
}
