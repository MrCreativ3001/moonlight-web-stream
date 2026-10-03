use std::{fmt, future::Future, sync::Arc, time::Duration};

use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, mpsc},
    task::JoinHandle,
    time::{Instant, timeout_at},
};

// Include the currently sending payload in this budget. The message ceiling also
// bounds queue overhead for tiny control/audio packets. Neither limit waits for
// capacity: overflow terminates the viewer rather than dropping dependent frames.
pub(super) const MAX_PENDING_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_PENDING_MESSAGES: usize = 512;
// This is an enqueue-to-send deadline, not a fresh timeout for every dequeue.
pub(super) const MAX_SEND_AGE: Duration = Duration::from_secs(2);

pub(super) trait Payload {
    fn retained_bytes(&self) -> usize;
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum SendError {
    ByteLimit,
    MessageLimit,
    Closed,
    Deadline,
    Sink,
    Task,
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ByteLimit => "web socket pending byte limit exceeded",
            Self::MessageLimit => "web socket pending message limit exceeded",
            Self::Closed => "web socket sender closed",
            Self::Deadline => "web socket enqueue-to-send deadline exceeded",
            Self::Sink => "web socket send failed",
            Self::Task => "web socket sender task failed",
        })
    }
}

struct Pending<T> {
    data: T,
    deadline: Instant,
    budget: OwnedSemaphorePermit,
}

pub(super) struct SendQueue<T> {
    sender: mpsc::Sender<Pending<T>>,
    budget: Arc<Semaphore>,
    max_age: Duration,
}

pub(super) struct SendReceiver<T> {
    receiver: mpsc::Receiver<Pending<T>>,
}

pub(super) fn channel<T>(
    max_bytes: usize,
    max_messages: usize,
    max_age: Duration,
) -> (SendQueue<T>, SendReceiver<T>) {
    let (sender, receiver) = mpsc::channel(max_messages);
    (
        SendQueue {
            sender,
            budget: Arc::new(Semaphore::new(max_bytes)),
            max_age,
        },
        SendReceiver { receiver },
    )
}

impl<T: Payload> SendQueue<T> {
    pub(super) fn try_send(&self, data: T) -> Result<(), SendError> {
        if self.sender.is_closed() {
            return Err(SendError::Closed);
        }
        let size = u32::try_from(data.retained_bytes()).map_err(|_| SendError::ByteLimit)?;
        let budget = self
            .budget
            .clone()
            .try_acquire_many_owned(size)
            .map_err(|_| SendError::ByteLimit)?;
        self.sender
            .try_send(Pending {
                data,
                deadline: Instant::now() + self.max_age,
                budget,
            })
            .map_err(|err| match err {
                mpsc::error::TrySendError::Full(_) => SendError::MessageLimit,
                mpsc::error::TrySendError::Closed(_) => SendError::Closed,
            })
    }
}

impl<T> SendReceiver<T> {
    pub(super) async fn run<F, Fut, E>(mut self, mut send: F) -> Result<(), SendError>
    where
        F: FnMut(T) -> Fut,
        Fut: Future<Output = Result<(), E>>,
    {
        while let Some(Pending {
            data,
            deadline,
            budget,
        }) = self.receiver.recv().await
        {
            // timeout_at may poll a ready future even after its deadline. Do not
            // submit a stale queued packet to the sink in that case.
            if Instant::now() >= deadline {
                return Err(SendError::Deadline);
            }
            timeout_at(deadline, send(data))
                .await
                .map_err(|_| SendError::Deadline)?
                .map_err(|_| SendError::Sink)?;
            // A ready sink future wins timeout_at's poll order even if the
            // executor resumes late. Treat that completion as a failed viewer.
            if Instant::now() >= deadline {
                return Err(SendError::Deadline);
            }
            drop(budget);
        }
        Ok(())
    }
}

// JoinHandle alone detaches on drop. Own it here so every exit/cancellation of
// the viewer handler cancels the sender; normal shutdown also awaits its drop.
pub(super) struct SenderTask(Option<JoinHandle<Result<(), SendError>>>);

impl SenderTask {
    pub(super) fn new(task: JoinHandle<Result<(), SendError>>) -> Self {
        Self(Some(task))
    }

    pub(super) async fn wait(&mut self) -> Result<(), SendError> {
        let result = self.0.as_mut().expect("sender task exists").await;
        self.0.take();
        result.map_err(|_| SendError::Task)?
    }

