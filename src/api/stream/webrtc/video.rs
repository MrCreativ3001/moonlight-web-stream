use std::{collections::HashMap, mem::swap, sync::Arc};

use bytes::Bytes;
use moonlight_common::{
    stream::{
        proto::video::frame::OwnedVideoFrame,
        video::{SunshineHdrMetadata, VideoFormat, VideoFormats, VideoSetup},
    },
    webrtc::sdp::Session,
};
use tokio::{
    select, spawn,
    sync::mpsc::{UnboundedSender, unbounded_channel},
};
use tracing::{Instrument, debug, debug_span, info, trace, warn};
use webrtc::{
    api::media_engine::{MIME_TYPE_AV1, MIME_TYPE_H264, MIME_TYPE_HEVC},
    peer_connection::RTCPeerConnection,
    rtcp::payload_feedbacks::{
        picture_loss_indication::PictureLossIndication,
        receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate,
    },
    rtp::{
        codecs::{
            av1::Av1Payloader,
            h264::H264Payloader,
            h265::{HevcPayloader, RTP_OUTBOUND_MTU},
        },
        extension::{HeaderExtension, playout_delay_extension::PlayoutDelayExtension},
        header::Header,
        packet::Packet,
        packetizer::Payloader,
    },
    rtp_transceiver::{
        RTCPFeedback,
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters},
        rtp_sender::RTCRtpSender,
    },
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::{api::stream::webrtc::ext_color_space::ColorSpaceExtension, app::AppError};

pub enum VideoChannelEvent {
    SignalIdr,
}

enum Message {
    Frame(OwnedVideoFrame),
    HdrMetadata(Option<SunshineHdrMetadata>),
}

enum State {
    SelectVideoFormat,
    Panic,
    Sending {
        rtcp_buffer: Vec<u8>,
        rtcp_receiver: Arc<RTCRtpSender>,
        frame_sender: UnboundedSender<Message>,
    },
}

pub struct VideoChannel {
    video_formats: HashMap<VideoFormat, RTCRtpCodecParameters>,
    state: State,
}

impl VideoChannel {
    pub fn new(sdp: &Session, preferred_formats: VideoFormats) -> Result<Self, AppError> {
        let mut video_formats_mapping = get_video_formats(sdp);

        // Remove all not preferred codecs
        for format in VideoFormat::all() {
            if !format.contained_in(preferred_formats) {
                video_formats_mapping.remove(&format);
            }
        }

        debug!(formats = ?video_formats_mapping, "found video codecs");

        Ok(Self {
            video_formats: video_formats_mapping,
            state: State::SelectVideoFormat,
        })
    }

    pub fn supported_video_formats(&self) -> &HashMap<VideoFormat, RTCRtpCodecParameters> {
        &self.video_formats
    }

    pub async fn on_video_format_selected(
        &mut self,
        setup: VideoSetup,
        peer: &RTCPeerConnection,
    ) -> Result<(), AppError> {
        let mut new_state = State::Panic;
        swap(&mut new_state, &mut self.state);

        // Check video format
        let format = setup.format;
        let Some(codec) = self.video_formats.remove(&format) else {
            return Err(AppError::WebRtcClientCodecNotSupported);
        };

        // Create video track
        let clock_rate = codec.capability.clock_rate;
        let track = Arc::new(TrackLocalStaticRTP::new(
            codec.capability.clone(),
            "video".to_string(),
            "moonlight".to_string(),
        ));

        let video_sender = peer.add_track(track.clone()).await?;

        let mut payloader = if format.contained_in(VideoFormats::MASK_H264) {
            Box::new(H264Payloader::default()) as Box<dyn Payloader + Send + Sync>
        } else if format.contained_in(VideoFormats::MASK_H265) {
            Box::new(HevcPayloader::default()) as Box<dyn Payloader + Send + Sync>
        } else {
            Box::new(Av1Payloader::default()) as Box<dyn Payloader + Send + Sync>
        };

        let (frame_sender, mut frame_receiver) = unbounded_channel();

        self.state = State::Sending {
            rtcp_buffer: vec![0u8; 1500],
            rtcp_receiver: video_sender,
            frame_sender,
        };

        spawn(
            async move {
                let mut sequence_number = 0u16;

                let mut hdr_metadata = None;

                while let Some(message) = frame_receiver.recv().await {
                    let frame = match message {
                        Message::HdrMetadata(metadata) => {
                            hdr_metadata = metadata;
                            continue;
                        }
                        Message::Frame(frame) => frame,
                    };
                    let frame = frame.as_ref();

                    let timestamp =
                        (frame.metadata.timestamp.as_millis() * clock_rate as u128 / 1000) as u32;

                    if track.all_binding_paused().await {
                        trace!("video track all binding paused");
                        // The peer may still be negotiating when Sunshine sends its first
                        // frame. Drop that frame, but keep the relay alive for the bound track.
                        continue;
                    }

                    let mut payloads = Vec::with_capacity(10);

                    // Each buffer is one nal
                    for buffer in &frame.buffers {
                        let nal_payloads = payloader
                            .payload(RTP_OUTBOUND_MTU, &Bytes::copy_from_slice(buffer.data))
                            .expect("failed to payload frame");

                        payloads.extend(nal_payloads);
                    }

                    let len = payloads.len();
                    for (i, payload) in payloads.into_iter().enumerate() {
                        sequence_number = sequence_number.wrapping_add(1);

                        let is_last = i == len - 1;

                        let extensions: &[HeaderExtension] =
                            if is_last && let Some(_metadata) = &hdr_metadata {
                                // TODO: find correct hdr fields
                                let _color_space = ColorSpaceExtension::default();

                                &[
                                    HeaderExtension::PlayoutDelay(PlayoutDelayExtension {
                                        min_delay: 0,
                                        max_delay: 0,
                                    }),
                                    // HeaderExtension::Custom {
                                    //     uri: Cow::Borrowed(COLOR_SPACE_URI),
                                    //     extension: Box::new(ColorSpaceExtension {}),
                                    // },
                                ]
                            } else {
                                &[HeaderExtension::PlayoutDelay(PlayoutDelayExtension {
                                    min_delay: 0,
                                    max_delay: 0,
                                })]
                            };

                        if let Err(err) = track
                            .write_rtp_with_extensions(
                                &Packet {
                                    header: Header {
                                        version: 2,
                                        // Marker needs to mark the end of one frame
                                        marker: is_last,
                                        sequence_number,
                                        timestamp,
                                        payload_type: codec.payload_type,
                                        ..Default::default()
                                    },
                                    payload,
                                },
                                extensions,
                            )
                            .await
                        {
                            warn!(error = %err, "failed to send video packet");
                        }
                    }
                }
            }
            .instrument(debug_span!("video frame relay")),
        );

        info!(setup = ?setup, codec = ?codec, "finished video track setup");

        Ok(())
    }

