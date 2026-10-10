use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use bytes::Bytes;
use tokio::time::Instant;
use tracing::{debug, trace, warn};

const LATENCY_WINDOW: Duration = Duration::from_secs(10);
const PING_TIMEOUT: Duration = Duration::from_secs(5);

const CONGESTED_THRESHOLD: Duration = Duration::from_millis(150);
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

    pub fn delay_state(&self) -> DelayState {
        let Some(average_delay) = self.average_delay() else {
            // No values present -> too much congestion
            return DelayState::Severe;
        };

        if average_delay >= SEVERE_THRESHOLD {
            DelayState::Severe
        } else if average_delay >= CONGESTED_THRESHOLD {
            DelayState::Congested
        } else {
            DelayState::Normal
        }
    }
    pub fn average_delay(&self) -> Option<Duration> {
        if self.rtts.is_empty() {
            return None;
        }

        let average_delay_secs = self
            .rtts
            .iter()
            .map(|(_, rtt)| rtt.as_secs_f64())
            .sum::<f64>()
            / self.rtts.len() as f64;

        Some(Duration::from_secs_f64(average_delay_secs))
    }
}
