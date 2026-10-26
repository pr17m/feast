//! Per-client rate limiting for abuse-prone endpoints (OTP, sign-in, signup).
//!
//! Fixed one-minute windows per (route, client IP), kept in memory on each
//! gateway instance. Behind a load balancer the effective limit is
//! `limit × instances`; put a shared limiter (Redis, or the CDN/WAF) in front
//! if that matters.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::AppState;

const WINDOW: Duration = Duration::from_secs(60);

/// (route key, client IP) → (window start, hits in the window).
type Hits = HashMap<(String, Option<IpAddr>), (Instant, u32)>;

pub struct RateLimiter {
    per_minute: u32,
    hits: Mutex<Hits>,
}

impl RateLimiter {
    pub fn new(per_minute: u32) -> Self {
        Self { per_minute, hits: Mutex::new(HashMap::new()) }
    }

    /// `Ok` or the seconds until the window resets.
    fn check(&self, route: &str, ip: Option<IpAddr>) -> Result<(), u64> {
        if self.per_minute == 0 {
            return Ok(());
        }
        let now = Instant::now();
        let Ok(mut hits) = self.hits.lock() else { return Ok(()) };
        if hits.len() > 50_000 {
            hits.retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
        }
        let e = hits.entry((route.to_owned(), ip)).or_insert((now, 0));
        if now.duration_since(e.0) >= WINDOW {
            *e = (now, 0);
        }
        e.1 += 1;
        if e.1 > self.per_minute {
            Err(WINDOW.saturating_sub(now.duration_since(e.0)).as_secs().max(1))
        } else {
            Ok(())
        }
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
    match s.limiter.check(&route, client_ip(&req, s.proxy_hops)) {
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

    #[test]
    fn limits_per_route_and_ip() {
        let l = RateLimiter::new(2);
        let ip = Some("1.2.3.4".parse().unwrap());
        assert!(l.check("/v1/auth/otp", ip).is_ok());
        assert!(l.check("/v1/auth/otp", ip).is_ok());
        assert!(l.check("/v1/auth/otp", ip).is_err());
        assert!(l.check("/v1/auth/login", ip).is_ok());
        assert!(l.check("/v1/auth/otp", Some("5.6.7.8".parse().unwrap())).is_ok());
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
