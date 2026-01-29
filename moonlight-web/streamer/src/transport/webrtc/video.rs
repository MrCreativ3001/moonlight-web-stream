use std::{
    io::Cursor,
    ops::Range,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use bytes::{Bytes, BytesMut};
use common::{
    api_bindings::{LogMessageType, StreamServerMessage},
    ipc::StreamerIpcMessage,
};
use log::{debug, error, info, trace, warn};
use moonlight_common::stream::{
    bindings::{DecodeResult, FrameType, SupportedVideoFormats, VideoDecodeUnit, VideoFormat},
    video::VideoSetup,
};
use tokio::runtime::Handle;
use webrtc::{
    api::media_engine::{MIME_TYPE_AV1, MIME_TYPE_H264, MIME_TYPE_HEVC, MediaEngine},
    peer_connection::RTCPeerConnection,
    rtcp::payload_feedbacks::{
        picture_loss_indication::PictureLossIndication,
        receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate,
    },
    rtp::{
        codecs::{av1::Av1Payloader, h265::RTP_OUTBOUND_MTU},
        header::Header,
        packet::Packet,
        packetizer::Payloader,
    },
    rtp_transceiver::{
        RTCPFeedback,
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType},
    },
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::transport::{
    TransportEvent,
    webrtc::{
        WebRtcInner,
        sender::{SequencedTrackLocalStaticRTP, TrackLocalSender},
        video::{
            annexb::AnnexBSplitter,
            h264::{payloader::H264Payloader, reader::H264Reader},
            h265::{payloader::H265Payloader, reader::H265Reader},
        },
    },
};

mod annexb;
mod h264;
mod h265;

enum VideoCodec {
    H264 {
        nal_reader: H264Reader<Cursor<Vec<u8>>>,
        payloader: H264Payloader,
    },
    H265 {
        nal_reader: H265Reader<Cursor<Vec<u8>>>,
        payloader: H265Payloader,
    },
    Av1 {
        annex_b: AnnexBSplitter<Cursor<Vec<u8>>>,
        payloader: Av1Payloader,
    },
}

pub struct WebRtcVideo {
    supported_video_formats: SupportedVideoFormats,
    sender: TrackLocalSender<SequencedTrackLocalStaticRTP>,
    needs_idr: Arc<AtomicBool>,
    clock_rate: u32,
    codec: Option<VideoCodec>,
    samples: Vec<BytesMut>,
}

impl WebRtcVideo {
    pub fn new(runtime: Handle, peer: Weak<RTCPeerConnection>, frame_queue_size: usize) -> Self {
        Self {
            clock_rate: 0,
            needs_idr: Default::default(),
            sender: TrackLocalSender::new(runtime, peer, frame_queue_size),
            codec: None,
            supported_video_formats: SupportedVideoFormats::empty(),
            samples: Default::default(),
        }
    }

    pub async fn set_codecs(&mut self, supported_codecs: SupportedVideoFormats) {
        self.supported_video_formats = supported_codecs;
    }

    pub async fn setup(
        &mut self,
        inner: &Arc<WebRtcInner>,
        VideoSetup {
            format,
            width,
            height,
            redraw_rate,
            flags: _,
        }: VideoSetup,
    ) -> bool {
        info!("[Stream] Stream setup: {width}x{height}x{redraw_rate} and {format:?}");

        if !format.contained_in(self.supported_video_formats) {
            let message = format!(
                "The host tried to setup a video stream with a non supported video format: {format:?}, supported formats: {}",
                self.supported_video_formats
            );

            error!("{}", message);

            if let Err(err) = inner
                .event_sender
                .send(TransportEvent::SendIpc(StreamerIpcMessage::WebSocket(
                    StreamServerMessage::DebugLog {
                        message,
                        ty: Some(LogMessageType::FatalDescription),
                    },
                )))
                .await
            {
                warn!("Failed to send error to client: {err}");
            }

            return false;
        }

        let Some(codec) = video_format_to_codec(format) else {
            // This shouldn't happen
            error!("Failed to get video codec with format {:?}", format);
            return false;
        };

        let needs_idr = self.needs_idr.clone();
        if let Err(err) = self
            .sender
            .create_track(
                TrackLocalStaticRTP::new(
                    codec.capability.clone(),
                    "video".to_string(),
                    "moonlight".to_string(),
                )
                .into(),
                {
                    let needs_idr = needs_idr.clone();

                    move |packet| {
                        let packet = packet.as_any();

                        if packet.is::<PictureLossIndication>() {
                            needs_idr.store(true, Ordering::Release);
                        }
                        if let Some(_max_bitrate) =
                            packet.downcast_ref::<ReceiverEstimatedMaximumBitrate>()
                        {
                            // Moonlight doesn't support dynamic bitrate changing :(
                        }
                    }
                },
            )
            .await
        {
            let message = format!(
                "Failed to create video track with format {format:?} and codec \"{codec:?}\": {err:?}"
            );
            error!("{}", message);

            if let Err(err) = inner
                .event_sender
                .send(TransportEvent::SendIpc(StreamerIpcMessage::WebSocket(
                    StreamServerMessage::DebugLog {
                        message,
                        ty: Some(LogMessageType::FatalDescription),
                    },
                )))
                .await
            {
                warn!("Failed to send error to client: {err}");
            }
            return false;
        }

        self.clock_rate = codec.capability.clock_rate;

        self.codec = match format {
            // -- H264
            VideoFormat::H264 | VideoFormat::H264High8_444 => Some(VideoCodec::H264 {
                nal_reader: H264Reader::new(Cursor::new(Vec::new()), 0),
                payloader: Default::default(),
            }),
            // -- H265
            VideoFormat::H265
            | VideoFormat::H265Main10
            | VideoFormat::H265Rext8_444
            | VideoFormat::H265Rext10_444 => Some(VideoCodec::H265 {
                nal_reader: H265Reader::new(Cursor::new(Vec::new()), 0),
                payloader: Default::default(),
            }),
            // -- AV1
            VideoFormat::Av1Main8
            | VideoFormat::Av1Main10
            | VideoFormat::Av1High8_444
            | VideoFormat::Av1High10_444 => Some(VideoCodec::Av1 {
                annex_b: AnnexBSplitter::new(Cursor::new(Vec::new()), 0),
                payloader: Default::default(),
            }),
        };

        // Renegotiate
        if !inner.send_offer().await {
            warn!("Failed to renegotiate. Video was added!");
        }

        true
    }

    pub async fn send_decode_unit(&mut self, unit: &VideoDecodeUnit<'_>) -> DecodeResult {
        trace!("Starting frame");

        let timestamp = unit.rtp_timestamp;

        let mut full_frame = Vec::new();
        for buffer in unit.buffers {
            full_frame.extend_from_slice(buffer.data);
        }

        let important = matches!(unit.frame_type, FrameType::Idr);

        match &mut self.codec {
            // -- H264
            Some(VideoCodec::H264 {
                nal_reader,
                payloader,
            }) => {
                nal_reader.reset(Cursor::new(full_frame));

                while let Ok(Some(nal)) = nal_reader.next_nal() {
                    trace!(
                        "H264, Start Code: {:?}, NAL: {:?}, Bytes: {:02X?}",
                        nal.start_code, nal.header, &nal.full,
                    );

                    if nal.header.nal_unit_type == h264::NalUnitType::FillerData {
                        trace!("Ignoring nal because it's filler data: {:?}", nal.header);
                        continue;
                    }

                    let data = trim_bytes_to_range(
                        nal.full,
                        nal.header_range.start..nal.payload_range.end,
                    );

                    self.samples.push(data);
                }

                send_single_frame(
                    &mut self.samples,
                    &mut self.sender,
                    payloader,
                    timestamp,
                    important,
                    &self.needs_idr,
                )
                .await;
            }
            // -- H265
            Some(VideoCodec::H265 {
                nal_reader,
                payloader,
            }) => {
                nal_reader.reset(Cursor::new(full_frame));

                while let Ok(Some(nal)) = nal_reader.next_nal() {
                    trace!(
                        "H265, Start Code: {:?}, NAL: {:?}, Bytes: {:02X?}",
                        nal.start_code, nal.header, &nal.full
                    );

                    let data = trim_bytes_to_range(
                        nal.full,
                        nal.header_range.start..nal.payload_range.end,
                    );

                    self.samples.push(data);
                }

                send_single_frame(
                    &mut self.samples,
                    &mut self.sender,
                    payloader,
                    timestamp,
                    important,
                    &self.needs_idr,
                )
                .await;
            }
            // -- AV1
            Some(VideoCodec::Av1 { annex_b, payloader }) => {
                annex_b.reset(Cursor::new(full_frame));

                while let Ok(Some(annex_b_payload)) = annex_b.next() {
                    let data =
                        trim_bytes_to_range(annex_b_payload.full, annex_b_payload.payload_range);

                    self.samples.push(data);
                }

                send_single_frame(
                    &mut self.samples,
                    &mut self.sender,
                    payloader,
                    timestamp,
                    important,
                    &self.needs_idr,
                )
                .await;
            }
            None => {
                warn!("Failed to send decode unit because of missing codec!");
            }
        }

        trace!("Ending frame frame");

        if self
            .needs_idr
            .compare_exchange_weak(true, false, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
        {
            return DecodeResult::NeedIdr;
        }

        DecodeResult::Ok
    }
}

pub fn register_video_codecs(media_engine: &mut MediaEngine) -> Result<(), webrtc::Error> {
    for format in VideoFormat::all() {
        let Some(codec) = video_format_to_codec(format) else {
            continue;
        };
        debug!(
            "Registering Video Format {format:?}, Codec: {:?}",
            codec.capability
        );

        media_engine.register_codec(codec, RTPCodecType::Video)?;
    }

    Ok(())
}

async fn send_single_frame(
    samples: &mut Vec<BytesMut>,
    sender: &mut TrackLocalSender<SequencedTrackLocalStaticRTP>,
    payloader: &mut impl Payloader,
    timestamp: u32,
    important: bool,
    needs_idr: &AtomicBool,
) {
    if important {
        sender.clear_queue(false).await;
    }

    let mut peekable = samples.drain(..).peekable();

    let mut frame_samples = Vec::new();
    while let Some(sample) = peekable.next() {
        let packets = match packetize(
            payloader,
            RTP_OUTBOUND_MTU,
            0, // is set in the write fn
            timestamp,
            &sample.freeze(),
            peekable.peek().is_none(),
        ) {
            Ok(value) => value,
            Err(err) => {
                warn!("failed to packetize packet: {err}");
                continue;
            }
        };

        frame_samples.extend(packets);
    }

    if !sender.send_samples(frame_samples, important).await {
        sender.clear_queue(true).await;

        // We've dropped a frame (likely due to buffering)
        needs_idr.store(true, Ordering::Release);
    }
}

fn packetize(
    payloader: &mut impl Payloader,
    mtu: usize,
    sequence_number: u16,
    timestamp: u32,
    payload: &Bytes,
    end_has_marker: bool,
) -> Result<Vec<Packet>, anyhow::Error> {
    let payloads = payloader.payload(mtu - 12, payload)?;
    let payloads_len = payloads.len();
    let mut packets = Vec::with_capacity(payloads_len);
    for (i, payload) in payloads.into_iter().enumerate() {
        packets.push(Packet {
            header: Header {
                version: 2,
                padding: false,
                extension: false,
                marker: end_has_marker && i == payloads_len - 1,
                sequence_number,
                timestamp,
                payload_type: 0, // Value is handled when writing
                ssrc: 0,         // Value is handled when writing
                ..Default::default()
            },
            payload,
        });
    }

    Ok(packets)
}

fn video_format_to_codec(format: VideoFormat) -> Option<RTCRtpCodecParameters> {
    let rtcp_feedback = vec![
        RTCPFeedback {
            typ: "nack".to_string(),
            parameter: "".to_string(),
        },
        RTCPFeedback {
            typ: "nack".to_string(),
            parameter: "pli".to_string(),
        },
        RTCPFeedback {
            typ: "goog-remb".to_string(),
            parameter: "".to_string(),
        },
    ];

    match format {
        // -- H264 Constrained Baseline Profile
        VideoFormat::H264 => Some(RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                        .to_owned(),
                rtcp_feedback: rtcp_feedback.clone(),
            },
            payload_type: 96,
            ..Default::default()
        }),
        // -- H264 High Profile
        VideoFormat::H264High8_444 => Some(RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640032"
                        .to_owned(),
                rtcp_feedback: rtcp_feedback.clone(),
            },
            payload_type: 97,
            ..Default::default()
        }),

        // -- H265 Main Profile
        VideoFormat::H265 => Some(RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_HEVC.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: rtcp_feedback.clone(),
            },
            payload_type: 98,
            ..Default::default()
        }),
        // -- H265 Main10 Profile
        VideoFormat::H265Main10 => Some(RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_HEVC.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "profile-id=2;tier-flag=0;level-id=93;tx-mode=SRST".to_owned(),
                rtcp_feedback: rtcp_feedback.clone(),
            },
            payload_type: 99,
            ..Default::default()
        }),
        // -- H265 RExt 4:4:4 8-bit
        VideoFormat::H265Rext8_444 => Some(RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_HEVC.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "profile-id=4;tier-flag=0;level-id=120;tx-mode=SRST".to_owned(),
                rtcp_feedback: rtcp_feedback.clone(),
            },
            payload_type: 100,
            ..Default::default()
        }),
        // -- H265 RExt 4:4:4 10-bit
        VideoFormat::H265Rext10_444 => Some(RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_HEVC.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "profile-id=5;tier-flag=0;level-id=93;tx-mode=SRST".to_owned(),
                rtcp_feedback: rtcp_feedback.clone(),
            },
            payload_type: 101,
            ..Default::default()
        }),

        // -- Av1
        VideoFormat::Av1Main8 | VideoFormat::Av1Main10 => Some(RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_AV1.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "profile=0".to_owned(),
                rtcp_feedback: rtcp_feedback.clone(),
            },
            payload_type: 102,
            ..Default::default()
        }),
        VideoFormat::Av1High8_444 | VideoFormat::Av1High10_444 => Some(RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_AV1.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "profile=1".to_owned(),
                rtcp_feedback: rtcp_feedback.clone(),
            },
            payload_type: 103,
            ..Default::default()
        }),
    }
}

fn trim_bytes_to_range(mut buf: BytesMut, range: Range<usize>) -> BytesMut {
    if range.start > 0 {
        let _ = buf.split_to(range.start);
    }

    if range.end - range.start < buf.len() {
        let _ = buf.split_off(range.end - range.start);
    }

    buf
}
