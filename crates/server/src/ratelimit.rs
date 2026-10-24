//! Small in-memory sliding-window limiter for per-user / per-action limits. (Per-IP limiting of
//! the credential endpoints uses `tower_governor`, see `routes::api_router`.)
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct RateLimiter {
    enabled: bool,
    hits: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
}

impl RateLimiter {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            hits: Arc::default(),
        }
    }

    /// Record one hit for `key` and fail with `rate_limited` (429) when more than `limit` hits
    /// happened within `window`. Rejected requests are not counted.
    pub fn check(&self, key: &str, limit: usize, window: Duration) -> AppResult<()> {
        self.check_at(key, limit, window, Instant::now())
    }

    fn check_at(&self, key: &str, limit: usize, window: Duration, now: Instant) -> AppResult<()> {
        if !self.enabled {
            return Ok(());
        }
        let mut map = self.hits.lock().unwrap();
        if map.len() > 10_000 {
            map.retain(|_, q| q.back().is_some_and(|t| now.duration_since(*t) < window));
        }
        let q = map.entry(key.to_string()).or_default();
        while q.front().is_some_and(|t| now.duration_since(*t) >= window) {
            q.pop_front();
        }
        if q.len() >= limit {
            let wait = q
                .front()
                .map(|t| window.saturating_sub(now.duration_since(*t)))
                .unwrap_or(window);
            return Err(AppError::TooManyRequests(wait.as_secs().max(1)));
        }
        q.push_back(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliding_window() {
        let l = RateLimiter::new(true);
        let t0 = Instant::now();
        let w = Duration::from_secs(60);
        for _ in 0..3 {
            l.check_at("a", 3, w, t0).unwrap();
        }
        assert!(matches!(
            l.check_at("a", 3, w, t0),
            Err(AppError::TooManyRequests(_))
        ));
        l.check_at("b", 3, w, t0).unwrap();
        l.check_at("a", 3, w, t0 + Duration::from_secs(61)).unwrap();
    }

    #[test]
    fn disabled_never_limits() {
        let l = RateLimiter::new(false);
        for _ in 0..100 {
            l.check("a", 1, Duration::from_secs(60)).unwrap();
        }
    }
}
