use std::{sync::Arc, time::Duration};

use actix_web::{Error, HttpRequest, HttpResponse, get, rt::spawn, web::Payload};
use actix_ws::{Message, MessageStream, Session};
use async_trait::async_trait;
use common::api_bindings::{
    WebSocketChannel, WebSocketClientboundMessage, WebSocketServerboundMessage,
    WebSocketStreamResponse,
};
use moonlight_common::{
    ServerVersion,
    crypto::rustcrypto::RustCryptoBackend,
    high::tokio::MoonlightHost,
    http::Request,
    stream::{
        AesIv, AesKey, EncryptionFlags, MoonlightStreamSettings, StreamingConfig,
        audio::{AudioConfig, AudioFrame, OpusMultistreamConfig},
        control::ActiveGamepads,
        proto::control::packet::{ControlPacket, ControlPacketConfig, PacketDirection},
        tokio::{MoonlightStream, MoonlightStreamError, MoonlightStreamHandler},
        video::{ColorRange, ColorSpace, DecodeResult, VideoDecodeUnit, VideoFormats, VideoSetup},
    },
};
use tokio::{select, sync::Mutex, time::sleep};
use tracing::{Instrument, debug_span, error, info, instrument, warn};

use crate::{
    api::stream::create_control_packet_config,
    app::{
        AppError, RequestClient,
        host::{AppId, HostId},
        user::AuthenticatedUser,
    },
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

    // create moonlight stream handler, will handle sending over the web socket
    let handler = Arc::new(WsStreamHandler {
        control_config: control_config.clone(),
        ws_sender: ws_sender.clone(),
        setup: Default::default(),
    });

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
        Ok(value) => {
            error!(message = ?value, "web socket received unexpected start message");
            return Err(AppError::StreamClosed);
        }
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
        return Err(AppError::HostNotPaired.into());
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
            MoonlightStream::launch_query_parameters(),
        )
        .await?;

    let stream = MoonlightStream::connect(
        config,
        settings,
        Arc::new(RustCryptoBackend) as _,
        handler.clone(),
    )
    .await?;

    // send stream start response
    let (video_setup, audio_setup) = {
        let mut setup = handler.setup.lock().await;
        let audio = setup.audio.take().expect("audio setup");
        let video = setup.video.expect("video setup");

        (video, audio)
    };

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
            stream.stop().await;
            return Err(AppError::StreamClosed);
        }
    };
    let _ = ws_sender.text(response).await;

    // handle incoming ws messages
    while let Some(Ok(message)) = ws_receiver.recv().await {
        match message {
            Message::Binary(message) => {
                if message.len() < 1 {
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

                    if let Err(err) = stream.send_input_raw(packet).await {
                        warn!(error = %err, "failed to send control packet");
                    }
                }
            }
            _ => {}
        }
    }

    // stop stream
    info!("stopping stream because the web socket disconnected");
    stream.stop().await;

    Ok(())
}

struct WsStreamHandler {
    control_config: ControlPacketConfig,
    ws_sender: Session,
    setup: Mutex<StreamSetup>,
}

#[derive(Debug, Default)]
struct StreamSetup {
    video: Option<VideoSetup>,
    audio: Option<OpusMultistreamConfig>,
}

#[async_trait]
impl MoonlightStreamHandler for WsStreamHandler {
    async fn setup_video(&self, setup: VideoSetup) -> Result<(), MoonlightStreamError> {
        let mut stream_setup = self.setup.lock().await;

        stream_setup.video = Some(setup);

        Ok(())
    }
    async fn on_video_frame(&self, frame: VideoDecodeUnit<&[u8]>) -> DecodeResult {
        // TODO
        DecodeResult::Ok
    }

    async fn setup_audio(
        &self,
        _audio_config: AudioConfig,
        opus_config: OpusMultistreamConfig,
    ) -> Result<(), MoonlightStreamError> {
        let mut stream_setup = self.setup.lock().await;

        stream_setup.audio = Some(opus_config);

        Ok(())
    }
    async fn on_audio_frame(&self, frame: AudioFrame<&[u8]>) {
        let mut buffer = vec![0; 1 + frame.buffer.len()];
        buffer[1..].copy_from_slice(frame.buffer);

        buffer[0] = WebSocketChannel::AUDIO;

        let _ = self.ws_sender.clone().binary(buffer).await;
    }

    async fn on_control_packet(&self, packet: ControlPacket) {
        let mut buffer = [0; ControlPacket::MAX_SIZE + 1];

        buffer[0] = WebSocketChannel::CONTROL;

        #[allow(clippy::unwrap_used)]
        let packet_len = packet
            .serialize(&self.control_config, buffer[1..].as_mut_array().unwrap())
            .unwrap();

        let message = &buffer[0..(1 + packet_len)];

        let _ = self.ws_sender.clone().binary(message.to_vec()).await;
    }

    async fn on_stop(&self) {
        let _ = self.ws_sender.clone().close(None);
    }
}
