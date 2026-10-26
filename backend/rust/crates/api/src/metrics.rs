//! Prometheus metrics (`GET /metrics`), dependency-free: request counts and
//! latency histograms per route template, plus live SSE connections.
//! Protect the endpoint with `http.metrics_token` (or keep it off the public LB).

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Mutex;
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;

const BUCKETS: [f64; 11] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];

#[derive(Default)]
struct Series {
    count: u64,
    sum: f64,
    buckets: [u64; BUCKETS.len()],
}

#[derive(Default)]
pub struct Metrics {
    /// (method, route, status) → count
    requests: Mutex<BTreeMap<(String, String, u16), u64>>,
    /// (method, route) → latency histogram
    latency: Mutex<BTreeMap<(String, String), Series>>,
}

impl Metrics {
    fn observe(&self, method: &str, route: &str, status: u16, secs: f64) {
        if let Ok(mut m) = self.requests.lock() {
            *m.entry((method.into(), route.into(), status)).or_default() += 1;
        }
        if let Ok(mut m) = self.latency.lock() {
            let s = m.entry((method.into(), route.into())).or_default();
            s.count += 1;
            s.sum += secs;
            for (i, b) in BUCKETS.iter().enumerate() {
                if secs <= *b {
                    s.buckets[i] += 1;
                }
            }
        }
    }

    fn render(&self, connections: usize) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# HELP feast_http_requests_total HTTP requests by route template and status.");
        let _ = writeln!(out, "# TYPE feast_http_requests_total counter");
        if let Ok(m) = self.requests.lock() {
            for ((method, route, status), n) in m.iter() {
                let _ = writeln!(out, "feast_http_requests_total{{method=\"{method}\",route=\"{route}\",status=\"{status}\"}} {n}");
            }
        }
        let _ = writeln!(out, "# HELP feast_http_request_duration_seconds Request latency.");
        let _ = writeln!(out, "# TYPE feast_http_request_duration_seconds histogram");
        if let Ok(m) = self.latency.lock() {
            for ((method, route), s) in m.iter() {
                let labels = format!("method=\"{method}\",route=\"{route}\"");
                for (i, b) in BUCKETS.iter().enumerate() {
                    let _ = writeln!(out, "feast_http_request_duration_seconds_bucket{{{labels},le=\"{b}\"}} {}", s.buckets[i]);
                }
                let _ = writeln!(out, "feast_http_request_duration_seconds_bucket{{{labels},le=\"+Inf\"}} {}", s.count);
                let _ = writeln!(out, "feast_http_request_duration_seconds_sum{{{labels}}} {}", s.sum);
                let _ = writeln!(out, "feast_http_request_duration_seconds_count{{{labels}}} {}", s.count);
            }
        }
        let _ = writeln!(out, "# HELP feast_live_connections Open Server-Sent Event streams on this instance.");
        let _ = writeln!(out, "# TYPE feast_live_connections gauge");
        let _ = writeln!(out, "feast_live_connections {connections}");
        out
    }
}

/// Middleware: time every request, labelled by its route template (not the raw path).
pub async fn track(State(s): State<AppState>, req: Request, next: Next) -> Response {
    let route = req.extensions().get::<MatchedPath>().map(|p| p.as_str().to_owned()).unwrap_or_else(|| "unmatched".into());
    let method = req.method().as_str().to_owned();
    let start = Instant::now();
    let res = next.run(req).await;
    if route != "/v1/me/events" {
        s.metrics.observe(&method, &route, res.status().as_u16(), start.elapsed().as_secs_f64());
    }
    res
}

pub async fn endpoint(State(s): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = s.metrics_token.as_deref() {
        let ok = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).is_some_and(|t| t == token);
        if !ok {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], s.metrics.render(s.hub.connections())).into_response()
}