    pub fn on_frame(&mut self, frame: OwnedVideoFrame) {
        match &mut self.state {
            State::SelectVideoFormat | State::Panic => {
                panic!("VideoChannel is in an invalid state")
            }
            State::Sending { frame_sender, .. } => {
                let _ = frame_sender.send(Message::Frame(frame));
            }
        }
    }

    pub fn set_hdr_enabled(&mut self, _enabled: bool, metadata: Option<SunshineHdrMetadata>) {
        match &mut self.state {
            State::SelectVideoFormat | State::Panic => {
                panic!("VideoChannel is in an invalid state")
            }
            State::Sending { frame_sender, .. } => {
                let _ = frame_sender.send(Message::HdrMetadata(metadata));
            }
        }
    }

    pub async fn drive(&mut self) -> Result<VideoChannelEvent, AppError> {
        loop {
            let State::Sending {
                rtcp_buffer,
                rtcp_receiver,
                ..
            } = &mut self.state
            else {
                panic!("VideoChannel is in an invalid state");
            };

            select! {
                // This function seems cancel safe
                result = rtcp_receiver.read(rtcp_buffer) => {
                    let Ok((packets, _)) = result else {
                        continue;
                    };

                    for packet in packets {
                        let packet = packet.as_any();

                        if packet.downcast_ref::<PictureLossIndication>().is_some() {
                            debug!("got picture loss indication, set need idr flag");
                            return Ok(VideoChannelEvent::SignalIdr);
                        } else if let Some(ReceiverEstimatedMaximumBitrate { bitrate: _, .. }) =
                            packet.downcast_ref::<ReceiverEstimatedMaximumBitrate>()
                        {
                            // TODO
                        }
                    }
                }
            }
        }
    }
}

