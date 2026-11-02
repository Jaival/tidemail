//! Per-account token bucket: 250 quota units per second (Gmail per-user limit).

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
}

const UNITS_PER_SECOND: f64 = 250.0;

pub struct Quota {
    rate: f64,
    state: Mutex<(f64, Instant)>,
}

impl Default for Quota {
    fn default() -> Self {
        Self::new(UNITS_PER_SECOND)
    }
}

impl Quota {
    pub fn new(units_per_second: f64) -> Self {
        Self {
            rate: units_per_second,
            state: Mutex::new((units_per_second, Instant::now())),
        }
    }

    /// Wait until `units` can be spent. Costs above one second of budget are clamped.
    pub async fn acquire(&self, units: u32) {
        let need = f64::from(units).min(self.rate);
        loop {
            let wait = {
                let mut s = self.state.lock().await;
                let now = Instant::now();
                s.0 = (s.0 + now.duration_since(s.1).as_secs_f64() * self.rate).min(self.rate);
                s.1 = now;
                if s.0 >= need {
                    s.0 -= need;
                    return;
                }
                Duration::from_secs_f64((need - s.0) / self.rate)
            };
            tokio::time::sleep(wait).await;
        }
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
}
