use moonlight_common::stream::video::VideoFormat;
use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_HEVC},
    rtp_transceiver::{RTCPFeedback, rtp_codec::RTCRtpCodecCapability},
};

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
    VideoFormat::H264 = MIME_TYPE_H264: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f",
    VideoFormat::H264High8_444 = MIME_TYPE_H264: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=f4001f",

    // H265
    VideoFormat::H265 = MIME_TYPE_HEVC: "packetization-mode=1",
    VideoFormat::H265Main10 = MIME_TYPE_HEVC: "profile-id=2;level-id=93;tier-flag=0;packetization-mode=1",
    VideoFormat::H265Rext8_444 = MIME_TYPE_HEVC: "profile-id=4;level-id=93;tier-flag=0;packetization-mode=1",
    VideoFormat::H265Rext10_444 = MIME_TYPE_HEVC: "profile-id=5;level-id=93;tier-flag=0;packetization-mode=1"
    // AV1
    // TODO: av1
);
