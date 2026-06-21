use std::{ops::Deref, sync::Arc, time::Duration};

use tokio::{spawn, sync::mpsc::Sender, task::JoinHandle, time::sleep};
use tracing::{Level, Span, debug, instrument, warn};

use crate::app::{
    App, AppError, AppInner,
    user::{AuthenticatedUser, RoleType, User},
};

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
        owner: &AuthenticatedUser,
        event_sender: Sender<ExternalStreamEvent>,
    ) -> Result<Self, AppError> {
        app.insert_stream(|id| {
            let app_ref = app.new_ref();

            spawn({
                let event_sender = event_sender.clone();

                async move {
                loop {
                    sleep(Duration::from_secs(30)).await;

                    {
                        let app = match app_ref.access() {
                            Ok(value) => value,
                            Err(err) => {
                                warn!(stream_id = ?id, error = %err, "failed to aquire app in alive check for stream");
                                return;
                            }
                        };

                        if event_sender.is_closed() {
                            debug!("identified stream as closed, waiting some time and then removing it");
                            sleep(Duration::from_secs(10)).await;

                            let mut streams = app.streams.write().await;
                            streams.remove(&id);

                            debug!(stream_id = ?id, "removed stopped stream from app");
                            return;
                        }
                    }
                }
            }});

            Self {
                inner: Arc::new(StreamInner {
                    id,
                    owner: owner.deref().clone(),
                    event_sender,
                }),
            }
        })
        .await
    }

    pub fn id(&self) -> StreamId {
        self.inner.id
    }

    pub async fn owner(&self) -> Result<User, AppError> {
        Ok(self.inner.owner.clone())
    }

    async fn has_permissions(&self, user: &mut AuthenticatedUser) -> Result<(), AppError> {
        if matches!(user.role().await?.ty().await?, RoleType::Admin)
            || user.id() == self.owner().await?.id()
        {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    pub async fn send_event(
        &self,
        user: &mut AuthenticatedUser,
        event: ExternalStreamEvent,
    ) -> Result<(), AppError> {
        self.has_permissions(user).await?;

        self.inner
            .event_sender
            .send(event)
            .await
            .map_err(|_| AppError::StreamClosed)?;

        Ok(())
    }

    pub async fn full_log(&self, user: &mut AuthenticatedUser) -> Result<String, AppError> {
        self.has_permissions(user).await?;

        todo!()
    }

    pub fn is_alive(&self) -> Result<bool, AppError> {
        Ok(!self.inner.event_sender.is_closed())
    }
}

pub(crate) struct StreamInner {
    pub(crate) id: StreamId,
    pub(crate) owner: User,
    pub(crate) event_sender: Sender<ExternalStreamEvent>,
}
