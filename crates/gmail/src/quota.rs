//! Per-account token bucket. Starts at 250 quota units per second (the
//! documented Gmail per-user limit) and adapts: a quota error halves the rate,
//! then it creeps back up. Some projects get a lower "units per minute per
//! user" limit than documented.

use std::time::{Duration, Instant};

use tokio::sync::Mutex;

/// Documented Gmail quota unit costs.
pub mod cost {
    pub const GET_PROFILE: u32 = 1;
    pub const LABELS_LIST: u32 = 1;
    pub const HISTORY_LIST: u32 = 2;
    pub const MESSAGES_LIST: u32 = 5;
    pub const MESSAGES_GET: u32 = 5;
    pub const ATTACHMENTS_GET: u32 = 5;
    pub const THREADS_MODIFY: u32 = 10;
    pub const THREADS_TRASH: u32 = 10;
    pub const THREADS_UNTRASH: u32 = 10;
    pub const MESSAGES_SEND: u32 = 100;
}

const UNITS_PER_SECOND: f64 = 250.0;
/// Floor of the adaptive rate.
const MIN_RATE: f64 = 10.0;
/// Rate regained per second without a quota error (units/s per s).
const RECOVERY: f64 = 0.5;

struct State {
    tokens: f64,
    at: Instant,
    rate: f64,
    /// Last `slow_down`; errors within a second count once (one batch).
    slowed: Option<Instant>,
}

pub struct Quota {
    max: f64,
    state: Mutex<State>,
}

impl Default for Quota {
    fn default() -> Self {
        Self::new(UNITS_PER_SECOND)
    }
}

impl Quota {
    pub fn new(units_per_second: f64) -> Self {
        Self {
            max: units_per_second,
            state: Mutex::new(State {
                tokens: units_per_second,
                at: Instant::now(),
                rate: units_per_second,
                slowed: None,
            }),
        }
    }

    /// Wait until `units` can be spent. Costs above one second of budget are clamped.
    pub async fn acquire(&self, units: u32) {
        loop {
            let wait = {
                let mut s = self.state.lock().await;
                let now = Instant::now();
                let dt = now.duration_since(s.at).as_secs_f64();
                s.rate = (s.rate + dt * RECOVERY).min(self.max);
                s.tokens = (s.tokens + dt * s.rate).min(s.rate);
                s.at = now;
                let need = f64::from(units).min(s.rate);
                if s.tokens >= need {
                    s.tokens -= need;
                    return;
                }
                Duration::from_secs_f64((need - s.tokens) / s.rate)
            };
            tokio::time::sleep(wait).await;
        }
    }

    /// Gmail said the quota is used up: halve the rate and empty the bucket.
    pub async fn slow_down(&self) {
        let mut s = self.state.lock().await;
        let now = Instant::now();
        if s.slowed.is_some_and(|t| now.duration_since(t) < Duration::from_secs(1)) {
            return;
        }
        s.slowed = Some(now);
        s.rate = (s.rate / 2.0).max(MIN_RATE);
        s.tokens = 0.0;
        s.at = now;
    }

    #[cfg(test)]
    async fn rate(&self) -> f64 {
        self.state.lock().await.rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bucket_throttles_to_rate() {
        let q = Quota::new(100.0);
        let start = Instant::now();
        // 100 free (full bucket) + 50 more at 100/s = about 0.5 s.
        for _ in 0..15 {
            q.acquire(10).await;
        }
        let t = start.elapsed();
        assert!(t >= Duration::from_millis(400), "{t:?}");
        assert!(t < Duration::from_millis(900), "{t:?}");
    }

    #[tokio::test]
    async fn slow_down_halves_once_per_burst_and_has_a_floor() {
        let q = Quota::new(100.0);
        q.slow_down().await;
        q.slow_down().await; // same batch: ignored
        assert!((q.rate().await - 50.0).abs() < 0.1);
        let q = Quota::new(12.0);
        q.slow_down().await;
        assert!((q.rate().await - MIN_RATE).abs() < 0.1);
    }
}
