use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use bytes::Bytes;
use moonlight_common::{
    stream::{
        proto::control::packet::ControlPacket,
        tokio::MoonlightStream,
        video::{VideoFormat, VideoFormats},
    },
    webrtc::sdp::Session,
};
use tokio::spawn;
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
    },
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::app::AppError;

pub fn get_video_formats(sdp: &Session) -> HashMap<VideoFormat, RTCRtpCodecParameters> {
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
    let mut current_pt = 112;

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
                    payload_type: current_pt,
                    ..Default::default()
                },
            );
            current_pt += 1;
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
                    payload_type: current_pt,
                    ..Default::default()
                },
            );
            current_pt += 1;
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
                    payload_type: current_pt,
                    ..Default::default()
                },
            );
            current_pt += 1;
        }
    }

    debug!(formats = ?formats, "found video codecs");

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

pub async fn add_video_track(
    peer: &RTCPeerConnection,
    stream: &MoonlightStream,
    mut video_formats: HashMap<VideoFormat, RTCRtpCodecParameters>,
) -> Result<(), AppError> {
    // Check video format
    let format = stream.video_setup().format;
    let Some(codec) = video_formats.remove(&format) else {
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

    // Feedback
    let need_idr = Arc::new(AtomicBool::new(false));
    spawn({
        let need_idr = need_idr.clone();

        async move {
            let mut buffer = [0; 1500];

            while let Ok((packets, _)) = video_sender.read(&mut buffer).await {
                for packet in packets {
                    let packet = packet.as_any();

                    if packet.downcast_ref::<PictureLossIndication>().is_some() {
                        debug!("got picture loss indication, set need idr flag");
                        need_idr.store(true, Ordering::Release);
                    } else if let Some(ReceiverEstimatedMaximumBitrate { bitrate: _, .. }) =
                        packet.downcast_ref::<ReceiverEstimatedMaximumBitrate>()
                    {
                        // TODO
                    }
                }
            }
        }
        .instrument(debug_span!("video rtcp feedback"))
    });

    let mut payloader = if format.contained_in(VideoFormats::MASK_H264) {
        Box::new(H264Payloader::default()) as Box<dyn Payloader + Send + Sync>
    } else if format.contained_in(VideoFormats::MASK_H265) {
        Box::new(HevcPayloader::default()) as Box<dyn Payloader + Send + Sync>
    } else {
        Box::new(Av1Payloader::default()) as Box<dyn Payloader + Send + Sync>
    };

    // Spawn forwarding task
    spawn({
        let stream = stream.clone();
        let need_idr = need_idr.clone();

        async move {
            let mut sequence_number = 0;

            while let Ok(frame) = stream.poll_video_frame().await {
                let frame = frame.as_ref();

                let timestamp = (frame.metadata.timestamp.as_secs_f64() * clock_rate as f64) as u32;

                if track.all_binding_paused().await {
                    trace!("video track all binding paused");
                    // Don't send any packets when the track is paused because we don't want to increment the sequence number
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
                    sequence_number += 1;

                    if let Err(err) = track
                        .write_rtp_with_extensions(
                            &Packet {
                                header: Header {
                                    version: 2,
                                    // Marker needs to mark the end of one frame
                                    marker: i == len - 1,
                                    sequence_number,
                                    timestamp,
                                    payload_type: codec.payload_type,
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

                // Check if idr is needed
                if need_idr
                    .compare_exchange(true, false, Ordering::Acquire, Ordering::Acquire)
                    .is_ok()
                {
                    info!("requesting idr");
                    if let Err(err) = stream.send_raw(ControlPacket::RequestIdr) {
                        warn!(error = %err, "failed to send idr request");
                    }
                }
            }
        }
        .instrument(debug_span!("relay: video"))
    });

    info!(setup = ?stream.video_setup(), codec = ?codec, "finished video track setup");

    Ok(())
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
