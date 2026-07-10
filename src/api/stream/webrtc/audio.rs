use std::sync::Arc;

use bytes::Bytes;
use moonlight_common::stream::tokio::MoonlightStream;
use tokio::spawn;
use tracing::{Instrument, debug_span, info, trace, warn};
use webrtc::{
    Error,
    peer_connection::RTCPeerConnection,
    rtp::{
        extension::{HeaderExtension, playout_delay_extension::PlayoutDelayExtension},
        header::Header,
        packet::Packet,
    },
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::api::stream::webrtc::opus_codec;

pub async fn add_audio_track(
    peer: &RTCPeerConnection,
    stream: &MoonlightStream,
) -> Result<(), Error> {
    // TODO: what audio format is used?

    let config = stream.audio_setup();

    // Create audio track
    let mut codec = opus_codec();
    codec.channels = config.channel_count as u16;

    let track = Arc::new(TrackLocalStaticRTP::new(
        codec.clone(),
        "audio".to_string(),
        "moonlight".to_string(),
    ));

    let audio_sender = peer.add_track(track.clone()).await?;

    // Feedback
    spawn(
        async move {
            let mut buffer = [0; 1500];

            while let Ok((_packets, _)) = audio_sender.read(&mut buffer).await {
                // do nothing, because we'll just have to poll packets to dequeue them
            }
        }
        .instrument(debug_span!("audio rtcp feedback")),
    );

    info!(audio_config = ?config, codec = ?codec, "finished audio track setup");

    spawn({
        let stream = stream.clone();

        async move {
            let mut sequence_number = 0;

            while let Ok(frame) = stream.poll_audio_frame().await {
                let timestamp = (frame.timestamp.as_millis() * 48) as u32;

                if track.all_binding_paused().await {
                    trace!("audio track all binding paused");
                    // Don't send any packets when the track is paused because we don't want to increment the sequence number
                    return;
                }

                trace!(len = ?frame.buffer.len(), timestamp = ?frame.timestamp, "audio frame");

                sequence_number += 1;

                // Opus doesn't need any special payloading: https://github.com/webrtc-rs/webrtc/blob/6b94718e23111df28125f96af4b0de8cbb3dfd0d/rtp/src/codecs/opus/mod.rs#L9-L24
                if let Err(err) = track
                    .write_rtp_with_extensions(
                        &Packet {
                            header: Header {
                                version: 2,
                                sequence_number,
                                timestamp,
                                payload_type: 111,
                                ..Default::default()
                            },
                            payload: Bytes::from_owner(frame.buffer),
                        },
                        &[HeaderExtension::PlayoutDelay(PlayoutDelayExtension {
                            min_delay: 0,
                            max_delay: 0,
                        })],
                    )
                    .await
                {
                    warn!(error = %err, "failed to send audio frame");
                }
            }
        }
        .instrument(debug_span!("relay: audio"))
    });

    Ok(())
}