    pub(super) async fn stop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for SenderTask {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        future::pending,
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    use tokio::{
        runtime::Builder,
        task::yield_now,
        time::{advance, sleep},
    };

    impl Payload for Vec<u8> {
        fn retained_bytes(&self) -> usize {
            self.capacity()
        }
    }

    fn run_test(test: impl Future<Output = ()>) {
        Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap()
            .block_on(test);
    }

    #[test]
    fn byte_limit_is_immediate_and_includes_in_flight_payload() {
        run_test(async {
            let (queue, receiver) = channel(8, 4, MAX_SEND_AGE);
            queue.try_send(vec![0; 6]).unwrap();
            let mut task =
                SenderTask::new(tokio::spawn(receiver.run(|_| pending::<Result<(), ()>>())));
            yield_now().await;
            assert_eq!(queue.budget.available_permits(), 2);
            assert_eq!(queue.try_send(vec![0; 3]), Err(SendError::ByteLimit));
            queue.try_send(vec![0; 2]).unwrap();
            assert_eq!(queue.budget.available_permits(), 0);
            task.stop().await;
            assert_eq!(queue.budget.available_permits(), 8);
            assert_eq!(queue.try_send(vec![0]), Err(SendError::Closed));
        });
    }

    #[test]
    fn message_limit_bounds_tiny_messages_and_refunds_rejected_bytes() {
        run_test(async {
            let (queue, _receiver) = channel(8, 2, MAX_SEND_AGE);
            queue.try_send(vec![]).unwrap();
            queue.try_send(vec![0]).unwrap();
            assert_eq!(queue.try_send(vec![0]), Err(SendError::MessageLimit));
            assert_eq!(queue.budget.available_permits(), 7);
        });
    }

    #[test]
    fn oversized_packet_is_rejected_without_consuming_budget() {
        run_test(async {
            let (queue, _receiver) = channel(8, 2, MAX_SEND_AGE);
            assert_eq!(queue.try_send(vec![0; 9]), Err(SendError::ByteLimit));
            assert_eq!(queue.budget.available_permits(), 8);
        });
    }

    #[test]
    fn spare_allocation_capacity_counts_towards_byte_budget() {
        run_test(async {
            let (queue, _receiver) = channel(8, 2, MAX_SEND_AGE);
            assert_eq!(
                queue.try_send(Vec::with_capacity(9)),
                Err(SendError::ByteLimit)
            );
            let mut packet = Vec::with_capacity(8);
            packet.push(1);
            queue.try_send(packet).unwrap();
            assert_eq!(queue.budget.available_permits(), 0);
        });
    }

    #[test]
    fn blocked_sink_times_out_and_releases_entire_queue() {
        run_test(async {
            let (queue, receiver) = channel(8, 4, MAX_SEND_AGE);
            queue.try_send(vec![0; 4]).unwrap();
            queue.try_send(vec![0; 4]).unwrap();
            let mut task =
                SenderTask::new(tokio::spawn(receiver.run(|_| pending::<Result<(), ()>>())));
            yield_now().await;
            advance(MAX_SEND_AGE).await;
            assert_eq!(task.wait().await, Err(SendError::Deadline));
            assert_eq!(queue.budget.available_permits(), 8);
            assert_eq!(queue.try_send(vec![0]), Err(SendError::Closed));
            task.stop().await; // Completion was consumed; do not poll it twice.
        });
    }

    #[test]
    fn slow_sink_uses_original_enqueue_deadline_for_next_packet() {
        run_test(async {
            let (queue, receiver) = channel(8, 4, MAX_SEND_AGE);
            queue.try_send(vec![0; 4]).unwrap();
            queue.try_send(vec![0; 4]).unwrap();
            let task = tokio::spawn(receiver.run(|_| async {
                sleep(Duration::from_millis(1500)).await;
                Ok::<_, ()>(())
            }));
            yield_now().await;
            advance(Duration::from_millis(1500)).await;
            yield_now().await;
            assert_eq!(queue.budget.available_permits(), 4);
            advance(Duration::from_millis(500)).await;
            assert_eq!(task.await.unwrap(), Err(SendError::Deadline));
            assert_eq!(queue.budget.available_permits(), 8);
        });
    }

    #[test]
    fn expired_packet_never_reaches_even_a_ready_sink() {
        run_test(async {
            let (queue, receiver) = channel(8, 4, MAX_SEND_AGE);
            queue.try_send(vec![0]).unwrap();
            advance(MAX_SEND_AGE).await;
            let called = Cell::new(false);
            let result = receiver
                .run(|_| {
                    called.set(true);
                    std::future::ready(Ok::<_, ()>(()))
                })
                .await;
            assert_eq!(result, Err(SendError::Deadline));
            assert!(!called.get());
            assert_eq!(queue.budget.available_permits(), 8);
        });
    }

    #[test]
    fn sink_completion_at_deadline_still_fails_viewer() {
        run_test(async {
            let (queue, receiver) = channel(8, 4, MAX_SEND_AGE);
            queue.try_send(vec![0]).unwrap();
            let task = tokio::spawn(receiver.run(|_| async {
                sleep(MAX_SEND_AGE).await;
                Ok::<_, ()>(())
            }));
            yield_now().await;
            advance(MAX_SEND_AGE).await;
            assert_eq!(task.await.unwrap(), Err(SendError::Deadline));
            assert_eq!(queue.budget.available_permits(), 8);
        });
    }

    #[test]
    fn sink_failure_wakes_owner_and_discards_pending_packets() {
        run_test(async {
            let (queue, receiver) = channel(8, 4, MAX_SEND_AGE);
            queue.try_send(vec![0; 4]).unwrap();
            queue.try_send(vec![0; 4]).unwrap();
            let mut task =
                SenderTask::new(tokio::spawn(receiver.run(|_| async { Err::<(), _>(()) })));
            assert_eq!(task.wait().await, Err(SendError::Sink));
            assert_eq!(queue.budget.available_permits(), 8);
            assert_eq!(queue.try_send(vec![0]), Err(SendError::Closed));
        });
    }

    #[test]
    fn healthy_sink_preserves_packet_order_and_releases_budget() {
        run_test(async {
            let (queue, receiver) = channel(8, 4, MAX_SEND_AGE);
            queue.try_send(vec![1]).unwrap();
            queue.try_send(vec![2]).unwrap();
            let seen = Rc::new(Cell::new(0));
            let capture = seen.clone();
            drop(queue);
            receiver
                .run(|data| {
                    capture.set(capture.get() * 10 + data[0]);
                    std::future::ready(Ok::<_, ()>(()))
                })
                .await
                .unwrap();
            assert_eq!(seen.get(), 12);
        });
    }

    #[test]
    fn dropping_owner_cancels_blocked_sender_instead_of_detaching_it() {
        run_test(async {
            let (queue, receiver) = channel(8, 4, MAX_SEND_AGE);
            queue.try_send(vec![0; 4]).unwrap();
            let task = SenderTask::new(tokio::spawn(receiver.run(|_| pending::<Result<(), ()>>())));
            yield_now().await;
            drop(task);
            yield_now().await;
            assert_eq!(queue.budget.available_permits(), 8);
            assert!(queue.sender.is_closed());
        });
    }

    enum MixedPacket {
        NonVideo(Vec<u8>),
        Video(Vec<u8>),
    }

    impl Payload for MixedPacket {
        fn retained_bytes(&self) -> usize {
            match self {
                Self::NonVideo(bytes) | Self::Video(bytes) => bytes.capacity(),
            }
        }
    }

    #[test]
    fn stalled_non_video_send_uses_the_video_frames_original_deadline() {
        run_test(async {
            let (queue, receiver) = channel(16, 4, MAX_SEND_AGE);
            queue.try_send(MixedPacket::NonVideo(vec![0; 4])).unwrap();
            queue.try_send(MixedPacket::Video(vec![1; 4])).unwrap();
            let video_started = Arc::new(AtomicBool::new(false));
            let saw_video = video_started.clone();
            let task = tokio::spawn(receiver.run(move |packet| {
                let saw_video = saw_video.clone();
                async move {
                    match packet {
                        MixedPacket::NonVideo(_) => {
                            sleep(Duration::from_millis(1900)).await;
                        }
                        MixedPacket::Video(_) => {
                            saw_video.store(true, Ordering::SeqCst);
                            sleep(Duration::from_millis(200)).await;
                        }
                    }
                    Ok::<_, ()>(())
                }
            }));
            yield_now().await;
            advance(Duration::from_millis(1900)).await;
            yield_now().await;
            assert!(video_started.load(Ordering::SeqCst));
            advance(Duration::from_millis(100)).await;
            assert_eq!(task.await.unwrap(), Err(SendError::Deadline));
            assert_eq!(queue.budget.available_permits(), 16);
        });
    }
}
