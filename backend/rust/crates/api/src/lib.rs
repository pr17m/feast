//! The HTTP gateway: REST/JSON for the web and mobile apps, plus a
//! Server-Sent Events stream for live updates.
//!
//! This crate is a thin delivery layer: it authenticates, parses, calls a use
//! case on [`feast_application::Feast`] and serialises the result. It holds no
//! business rules, so a Go (or any other) gateway can implement the same
//! contract (`docs/openapi.yaml`) against the same backend design.

mod error;
mod extract;
pub mod metrics;
pub mod ratelimit;
pub mod realtime;
mod routes;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::Router;
use feast_application::Feast;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

pub use error::ApiError;
pub use realtime::RealtimeHub;

/// A dependency the readiness probe checks (database, bus, notification service…).
#[async_trait]
pub trait ReadinessCheck: Send + Sync {
    fn name(&self) -> &'static str;
    async fn check(&self) -> Result<(), String>;
}

#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// Allowed browser origins (`*` allows any; fine for dev, not prod).
    pub cors_origins: Vec<String>,
    pub request_timeout: Duration,
    pub max_body_bytes: usize,
    /// Document uploads (multipart) may be larger than JSON bodies.
    pub max_upload_bytes: usize,
}

pub use metrics::Metrics;
pub use ratelimit::RateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub feast: Arc<Feast>,
    pub hub: Arc<RealtimeHub>,
    pub ready: Arc<Vec<Arc<dyn ReadinessCheck>>>,
    /// Trusted proxies in front of the API (load balancer, the web app's /v1
    /// proxy…): how many `X-Forwarded-For` entries from the right to skip. 0 = none.
    pub proxy_hops: usize,
    pub metrics: Arc<Metrics>,
    /// Bearer token required for `/metrics` (None = open; keep it internal).
    pub metrics_token: Option<String>,
    /// Limits sign-in/OTP/signup attempts per client.
    pub limiter: Arc<RateLimiter>,
}

static REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Baseline security headers for an API (no HTML is served).
fn security_headers() -> tower::layer::util::Stack<
    tower_http::set_header::SetResponseHeaderLayer<HeaderValue>,
    tower::layer::util::Stack<tower_http::set_header::SetResponseHeaderLayer<HeaderValue>, tower_http::set_header::SetResponseHeaderLayer<HeaderValue>>,
> {
    use tower_http::set_header::SetResponseHeaderLayer as H;
    tower::layer::util::Stack::new(
        H::if_not_present(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        tower::layer::util::Stack::new(
            H::if_not_present(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY")),
            H::if_not_present(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer")),
        ),
    )
}

/// Every request runs as an audit "actor" (request id + client IP; the user
/// once they authenticate), so events it emits record who caused them.
async fn actor_scope(
    axum::extract::State(s): axum::extract::State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let ip = ratelimit::client_ip(&req, s.proxy_hops).map(|i| i.to_string());
    let request_id = req.headers().get(&REQUEST_ID).and_then(|v| v.to_str().ok()).map(str::to_owned);
    feast_application::actor::scope(feast_application::actor::request(ip, request_id), next.run(req)).await
}

/// Build the full router with middleware.
pub fn router(state: AppState, cfg: &ApiConfig) -> Router {
    let cors = {
        let origins = if cfg.cors_origins.iter().any(|o| o == "*") {
            AllowOrigin::any()
        } else {
            AllowOrigin::list(cfg.cors_origins.iter().filter_map(|o| HeaderValue::from_str(o).ok()))
        };
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE, Method::OPTIONS])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, REQUEST_ID.clone()])
            .expose_headers([REQUEST_ID.clone()])
            .max_age(Duration::from_secs(3600))
    };

    // Request/response routes get a timeout; the SSE stream must not.
    let timeout = TimeoutLayer::with_status_code(axum::http::StatusCode::GATEWAY_TIMEOUT, cfg.request_timeout);
    let limited = routes::limited().route_layer(axum::middleware::from_fn_with_state(state.clone(), ratelimit::limit));
    let api = routes::api().merge(limited).layer(RequestBodyLimitLayer::new(cfg.max_body_bytes)).layer(timeout);
    let uploads = routes::uploads()
        .layer(axum::extract::DefaultBodyLimit::max(cfg.max_upload_bytes + 64 * 1024))
        .layer(RequestBodyLimitLayer::new(cfg.max_upload_bytes + 64 * 1024))
        .layer(TimeoutLayer::with_status_code(axum::http::StatusCode::GATEWAY_TIMEOUT, cfg.request_timeout * 4));
    let streams = routes::streams();

    Router::new()
        .merge(api)
        .merge(uploads)
        .merge(streams)
        .merge(routes::health())
        .route("/metrics", axum::routing::get(metrics::endpoint))
        .fallback(|| async { ApiError::from(feast_application::AppError::not_found("Route")) })
        .layer(axum::middleware::from_fn_with_state(state.clone(), metrics::track))
        .layer(axum::middleware::from_fn_with_state(state.clone(), actor_scope))
        .with_state(state)
        .layer(security_headers())
        .layer(cors)
        .layer(PropagateRequestIdLayer::new(REQUEST_ID.clone()))
        .layer(TraceLayer::new_for_http().make_span_with(|req: &axum::http::Request<_>| {
            let id = req.headers().get(&REQUEST_ID).and_then(|v| v.to_str().ok()).unwrap_or("-");
            tracing::info_span!("http", method = %req.method(), path = %req.uri().path(), request_id = %id)
        }))
        .layer(SetRequestIdLayer::new(REQUEST_ID.clone(), MakeRequestUuid))
        .layer(CatchPanicLayer::custom(|_| {
            tracing::error!("handler panicked");
            ApiError::from(feast_application::AppError::new(feast_application::ErrorKind::Internal, "INTERNAL", "Something went wrong. Please try again."))
                .into_response_raw()
        }))
}
