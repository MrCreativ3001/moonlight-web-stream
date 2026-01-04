use async_trait::async_trait;
use bytes::Bytes;
use common::{
    StreamSettings,
    api_bindings::{StreamClientMessage, TransportChannelId},
    ipc::{ServerIpcMessage, StreamerIpcMessage},
};
use log::{trace, warn};
use moonlight_common::stream::{
    bindings::{
        AudioConfig, DecodeResult, FrameType, OpusMultistreamConfig, SupportedVideoFormats,
        VideoDecodeUnit,
    },
    video::VideoSetup,
};
use tokio::sync::mpsc::{Receiver, Sender, channel};

use crate::{
    buffer::ByteBuffer,
    transport::{
        InboundPacket, OutboundPacket, TransportChannel, TransportError, TransportEvent,
        TransportEvents, TransportSender,
    },
};

pub async fn new() -> Result<(WebSocketTransportSender, WebSocketTransportEvents), anyhow::Error> {
    let (event_sender, event_receiver) = channel::<TransportEvent>(20);

    // TODO: use the video_frame_queue_size with packet rtt info to estimate latency of pictures and request idr if too big

    Ok((
        WebSocketTransportSender { event_sender },
        WebSocketTransportEvents { event_receiver },
    ))
}

pub struct WebSocketTransportEvents {
    event_receiver: Receiver<TransportEvent>,
}

#[async_trait]
impl TransportEvents for WebSocketTransportEvents {
    async fn poll_event(&mut self) -> Result<TransportEvent, TransportError> {
        trace!("Polling WebSocketEvents");
        self.event_receiver
            .recv()
            .await
            .ok_or(TransportError::Closed)
    }
}

pub struct WebSocketTransportSender {
    event_sender: Sender<TransportEvent>,
}

#[async_trait]
impl TransportSender for WebSocketTransportSender {
    async fn setup_video(&self, _setup: VideoSetup) -> i32 {
        // empty
        0
    }
    async fn send_video_unit<'a>(
        &'a self,
        unit: &'a VideoDecodeUnit<'a>,
    ) -> Result<DecodeResult, TransportError> {
        let mut new_buffer = vec![0; 5];

        let mut byte_buffer = ByteBuffer::new(new_buffer.as_mut_slice());
        byte_buffer.put_u8(TransportChannelId::HOST_VIDEO);
        byte_buffer.put_u8(match unit.frame_type {
            FrameType::Idr => 1,
            FrameType::PFrame => 0,
        });
        byte_buffer.put_u32(unit.presentation_time.as_micros() as u32);

        for buffer in unit.buffers {
            new_buffer.extend_from_slice(buffer.data);
        }
        // TODO: ignore h264/h265 fillerdata?
        self.event_sender
            .send(TransportEvent::SendIpc(
                StreamerIpcMessage::WebSocketTransport(Bytes::from(new_buffer)),
            ))
            .await
            .unwrap();

        Ok(DecodeResult::Ok)
    }

    async fn setup_audio(
        &self,
        _audio_config: AudioConfig,
        _stream_config: OpusMultistreamConfig,
    ) -> i32 {
        // empty
        0
    }
    async fn send_audio_sample(&self, data: &[u8]) -> Result<(), TransportError> {
        let mut new_buffer = vec![0];

        let mut byte_buffer = ByteBuffer::new(new_buffer.as_mut_slice());
        byte_buffer.put_u8(TransportChannelId::HOST_AUDIO);

        new_buffer.extend_from_slice(data);

        self.event_sender
            .send(TransportEvent::SendIpc(
                StreamerIpcMessage::WebSocketTransport(Bytes::from(new_buffer)),
            ))
            .await
            .unwrap();

        Ok(())
    }

    async fn send(&self, packet: OutboundPacket) -> Result<(), TransportError> {
        let mut new_buffer = Vec::new();

        let (id, mut range) = packet.serialize(&mut new_buffer).unwrap();

        if range.start == 0 {
            new_buffer.resize(range.end - range.start + 1, 0);
            new_buffer.copy_within(range.clone(), range.start + 1);
            range.start += 1;
        }
        new_buffer[range.start - 1] = id.0;

        self.event_sender
            .send(TransportEvent::SendIpc(
                StreamerIpcMessage::WebSocketTransport(Bytes::from(new_buffer)),
            ))
            .await
            .unwrap();

        Ok(())
    }

    async fn on_ipc_message(&self, message: ServerIpcMessage) -> Result<(), TransportError> {
        match message {
            ServerIpcMessage::WebSocketTransport(message) => {
                if message.is_empty() {
                    warn!("Empty packet received!");
                    return Ok(());
                }

                let channel_id = message[0];

                let Some(packet) =
                    InboundPacket::deserialize(TransportChannel(channel_id), &message[1..])
                else {
                    warn!("Failed to receive packet on channel {channel_id}");
                    return Ok(());
                };

                self.event_sender
                    .send(TransportEvent::RecvPacket(packet))
                    .await
                    .unwrap();
            }
            ServerIpcMessage::WebSocket(StreamClientMessage::StartStream {
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
            }) => {
                let video_supported_formats = SupportedVideoFormats::from_bits(video_supported_formats).unwrap_or_else(|| {
                    warn!("Failed to deserialize SupportedVideoFormats: {video_supported_formats}, falling back to only H264");
                    SupportedVideoFormats::H264
                });

                self.event_sender
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
                    .unwrap();
            }
            _ => {}
        }
        Ok(())
    }

    async fn close(&self) -> Result<(), TransportError> {
        // emtpy
        Ok(())
    }
}
