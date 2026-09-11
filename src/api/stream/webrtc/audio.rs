use std::sync::Arc;

use bytes::Bytes;
use moonlight_common::{
    crypto::rustcrypto::RustCryptoBackend,
    http::pair::PairingCryptoBackend,
    stream::{audio::AudioFrame, tokio::MoonlightStream},
};
use rtc::{
    media_stream::MediaStreamTrack,
    rtp::{
        Header, Packet,
        extension::{HeaderExtension, playout_delay_extension::PlayoutDelayExtension},
    },
    rtp_transceiver::{
        SSRC,
        rtp_sender::{RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind},
    },
};
use tokio::{
    spawn,
    sync::mpsc::{UnboundedSender, unbounded_channel},
};
use tracing::{Instrument, debug_span, info, trace, warn};
use webrtc::{
    media_stream::track_local::{TrackLocal, static_rtp::TrackLocalStaticRTP},
    peer_connection::PeerConnection,
};

use crate::{api::stream::webrtc::opus_codec, app::AppError};

// TODO: add audio over unreliable data channels using AudioStream and AudioPayloader

pub struct AudioChannel {
    frame_sender: UnboundedSender<AudioFrame<Bytes>>,
}

impl AudioChannel {
    pub async fn new_track(
        stream: &MoonlightStream,
        peer: &dyn PeerConnection,
    ) -> Result<Self, AppError> {
        // TODO: what audio format is used?

        let config = stream.audio_setup();

        // Create audio track
        let mut ssrc = [0; 4];
        RustCryptoBackend.random_bytes(&mut ssrc)?;
        let ssrc = SSRC::from_ne_bytes(ssrc);

        let mut codec = opus_codec();
        codec.channels = config.channel_count as u16;

        let track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
            "audio".to_string(),
            "audio".to_string(),
            "audio".to_string(),
            RtpCodecKind::Audio,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(ssrc),
                    ..Default::default()
                },
                codec: codec.clone(),
                ..Default::default()
            }],
        )));

        peer.add_track(Arc::clone(&track) as Arc<dyn TrackLocal>)
            .await?;

        info!(audio_config = ?config, codec = ?codec, "finished audio track setup");

        let (frame_sender, mut frame_receiver) = unbounded_channel::<AudioFrame<Bytes>>();

        spawn(
            async move {
                let mut sequence_number = 0u16;

                while let Some(frame) = frame_receiver.recv().await {
                    let timestamp = (frame.timestamp.as_millis() * 48) as u32;

                    trace!(len = ?frame.buffer.len(), timestamp = ?frame.timestamp, "audio frame");

                    sequence_number = sequence_number.wrapping_add(1);

                    // Opus doesn't need any special payloading: https://github.com/webrtc-rs/webrtc/blob/6b94718e23111df28125f96af4b0de8cbb3dfd0d/rtp/src/codecs/opus/mod.rs#L9-L24
                    if let Err(err) = track
                        .write_rtp(
                            Packet {
                                header: Header {
                                    version: 2,
                                    sequence_number,
                                    timestamp,
                                    payload_type: 111,
                                    ssrc,
                                    ..Default::default()
                                },
                                payload: frame.buffer,
                            },
                            // &[HeaderExtension::PlayoutDelay(PlayoutDelayExtension {
                            //     min_delay: 0,
                            //     max_delay: 0,
                            // })],
                        )
                        .await
                    {
                        warn!(error = %err, "failed to send audio frame");
                    }
                }
            }
            .instrument(debug_span!("audio frame relay")),
        );

        Ok(Self { frame_sender })
    }

    pub fn on_frame(&mut self, frame: AudioFrame<Bytes>) {
        let _ = self.frame_sender.send(frame);
    }
}
