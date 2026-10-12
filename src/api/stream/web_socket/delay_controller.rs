use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use bytes::Bytes;
use tokio::time::Instant;
use tracing::{debug, trace, warn};

const LATENCY_WINDOW: Duration = Duration::from_secs(10);
const PING_TIMEOUT: Duration = Duration::from_secs(5);

// Thresholds for the queue delay: the round trip above the lowest round trip in the window.
// Comparing against the lowest round trip keeps links with a high base latency from looking congested.
const CONGESTED_THRESHOLD: Duration = Duration::from_millis(100);
const SEVERE_THRESHOLD: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub enum DelayState {
    Normal,
    Congested,
    Severe,
}

#[derive(Debug, Default)]
pub struct DelayController {
    current_ping: u64,
    /// marks the start of the pings
    pings: HashMap<u64, Instant>,
    /// recent rtts
    rtts: VecDeque<(Instant, Duration)>,
}

impl DelayController {
    pub fn on_ping_send(&mut self, time: Instant) -> Bytes {
        let current_ping = self.current_ping;
        self.pings.insert(current_ping, time);
        trace!(id = %current_ping, "sending ping with id");

        self.current_ping = self.current_ping.wrapping_add(1);

        Bytes::copy_from_slice(&current_ping.to_le_bytes())
    }
    pub fn on_pong_receive(&mut self, bytes: &[u8], time: Instant) {
        if bytes.len() < size_of::<u64>() {
            return;
        }

        let id = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        let Some(ping_send) = self.pings.remove(&id) else {
            debug!(id = %id, "received pong packet with an invalid ping id");
            return;
        };

        let Some(elapsed) = time.checked_duration_since(ping_send) else {
            warn!("received pong timestamp precedes ping timestamp");
            return;
        };
        trace!(%id, send = ?ping_send, elapsed = ?elapsed, "received ping");

        self.rtts.push_back((time, elapsed));
    }

    pub fn update(&mut self, now: Instant) {
        // Drop pings that have timed out.
        self.pings.retain(|_, sent| {
            let elapsed = now.checked_duration_since(*sent);
            let retain = elapsed.is_some_and(|elapsed| elapsed <= PING_TIMEOUT);

            if !retain {
                debug!(now = ?now, ping_sent = ?*sent, elapsed = ?elapsed, timeout = ?PING_TIMEOUT, "dropping ping because it took too long to receive");
            }

            retain
        });

        // Keep only latency samples within the rolling window.
        while self.rtts.front().is_some_and(|&(received, _)| {
            now.checked_duration_since(received)
                .is_some_and(|age| age > LATENCY_WINDOW)
        }) {
            self.rtts.pop_front();
        }
    }

    pub fn delay_state(&self, now: Instant) -> DelayState {
        let queue_delay = self.queue_delay(now);

        if queue_delay >= SEVERE_THRESHOLD {
            DelayState::Severe
        } else if queue_delay >= CONGESTED_THRESHOLD {
            DelayState::Congested
        } else {
            DelayState::Normal
        }
    }

    /// How much longer than the lowest round trip in the window the current round trip is.
    /// This is the time data waits in queues on the way to the client.
    pub fn queue_delay(&self, now: Instant) -> Duration {
        let base = self.base_rtt().unwrap_or_default();

        // A ping that has not been answered yet is at least as late as it is old,
        // so a growing backlog is noticed before its pong arrives.
        self.latest_rtt()
            .max(self.oldest_unanswered(now))
            .saturating_sub(base)
    }

    /// The client stopped answering pings: the oldest unanswered ping is well past the latest round trip.
    pub fn is_stalled(&self, now: Instant) -> bool {
        self.oldest_unanswered(now) > self.latest_rtt() + CONGESTED_THRESHOLD
    }

    fn oldest_unanswered(&self, now: Instant) -> Duration {
        self.pings
            .values()
            .filter_map(|sent| now.checked_duration_since(*sent))
            .max()
            .unwrap_or_default()
    }

    fn latest_rtt(&self) -> Duration {
        self.rtts.back().map(|(_, rtt)| *rtt).unwrap_or_default()
    }

    fn base_rtt(&self) -> Option<Duration> {
        self.rtts.iter().map(|(_, rtt)| *rtt).min()
    }

    /// Takes the latest round trip as the new base round trip. Used when the round trip stays high
    /// although nothing is queued anymore, which means the latency of the link itself changed.
    pub fn rebase(&mut self) {
        let len = self.rtts.len();
        self.rtts.drain(..len.saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(controller: &mut DelayController, sent: Instant, rtt_ms: u64) -> Instant {
        let id = controller.on_ping_send(sent);
        let received = sent + Duration::from_millis(rtt_ms);
        controller.on_pong_receive(&id, received);
        received
    }

    #[test]
    fn queue_delay_is_relative_to_lowest_rtt() {
        let mut controller = DelayController::default();
        let start = Instant::now();

        // A high but steady latency link is not congested.
        let now = sample(&mut controller, start, 220);
        assert!(matches!(controller.delay_state(now), DelayState::Normal));

        // A round trip 300ms above the base is.
        let now = sample(&mut controller, now, 520);
        assert!(matches!(controller.delay_state(now), DelayState::Congested));

        // After a rebase the latest round trip is the new base.
        controller.rebase();
        assert!(matches!(controller.delay_state(now), DelayState::Normal));
    }

    #[test]
    fn unanswered_ping_counts_as_delay() {
        let mut controller = DelayController::default();
        let start = Instant::now();
        let now = sample(&mut controller, start, 20);

        controller.on_ping_send(now);
        assert!(matches!(
            controller.delay_state(now + Duration::from_millis(600)),
            DelayState::Severe
        ));
        assert!(controller.is_stalled(now + Duration::from_millis(600)));
        assert!(!controller.is_stalled(now + Duration::from_millis(50)));
    }
}
