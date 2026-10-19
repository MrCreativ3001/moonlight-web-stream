use std::{future::pending, sync::Arc, time::Duration};

use bytes::Bytes;
use moonlight_common::webrtc::sdp::Session;
use tokio::{
    spawn,
    sync::mpsc::{UnboundedReceiver, unbounded_channel},
};
use tracing::{Instrument, debug, debug_span};
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};

use crate::app::AppError;

pub enum MicChannelEvent {
    OnFrame { timestamp: Duration, data: Bytes },
    Closed,
}

pub struct MicChannel {
    receiver: Option<UnboundedReceiver<MicChannelEvent>>,
}

impl MicChannel {
    pub async fn new(offer_sdp: &Session) -> Option<Self> {
        let mut has_mic = false;

        'outside: for media in &offer_sdp.medias {
            for attribute in &media.attributes {
                let attribute = &attribute.attribute;
                if attribute == "sendonly" || attribute == "sendrecv" {
                    has_mic = true;
                    break 'outside;
                }
            }
        }

        if !has_mic {
            return None;
        }

        Some(Self { receiver: None })
    }

    pub fn set_track(&mut self, track: Arc<dyn TrackRemote>) {
        let (sender, receiver) = unbounded_channel();

        spawn(
            async move {
                while let Some(event) = track.poll().await {
                    match event {
                        TrackRemoteEvent::OnOpen(info) => {
                            debug!(info = ?info, "current microphone channel");
                        }
                        TrackRemoteEvent::OnRtpPacket(packet) => {
                            let timestamp = Duration::from_millis(
                                // opus clock rate is 48
                                (packet.header.timestamp / 48) as u64,
                            );

                            // Opus frames don't have any packetization
                            if sender
                                .send(MicChannelEvent::OnFrame {
                                    timestamp,
                                    data: packet.payload,
                                })
                                .is_err()
                            {
                                debug!(
                                    "mic receiver was closed, closing remote track polling task"
                                );
                                break;
                            }
                        }
                        TrackRemoteEvent::OnEnded => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
            .instrument(debug_span!("microphone track remote")),
        );

        self.receiver = Some(receiver);
    }

    pub async fn drive(&mut self) -> Result<MicChannelEvent, AppError> {
        if let Some(receiver) = self.receiver.as_mut() {
            let maybe_event = receiver.recv().await;

            match maybe_event {
                Some(event) => Ok(event),
                None => {
                    self.receiver = None;
                    Ok(MicChannelEvent::Closed)
                }
            }
        } else {
            pending().await
        }
    }
}