fn get_video_formats(sdp: &Session) -> HashMap<VideoFormat, RTCRtpCodecParameters> {
    let mut formats = HashMap::default();

    // -- Find and extract codec and sdp fmtp line
    let mut codec_and_clock_rate = HashMap::<_, (&str, _)>::default();
    let mut sdp_fmtp_lines = HashMap::<_, &str>::default();

    for media in &sdp.medias {
        for attribute in &media.attributes {
            let Some(value) = &attribute.value else {
                continue;
            };

            match attribute.attribute.as_str() {
                "rtpmap" => {
                    let Some((pt, codec, clock_rate)) = parse_rtpmap(value) else {
                        warn!(attribute = ?attribute, "failed to parse rtpmap");
                        continue;
                    };

                    codec_and_clock_rate.insert(pt, (codec, clock_rate));
                }
                "fmtp" => {
                    let Some((pt, sdp_fmtp_line)) = parse_fmtp(value) else {
                        warn!(attribute = ?attribute, "failed to parse fmtp");
                        continue;
                    };

                    sdp_fmtp_lines.insert(pt, sdp_fmtp_line);
                }
                _ => {}
            }
        }
    }

    // -- Add all recognized codecs
    for (pt, (codec, clock_rate)) in &codec_and_clock_rate {
        let sdp_fmtp_line = sdp_fmtp_lines.get(pt).unwrap_or(&"");
        debug!(pt = *pt, codec = ?codec, clock_rate = ?clock_rate, sdp_fmtp_line = ?sdp_fmtp_line, "got codec");

        if codec.eq_ignore_ascii_case("H264") {
            if !sdp_fmtp_line.contains("packetization-mode=1") {
                // Single NAL mode is not supported
                continue;
            }

            // Get profile
            let mut format = VideoFormat::H264;

            let attributes = sdp_fmtp_line.split(";");
            for (attribute, value) in attributes.filter_map(|attribute| attribute.split_once("=")) {
                if attribute == "profile-level-id" {
                    if value.starts_with("64") {
                        format = VideoFormat::H264;
                    } else if value.starts_with("f4") {
                        format = VideoFormat::H264High8_444;
                    } else {
                        debug!(profile_level_id = ?value, "found unknown h264 profile-level-id");
                    }
                }
            }

            formats.insert(
                format,
                RTCRtpCodecParameters {
                    capability: RTCRtpCodecCapability {
                        mime_type: MIME_TYPE_H264.to_string(),
                        sdp_fmtp_line: sdp_fmtp_line.to_string(),
                        clock_rate: *clock_rate,
                        rtcp_feedback: rtcp_feedback(),
                        ..Default::default()
                    },
                    payload_type: *pt,
                    ..Default::default()
                },
            );
        } else if codec.eq_ignore_ascii_case("H265") {
            // Get profile
            let mut format = VideoFormat::H265;

            let attributes = sdp_fmtp_line.split(";");
            for (attribute, value) in attributes.filter_map(|attribute| attribute.split_once("=")) {
                if attribute == "profile-id" {
                    match value {
                        "1" => format = VideoFormat::H265,
                        "2" => format = VideoFormat::H265Main10,
                        "4" => {
                            // TODO: range extensions
                        }
                        _ => debug!(profile_id = ?value, "unknown h265 profile-id"),
                    }
                }
            }

            formats.insert(
                format,
                RTCRtpCodecParameters {
                    capability: RTCRtpCodecCapability {
                        mime_type: MIME_TYPE_HEVC.to_string(),
                        sdp_fmtp_line: sdp_fmtp_line.to_string(),
                        clock_rate: *clock_rate,
                        rtcp_feedback: rtcp_feedback(),
                        ..Default::default()
                    },
                    payload_type: *pt,
                    ..Default::default()
                },
            );
        } else if codec.eq_ignore_ascii_case("AV1") {
            // Get profile
            let mut format = VideoFormat::Av1Main8;

            let attributes = sdp_fmtp_line.split(";");
            for (attribute, value) in attributes.filter_map(|attribute| attribute.split_once("=")) {
                if attribute == "profile" {
                    match value {
                        "1" => format = VideoFormat::Av1Main8,
                        "2" => format = VideoFormat::Av1High8_444,
                        // TODO: how do the Main10 / High10 profiles work?
                        _ => debug!(profile = ?value, "unknown av1 profile"),
                    }
                }
            }

            formats.insert(
                format,
                RTCRtpCodecParameters {
                    capability: RTCRtpCodecCapability {
                        mime_type: MIME_TYPE_AV1.to_string(),
                        sdp_fmtp_line: sdp_fmtp_line.to_string(),
                        clock_rate: *clock_rate,
                        rtcp_feedback: rtcp_feedback(),
                        ..Default::default()
                    },
                    payload_type: *pt,
                    ..Default::default()
                },
            );
        }
    }

    formats
}
fn parse_rtpmap(attribute_value: &str) -> Option<(u8, &str, u32)> {
    let (pt_str, full_codec) = attribute_value.split_once(' ')?;
    let pt = pt_str.parse::<u8>().ok()?;

    // identify codec
    let (codec_str, clock_rate_str) = full_codec.split_once('/')?;

    let clock_rate = clock_rate_str.parse::<u32>().ok()?;

    Some((pt, codec_str, clock_rate))
}
fn parse_fmtp(attribute_value: &str) -> Option<(u8, &str)> {
    let (pt_str, sdp_fmtp_line) = attribute_value.split_once(' ')?;
    let pt = pt_str.parse::<u8>().ok()?;

    Some((pt, sdp_fmtp_line))
}

fn rtcp_feedback() -> Vec<RTCPFeedback> {
    vec![
        RTCPFeedback {
            // negative acknowledgement
            typ: "nack".to_string(),
            parameter: "".to_string(),
        },
        RTCPFeedback {
            // picture loss indicator (idr)
            typ: "nack".to_string(),
            parameter: "pli".to_string(),
        },
        RTCPFeedback {
            // receiver estimated maximum bitrate
            typ: "goog-remb".to_string(),
            parameter: "".to_string(),
        },
    ]
}
