use std::sync::Arc;

use actix_web::{Error, HttpRequest, HttpResponse, get, rt::spawn, web::Payload};
use actix_ws::{Message, MessageStream, Session};
use async_trait::async_trait;
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
use tracing::{instrument, warn};

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

    let host_id = 0;
    let host_id = HostId(host_id);

    let mut host = user.host(host_id).await?;

    let host = host.use_host(&mut user).await?;

    if !host.is_paired().await.map_err(|err| AppError::from(err))? {
        return Err(AppError::HostNotPaired.into());
    }

    // upgrade connection to web socket connection
    let (res, mut ws_sender, ws_receiver) = actix_ws::handle(&req, body_stream)?;

    spawn(async move {
        // TODO
        // match handle_ws(query, &host, ws_sender, ws_receiver).await {
        //     Ok(_) => {}
        //     Err(err) => {
        //         // TODO
        //         todo!();
        //     }
        // }
    });

    Ok(res)
}

async fn handle_ws(
    width: u32,
    height: u32,
    fps: u32,
    bitrate_kbps: u32,
    hdr: bool,
    supported_codecs: VideoFormats,
    local_audio_play_mode: bool,
    audio_config: AudioConfig,
    app_id: u32,
    host: &MoonlightHost<RequestClient>,
    ws_sender: Session,
    mut ws_receiver: MessageStream,
) -> Result<(), AppError> {
    let control_config = create_control_packet_config();

    // create moonlight stream handler, will handle sending over the web socket
    let handler = Arc::new(WsStreamHandler {
        control_config: control_config.clone(),
        ws_sender,
    });

    // get settings
    let settings = MoonlightStreamSettings {
        width,
        height,
        fps,
        fps_x100: fps * 100,
        bitrate: bitrate_kbps,
        packet_size: 2048,
        encryption_flags: EncryptionFlags::AUDIO | EncryptionFlags::FOUNDATION_MICROPHONE,
        streaming_remotely: StreamingConfig::Auto,
        sops: true,
        hdr,
        supported_video_formats: supported_codecs,
        // TODO: color range?
        color_space: ColorSpace::Rec709,
        color_range: ColorRange::Limited,
        local_audio_play_mode,
        audio_config,
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
            app_id,
            &settings,
            aes_key,
            aes_iv,
            MoonlightStream::launch_query_parameters(),
        )
        .await?;

    let stream =
        MoonlightStream::connect(config, settings, Arc::new(RustCryptoBackend) as _, handler)
            .await?;

    // handle incoming ws messages
    while let Some(Ok(message)) = ws_receiver.recv().await {
        match message {
            Message::Binary(message) => {
                if message.len() < 1 {
                    continue;
                }

                // TODO: put the channel id into a const
                if message[0] == 0 {
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

    Ok(())
}

struct WsStreamHandler {
    control_config: ControlPacketConfig,
    ws_sender: Session,
}

#[async_trait]
impl MoonlightStreamHandler for WsStreamHandler {
    async fn setup_video(&self, setup: VideoSetup) -> Result<(), MoonlightStreamError> {
        todo!()
    }
    async fn on_video_frame(&self, frame: VideoDecodeUnit<&[u8]>) -> DecodeResult {
        todo!()
    }

    async fn setup_audio(
        &self,
        audio_config: AudioConfig,
        opus_config: OpusMultistreamConfig,
    ) -> Result<(), MoonlightStreamError> {
        todo!()
    }
    async fn on_audio_frame(&self, frame: AudioFrame<&[u8]>) {
        let mut buffer = vec![0; 1 + frame.buffer.len()];
        buffer[1..].copy_from_slice(frame.buffer);

        buffer[0] = 2;

        let _ = self.ws_sender.clone().binary(buffer).await;
    }

    async fn on_control_packet(&self, packet: ControlPacket) {
        let mut buffer = [0; ControlPacket::MAX_SIZE + 1];

        // TODO: put the channel id into a const
        buffer[0] = 0;

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
