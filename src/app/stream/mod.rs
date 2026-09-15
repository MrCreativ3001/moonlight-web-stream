use std::{sync::Arc, time::Duration};

use tokio::{spawn, sync::mpsc::Sender, time::sleep};
use tracing::{debug, warn};

use crate::app::{App, AppError};

pub enum ExternalStreamEvent {
    WebRTCAddIceCandidate { ice_sdp_frag: String },
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamId(pub u32);

#[derive(Clone)]
pub struct Stream {
    inner: Arc<StreamInner>,
}

impl Stream {
    pub async fn new(
        app: &App,
        event_sender: Sender<ExternalStreamEvent>,
    ) -> Result<Self, AppError> {
        app.insert_stream(|id| {
            let app_ref = app.new_ref();
            spawn({
                let event_sender = event_sender.clone();
                async move {
                    loop {
                        sleep(Duration::from_secs(30)).await;
                        let app = match app_ref.access() {
                            Ok(value) => value,
                            Err(err) => {
                                warn!(stream_id = ?id, error = %err, "failed to acquire app in alive check for stream");
                                return;
                            }
                        };
                        if event_sender.is_closed() {
                            debug!("identified stream as closed, waiting before removing it");
                            sleep(Duration::from_secs(10)).await;
                            app.streams.write().await.remove(&id);
                            return;
                        }
                    }
                }
            });
            Self { inner: Arc::new(StreamInner { id, event_sender }) }
        }).await
    }

    pub fn id(&self) -> StreamId {
        self.inner.id
    }

    pub async fn send_event(&self, event: ExternalStreamEvent) -> Result<(), AppError> {
        self.inner
            .event_sender
            .send(event)
            .await
            .map_err(|_| AppError::StreamClosed)
    }

    #[allow(unused)]
    pub fn is_alive(&self) -> Result<bool, AppError> {
        Ok(!self.inner.event_sender.is_closed())
    }
}

pub(crate) struct StreamInner {
    pub(crate) id: StreamId,
    pub(crate) event_sender: Sender<ExternalStreamEvent>,
}
