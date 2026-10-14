//! In-memory sliding-window rate limiting and client IP resolution.
//!
//! State lives in process memory: a restart forgets it, which is acceptable
//! for a single-instance self-hosted app (YAGNI on a shared store).

use axum::{extract::FromRequestParts, http::request::Parts};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Above this many tracked keys, stale keys are swept on the next check, so
/// memory stays bounded under a flood of distinct addresses.
const SWEEP_THRESHOLD: usize = 10_000;

#[derive(Default)]
pub struct RateLimiter {
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    /// Record a hit for `key` and return whether it stays within `limit`
    /// hits per `window`. A refused hit is not recorded, so a blocked caller
    /// recovers as soon as old hits age out.
    pub fn allow(&self, key: &str, limit: usize, window: Duration) -> bool {
        self.allow_at(key, limit, window, Instant::now())
    }

    fn allow_at(&self, key: &str, limit: usize, window: Duration, now: Instant) -> bool {
        let mut hits = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        if hits.len() > SWEEP_THRESHOLD {
            hits.retain(|_, v| v.iter().any(|t| now.duration_since(*t) < window));
        }
        let entry = hits.entry(key.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < window);
        if entry.len() >= limit {
            return false;
        }
        entry.push(now);
        true
    }
}

/// The requesting client's IP as a string, for rate-limit keys.
///
/// `X-Forwarded-For` is trusted only when `MANGAREAD_TRUST_PROXY=1`;
/// otherwise any client could forge it to dodge limits.
pub struct ClientIp(pub String);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if std::env::var("MANGAREAD_TRUST_PROXY").as_deref() == Ok("1")
            && let Some(first) = parts
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(str::trim)
                .filter(|v| !v.is_empty())
        {
            return Ok(ClientIp(first.to_string()));
        }
        let ip = parts
            .extensions
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        Ok(ClientIp(ip))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_limit_then_recovers_after_window() {
        let rl = RateLimiter::default();
        let w = Duration::from_secs(3600);
        let t0 = Instant::now();
        assert!(rl.allow_at("k", 2, w, t0));
        assert!(rl.allow_at("k", 2, w, t0));
        assert!(!rl.allow_at("k", 2, w, t0));
        assert!(rl.allow_at("other", 2, w, t0), "keys are independent");
        assert!(rl.allow_at("k", 2, w, t0 + w), "old hits age out");
    }
}
