//! Per-client rate limiting for abuse-prone endpoints (OTP, sign-in, signup).
//!
//! Fixed one-minute windows per (route, client IP). The counters live in the
//! shared database (`SharedStateRepository`), so the limit holds across every
//! gateway instance behind a load balancer. If the counter store is down the
//! request is let through (logged): rate limiting must never lock everyone out.

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use feast_application::ports::SharedStateRepository;
use serde_json::json;

use crate::AppState;

const WINDOW_MS: i64 = 60_000;

pub struct RateLimiter {
    per_minute: u32,
    store: Arc<dyn SharedStateRepository>,
}

impl RateLimiter {
    pub fn new(per_minute: u32, store: Arc<dyn SharedStateRepository>) -> Self {
        Self { per_minute, store }
    }

    /// `Ok` or the seconds until the window resets.
    async fn check_at(&self, route: &str, ip: Option<IpAddr>, now_ms: i64) -> Result<(), u64> {
        if self.per_minute == 0 {
            return Ok(());
        }
        let window = now_ms.div_euclid(WINDOW_MS);
        let key = format!("{route}|{}", ip.map(|i| i.to_string()).unwrap_or_default());
        let hits = match self.store.rate_hit(&key, window, (window + 2) * WINDOW_MS).await {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(error = %e.message, "rate limit store unavailable; letting the request through");
                return Ok(());
            }
        };
        if hits > i64::from(self.per_minute) {
            Err((((window + 1) * WINDOW_MS - now_ms) / 1000).max(1) as u64)
        } else {
            Ok(())
        }
    }

    async fn check(&self, route: &str, ip: Option<IpAddr>) -> Result<(), u64> {
        self.check_at(route, ip, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)).await
    }
}

/// The client's IP. Behind `proxy_hops` trusted proxies, it is the entry that
/// many places from the right of `X-Forwarded-For` (each proxy appends what it
/// saw; anything further left was sent by the client and can be forged).
/// `proxy_hops = 0`: don't trust the header, use the TCP peer.
pub(crate) fn client_ip_from(headers: &axum::http::HeaderMap, peer: Option<IpAddr>, proxy_hops: usize) -> Option<IpAddr> {
    if proxy_hops == 0 {
        return peer;
    }
    let xff: Vec<&str> =
        headers.get_all("x-forwarded-for").iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(',')).map(str::trim).filter(|s| !s.is_empty()).collect();
    if xff.is_empty() {
        return peer;
    }
    let i = xff.len().saturating_sub(proxy_hops);
    xff[i].parse().ok().or(peer)
}

pub(crate) fn client_ip(req: &Request, proxy_hops: usize) -> Option<IpAddr> {
    let peer = req.extensions().get::<axum::extract::ConnectInfo<std::net::SocketAddr>>().map(|c| c.0.ip());
    client_ip_from(req.headers(), peer, proxy_hops)
}

pub async fn limit(State(s): State<AppState>, req: Request, next: Next) -> Response {
    let route = req.extensions().get::<MatchedPath>().map(|p| p.as_str().to_owned()).unwrap_or_default();
    match s.limiter.check(&route, client_ip(&req, s.proxy_hops)).await {
        Ok(()) => next.run(req).await,
        Err(retry) => {
            let mut res =
                (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "code": "RATE_LIMITED", "message": "Too many attempts. Please wait a minute and try again." })))
                    .into_response();
            if let Ok(v) = HeaderValue::from_str(&retry.to_string()) {
                res.headers_mut().insert("retry-after", v);
            }
            res
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two "instances" sharing one counter store, like gateways behind a load balancer.
    #[derive(Default)]
    struct Shared(std::sync::Mutex<std::collections::HashMap<(String, i64), i64>>);

    #[async_trait::async_trait]
    impl SharedStateRepository for Shared {
        async fn rate_hit(&self, key: &str, window: i64, _: i64) -> feast_application::AppResult<i64> {
            let mut m = self.0.lock().unwrap();
            let e = m.entry((key.to_owned(), window)).or_insert(0);
            *e += 1;
            Ok(*e)
        }
        async fn otp_put(&self, _: &str, _: &str, _: i64) -> feast_application::AppResult<()> {
            Ok(())
        }
        async fn otp_check(&self, _: &str, _: &str, _: i64, _: i64) -> feast_application::AppResult<bool> {
            Ok(false)
        }
        async fn lease(&self, _: &str, _: &str, _: i64, _: i64) -> feast_application::AppResult<bool> {
            Ok(true)
        }
        async fn purge_expired(&self, _: i64) -> feast_application::AppResult<u64> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn limits_per_route_and_ip_across_instances() {
        let store: Arc<dyn SharedStateRepository> = Arc::new(Shared::default());
        let (a, b) = (RateLimiter::new(2, store.clone()), RateLimiter::new(2, store));
        let ip = Some("1.2.3.4".parse().unwrap());
        let t = 1_800_000_010_000; // 10 s into a minute
        assert!(a.check_at("/v1/auth/otp", ip, t).await.is_ok());
        assert!(b.check_at("/v1/auth/otp", ip, t).await.is_ok(), "second hit, on the other instance");
        assert_eq!(a.check_at("/v1/auth/otp", ip, t).await, Err(50), "the limit is shared, not per instance");
        assert!(a.check_at("/v1/auth/login", ip, t).await.is_ok());
        assert!(a.check_at("/v1/auth/otp", Some("5.6.7.8".parse().unwrap()), t).await.is_ok());
        assert!(b.check_at("/v1/auth/otp", ip, t + 60_000).await.is_ok(), "a new minute starts afresh");
    }

    #[test]
    fn client_ip_counts_trusted_hops_from_the_right() {
        let mut h = axum::http::HeaderMap::new();
        h.insert("x-forwarded-for", "6.6.6.6, 1.2.3.4".parse().unwrap()); // forged, then what the LB saw
        let peer = Some("10.0.0.9".parse().unwrap());
        assert_eq!(client_ip_from(&h, peer, 1), Some("1.2.3.4".parse().unwrap()));
        assert_eq!(client_ip_from(&h, peer, 2), Some("6.6.6.6".parse().unwrap()));
        assert_eq!(client_ip_from(&h, peer, 0), peer);
        assert_eq!(client_ip_from(&axum::http::HeaderMap::new(), peer, 1), peer);
    }
}
