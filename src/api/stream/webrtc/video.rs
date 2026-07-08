use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use bytes::Bytes;
use moonlight_common::stream::{
    proto::control::packet::ControlPacket,
    tokio::MoonlightStream,
    video::{VideoFormat, VideoFormats},
};
use tokio::spawn;
use tracing::{Instrument, debug, debug_span, info, trace, warn};
use webrtc::{
    Error,
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_HEVC},
    peer_connection::RTCPeerConnection,
    rtcp::payload_feedbacks::{
        picture_loss_indication::PictureLossIndication,
        receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate,
    },
    rtp::{
        codecs::{
            h264::H264Payloader,
            h265::{HevcPayloader, RTP_OUTBOUND_MTU},
        },
        extension::{HeaderExtension, playout_delay_extension::PlayoutDelayExtension},
        header::Header,
        packet::Packet,
        packetizer::Payloader,
    },
    rtp_transceiver::{RTCPFeedback, rtp_codec::RTCRtpCodecCapability},
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

pub async fn add_video_track(
    peer: &RTCPeerConnection,
    stream: &MoonlightStream,
    allowed_formats: VideoFormats,
) -> Result<(), Error> {
    // Check video format
    let format = stream.video_setup().format;
    if !allowed_formats.contains(format.into_formats()) {
        todo!();
    }

    // Create video track
    let codec = video_format_to_codec(format).expect("webrtc video codec");
    let track = Arc::new(TrackLocalStaticRTP::new(
        codec.clone(),
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
        todo!()
    };

    // Spawn forwarding task
    spawn({
        let stream = stream.clone();
        let need_idr = need_idr.clone();

        async move {
            let mut sequence_number = 0;

            while let Ok(frame) = stream.poll_video_frame().await {
                let frame = frame.as_ref();

                let timestamp = (frame.metadata.timestamp.as_secs_f64() * 90000.0) as u32;

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
                                    // TODO: this needs to match
                                    payload_type: 96,
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

macro_rules! video_formats_codec_mapping {
    ($($format:path = $mime_type:ident : $sdp_fmtp_line:expr ),*) => {
        pub fn video_format_to_codec(format: VideoFormat) -> Option<RTCRtpCodecCapability> {
            match format {
                $(
                    $format => Some(RTCRtpCodecCapability {
                        mime_type: $mime_type.to_string(),
                        sdp_fmtp_line: $sdp_fmtp_line.to_string(),
                        clock_rate: 90000,
                        rtcp_feedback: rtcp_feedback(),
                        channels: 0,
                    }),
                )*
                _ => None,
            }
        }

        pub fn codec_to_video_format(codec: &RTCRtpCodecCapability) -> Option<VideoFormat> {
            match codec.mime_type.as_str() {
                $(
                    // TODO: check the sdp fmtp line?
                    $mime_type => Some($format),
                )*
                _ => None
            }
        }
    };
}

video_formats_codec_mapping!(
    // H264
    VideoFormat::H264 = MIME_TYPE_H264: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f",
    VideoFormat::H264High8_444 = MIME_TYPE_H264: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640032",

    // H265
    VideoFormat::H265 = MIME_TYPE_HEVC: "packetization-mode=1",
    VideoFormat::H265Main10 = MIME_TYPE_HEVC: "profile-id=2;level-id=93;tier-flag=0;packetization-mode=1",
    VideoFormat::H265Rext8_444 = MIME_TYPE_HEVC: "profile-id=4;level-id=93;tier-flag=0;packetization-mode=1",
    VideoFormat::H265Rext10_444 = MIME_TYPE_HEVC: "profile-id=5;level-id=93;tier-flag=0;packetization-mode=1"
    // AV1
    // TODO: av1
);
