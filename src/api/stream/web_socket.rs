use std::{sync::Arc, time::Duration};

use crate::api::bindings::{
    WebSocketChannel, WebSocketClientboundMessage, WebSocketServerboundMessage,
    WebSocketStreamResponse,
};
use actix_web::{Error, HttpRequest, HttpResponse, get, rt::spawn, web::Payload};
use actix_ws::{Message, MessageStream, Session};
use moonlight_common::{
    crypto::rustcrypto::RustCryptoBackend,
    stream::{
        AesIv, AesKey, EncryptionFlags, MoonlightStreamSettings, StreamingConfig,
        audio::AudioConfig,
        control::ActiveGamepads,
        proto::{
            MoonlightStreamSetup,
            control::packet::{ControlPacket, PacketDirection},
        },
        tokio::MoonlightStream,
        video::{ColorRange, ColorSpace, VideoCapabilities, VideoFormats},
    },
};
use tokio::{select, time::sleep};
use tracing::{Instrument, debug_span, error, info, instrument, warn};

use crate::{
    api::stream::create_control_packet_config,
    app::{AppError, host::HostId, user::AuthenticatedUser},
};

// TODO: on new major make this web socket a different path, e.g. "/host/stream/web_socket"

#[get("/host/stream")]
#[instrument(skip(user, body_stream), fields(user = %user.id()))]
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
    mut ws_sender: Session,
    mut ws_receiver: MessageStream,
) -> Result<(), AppError> {
    let control_config = create_control_packet_config();

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
        Err(err) => {
            error!(error = %err, "failed to deserialize json");
            return Err(AppError::StreamClosed);
        }
    };

    // TODO: apply role restrictions

    // -- Get host
    let host_id = HostId(stream_request.host_id);
    let mut host = user.host(host_id).await?;

    let host = host.use_host(&mut user).await?;

    if !host.is_paired().await.map_err(AppError::from)? {
        return Err(AppError::HostNotPaired);
    }

    // -- Start stream
    // get settings
    let settings = MoonlightStreamSettings {
        width: stream_request.width,
        height: stream_request.height,
        fps: stream_request.fps,
        fps_x100: stream_request.fps * 100,
        bitrate: stream_request.bitrate,
        packet_size: 2048,
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
    // TODO: apply permissions to settings

    // encryption
    let aes_key = AesKey::new_random(&RustCryptoBackend)?;
    let aes_iv = AesIv::new_random(&RustCryptoBackend)?;

    // start stream
    let config = host
        .start_stream(
            stream_request.app_id,
            &settings,
            aes_key,
            aes_iv,
            MoonlightStreamSetup::launch_query_parameters(),
        )
        .await?;

    let stream = MoonlightStream::connect(
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
    });
    let response = match serde_json::to_string(&response) {
        Ok(value) => value,
        Err(err) => {
            error!(error = %err, response = ?response, "failed to serialize response");

            stream.stop();
            return Err(AppError::StreamClosed);
        }
    };
    let _ = ws_sender.text(response).await;

    // forward audio frames
    spawn({
        let stream = stream.clone();
        let sender = ws_sender.clone();
        async move {
            while let Ok(frame) = stream.poll_audio_frame().await {
                let mut buffer = vec![0; 1 + frame.buffer.len()];
                buffer[1..].copy_from_slice(&frame.buffer);

                buffer[0] = WebSocketChannel::AUDIO;

                let _ = sender.clone().binary(buffer).await;
            }
        }
    });

    // forward video frames
    spawn({
        let stream = stream.clone();
        let sender = ws_sender.clone();
        async move {
            while let Ok(frame) = stream.poll_video_frame().await {
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

                let _ = sender.clone().binary(buffer).await;
            }
        }
    });

    // forward packets
    spawn({
        let stream = stream.clone();
        let sender = ws_sender.clone();
        let control_config = control_config.clone();
        async move {
            while let Ok(packet) = stream.poll_packet().await {
                let mut buffer = [0; ControlPacket::MAX_SIZE + 1];

                buffer[0] = WebSocketChannel::CONTROL;

                #[allow(clippy::unwrap_used)]
                let packet_len = packet
                    .serialize(&control_config, buffer[1..].as_mut_array().unwrap())
                    .unwrap();

                let message = &buffer[0..(1 + packet_len)];

                let _ = sender.clone().binary(message.to_vec()).await;
            }
        }
    });

    // look for stream stop
    spawn({
        let stream = stream.clone();
        let sender = ws_sender.clone();
        async move {
            loop {
                sleep(Duration::from_secs(1)).await;
                if stream.is_stopped() {
                    let _ = sender.close(None).await;
                    break;
                }
            }
        }
    });

    // handle incoming ws messages
    while let Some(Ok(message)) = ws_receiver.recv().await {
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
                warn!(message = ?text, "received text over web socket");
            }
            _ => {}
        }
    }

    // stop stream
    info!("stopping stream because the web socket disconnected");
    stream.stop();

    Ok(())
}
