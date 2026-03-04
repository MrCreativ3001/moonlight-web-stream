use std::{
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
};

use actix_web::{HttpRequest, HttpResponse, Responder, body::BoxBody, web::Bytes};
use futures::Stream;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::mpsc::{Receiver, Sender, channel};

pub struct StreamedResponse<Initial, Other> {
    receiver: Receiver<Other>,
    initial: Initial,
}

impl<Initial, Other> StreamedResponse<Initial, Other> {
    pub fn new(initial: Initial) -> (Self, StreamedResponseSender<Other>) {
        let (sender, receiver) = channel(1);

        let stream_sender = StreamedResponseSender { sender };

        (Self { initial, receiver }, stream_sender)
    }

    pub fn set_initial(&mut self, initial: Initial) {
        self.initial = initial;
    }
}

impl<Initial, Other> Responder for StreamedResponse<Initial, Other>
where
    Initial: Serialize + Unpin + 'static,
    Other: Serialize + Unpin + 'static,
{
    type Body = BoxBody;

    fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
        let stream = StreamedResponseReceiver {
            initial: Some(self.initial),
            receiver: self.receiver,
        };

        HttpResponse::Ok()
            .insert_header(("Content-Type", "application/x-ndjson"))
            .streaming(stream)
    }
}

struct StreamedResponseReceiver<Initial, Other> {
    initial: Option<Initial>,
    receiver: Receiver<Other>,
}

impl<Initial, Other> Stream for StreamedResponseReceiver<Initial, Other>
where
    Initial: Serialize + Unpin,
    Other: Serialize,
{
    type Item = Result<Bytes, serde_json::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        let initial = &mut this.initial;

        if let Some(initial) = initial.take() {
            let mut result = serde_json::to_string(&initial);
            if let Ok(text) = result.as_mut() {
                text.push('\n');
            }

            return Poll::Ready(Some(result.map(Bytes::from)));
        }

        this.receiver.poll_recv(cx).map(|option| {
            option.map(|value| {
                let mut result = serde_json::to_string(&value);
                if let Ok(text) = result.as_mut() {
                    text.push('\n');
                }

                result.map(Bytes::from)
            })
        })
    }
}

pub struct StreamedResponseSender<T> {
    sender: Sender<T>,
}

impl<T> Clone for StreamedResponseSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum StreamedResponseError {
    #[error("failed to send value whilst response streaming: {0}")]
    Send(anyhow::Error),
}

impl<T> StreamedResponseSender<T>
where
    T: Debug + Send + Sync + 'static,
{
    pub async fn send(&self, value: T) -> Result<(), StreamedResponseError> {
        self.sender
            .send(value)
            .await
            .map_err(|err| StreamedResponseError::Send(err.into()))?;

        Ok(())
    }
}
