//! Reconnecting to services that can restart (PipeWire, logind, the session
//! bus for MPRIS): wait 1 s, doubling up to 30 s, between attempts.

use std::time::{Duration, Instant};

const FIRST: Duration = Duration::from_secs(1);
const MAX: Duration = Duration::from_secs(30);
/// a connection that lasted this long was a success: start again from 1 s
const STABLE: Duration = Duration::from_secs(30);

pub struct Backoff {
    what: &'static str,
    delay: Duration,
    failures: u32,
    started: Instant,
}

impl Backoff {
    pub fn new(what: &'static str) -> Self {
        Self {
            what,
            delay: FIRST,
            failures: 0,
            started: Instant::now(),
        }
    }

    /// Call when an attempt starts.
    pub fn attempt(&mut self) {
        self.started = Instant::now();
    }

    /// Call when the connection is gone or failed, with the reason; waits
    /// before the next attempt. Only the first failure in a row is logged as
    /// a warning, so a missing service doesn't fill the log.
    pub async fn failed(&mut self, reason: impl std::fmt::Display) {
        if self.started.elapsed() >= STABLE {
            self.delay = FIRST;
            self.failures = 0;
        }
        self.failures += 1;
        if self.failures == 1 {
            log::warn!("{} unavailable: {reason}; retrying", self.what);
        } else {
            log::debug!("{} still unavailable: {reason}", self.what);
        }
        tokio::time::sleep(self.delay).await;
        self.delay = (self.delay * 2).min(MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn delays_double_up_to_the_maximum() {
        let mut b = Backoff::new("test");
        let mut delays = vec![];
        for _ in 0..7 {
            b.attempt();
            let before = tokio::time::Instant::now();
            b.failed("down").await;
            delays.push(before.elapsed().as_secs());
        }
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 30, 30]);
    }
}
