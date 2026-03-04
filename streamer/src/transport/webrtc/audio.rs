use std::{sync::Weak, time::Duration};

use bytes::Bytes;
use log::{error, warn};
use moonlight_common::stream::audio::{AudioConfig, OpusMultistreamConfig};
use tokio::runtime::Handle;
use webrtc::{
    api::media_engine::{MIME_TYPE_OPUS, MediaEngine},
    media::Sample,
    peer_connection::RTCPeerConnection,
    rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType},
    track::track_local::track_local_static_sample::TrackLocalStaticSample,
};

use crate::transport::webrtc::{WebRtcInner, sender::TrackLocalSender};

pub fn register_audio_codecs(media_engine: &mut MediaEngine) -> Result<(), webrtc::Error> {
    media_engine.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 111,
            ..Default::default()
        },
        RTPCodecType::Audio,
    )?;

    Ok(())
}

pub struct WebRtcAudio {
    sender: TrackLocalSender<TrackLocalStaticSample>,
    config: Option<OpusMultistreamConfig>,
}

impl WebRtcAudio {
    pub fn new(runtime: Handle, peer: Weak<RTCPeerConnection>, channel_queue_size: usize) -> Self {
        Self {
            sender: TrackLocalSender::new(runtime, peer, channel_queue_size),
            config: None,
        }
    }
}

impl WebRtcAudio {
    pub async fn setup(
        &mut self,
        inner: &WebRtcInner,
        audio_config: AudioConfig,
        stream_config: OpusMultistreamConfig,
    ) -> i32 {
        const SUPPORTED_SAMPLE_RATES: &[u32] = &[80000, 12000, 16000, 24000, 48000];
        if !SUPPORTED_SAMPLE_RATES.contains(&stream_config.sample_rate) {
            warn!(
                "[Stream] Audio could have problems because of the sample rate, Selected: {}, Expected one of: {SUPPORTED_SAMPLE_RATES:?}",
                stream_config.sample_rate
            );
        }
        if audio_config != self.config() {
            warn!(
                "[Stream] A different audio configuration than requested was selected, Expected: {:?}, Found: {audio_config:?}",
                self.config()
            );
        }

        if let Err(err) = self
            .sender
            .create_track(
                TrackLocalStaticSample::new(
                    RTCRtpCodecCapability {
                        mime_type: MIME_TYPE_OPUS.to_string(),
                        ..Default::default()
                    },
                    "audio".to_string(),
                    "moonlight".to_string(),
                ),
                |_| {},
            )
            .await
        {
            error!("Failed to create opus track: {err:?}");
            return -1;
        };

        self.config = Some(stream_config);

        // Renegotiate
        if !inner.send_offer().await {
            warn!("Failed to renegotiate. Audio was added!");
        }

        0
    }

    pub async fn send_audio_sample(&mut self, data: &[u8]) {
        let Some(config) = self.config.as_ref() else {
            return;
        };

        let duration =
            Duration::from_secs_f64(config.samples_per_frame as f64 / config.sample_rate as f64);

        let data = Bytes::copy_from_slice(data);

        let sample = Sample {
            data,
            duration,
            // Time should be set if you want fine-grained sync
            ..Default::default()
        };

        self.sender.send_samples(vec![sample], false).await;
    }

    fn config(&self) -> AudioConfig {
        AudioConfig::STEREO
    }
}
