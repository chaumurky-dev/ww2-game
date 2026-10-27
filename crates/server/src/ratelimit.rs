//! Small in-memory sliding-window limiter for per-user / per-action limits. (Per-IP limiting of
//! the credential endpoints uses `tower_governor`, see `routes::api_router`.)
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use std::net::{IpAddr, SocketAddr};

use axum::extract::ConnectInfo;
use axum::http::Request;
use tower_governor::key_extractor::KeyExtractor;
use tower_governor::GovernorError;

use crate::error::{AppError, AppResult};

/// Client-IP key extractor for the per-IP limiter. `hops == 0`: the TCP peer address. `hops == n`:
/// the n-th entry from the RIGHT of `X-Forwarded-For` (each trusted proxy appends the address it
/// saw, so the rightmost entries are the trustworthy ones), falling back to the peer address when
/// the header is missing, too short or unparsable. `X-Real-IP` and `Forwarded` are ignored.
#[derive(Debug, Clone, Copy)]
pub struct TrustedProxyKey {
    pub hops: u8,
}

impl TrustedProxyKey {
    fn forwarded_ip(&self, headers: &axum::http::HeaderMap) -> Option<IpAddr> {
        if self.hops == 0 {
            return None;
        }
        // Several header lines are equivalent to one comma-joined line.
        let mut entries: Vec<&str> = Vec::new();
        for v in headers.get_all("x-forwarded-for") {
            entries.extend(v.to_str().ok()?.split(','));
        }
        let idx = entries.len().checked_sub(self.hops as usize)?;
        entries[idx].trim().parse().ok()
    }
}

impl KeyExtractor for TrustedProxyKey {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        if let Some(ip) = self.forwarded_ip(req.headers()) {
            return Ok(ip);
        }
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip())
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

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

    /// Fail with `rate_limited` when `key` already has `limit` recorded hits in `window`, without
    /// recording a new one (pair with [`RateLimiter::record`], e.g. to count only failures).
    pub fn ensure_below(&self, key: &str, limit: usize, window: Duration) -> AppResult<()> {
        if !self.enabled {
            return Ok(());
        }
        let now = Instant::now();
        let mut map = self.hits.lock().unwrap();
        let Some(q) = map.get_mut(key) else {
            return Ok(());
        };
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
        Ok(())
    }

    /// Record one hit for `key` unconditionally.
    pub fn record(&self, key: &str) {
        if !self.enabled {
            return;
        }
        let mut map = self.hits.lock().unwrap();
        map.entry(key.to_string())
            .or_default()
            .push_back(Instant::now());
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

    fn req(xff: Option<&str>, peer: Option<&str>) -> Request<()> {
        let mut b = Request::builder();
        if let Some(x) = xff {
            b = b
                .header("x-forwarded-for", x)
                .header("x-real-ip", "7.7.7.7");
        }
        let mut r = b.body(()).unwrap();
        if let Some(p) = peer {
            r.extensions_mut().insert(ConnectInfo(
                format!("{p}:1234").parse::<SocketAddr>().unwrap(),
            ));
        }
        r
    }

    #[test]
    fn trusted_proxy_key_uses_rightmost_entries() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        let k0 = TrustedProxyKey { hops: 0 };
        let k1 = TrustedProxyKey { hops: 1 };
        let k2 = TrustedProxyKey { hops: 2 };
        // hops 0: header ignored entirely
        assert_eq!(
            k0.extract(&req(Some("1.1.1.1"), Some("10.0.0.1"))).unwrap(),
            ip("10.0.0.1")
        );
        assert!(k0.extract(&req(Some("1.1.1.1"), None)).is_err());
        // hops 1: last entry, never the (spoofable) first one
        assert_eq!(
            k1.extract(&req(Some("6.6.6.6, 9.9.9.9"), Some("10.0.0.1")))
                .unwrap(),
            ip("9.9.9.9")
        );
        // hops 2: second from the right
        assert_eq!(
            k2.extract(&req(Some("6.6.6.6, 8.8.8.8, 9.9.9.9"), None))
                .unwrap(),
            ip("8.8.8.8")
        );
        // too few entries / garbage: fall back to the peer
        assert_eq!(
            k2.extract(&req(Some("9.9.9.9"), Some("10.0.0.1"))).unwrap(),
            ip("10.0.0.1")
        );
        assert_eq!(
            k1.extract(&req(Some("not-an-ip"), Some("10.0.0.1")))
                .unwrap(),
            ip("10.0.0.1")
        );
    }

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
