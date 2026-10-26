use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

/// Liveness: the process is up.
pub async fn live(State(s): State<AppState>) -> Json<Value> {
    Json(json!({ "status": "ok", "liveConnections": s.hub.connections() }))
}

/// Readiness: every dependency answers. 503 lists the failing ones.
pub async fn ready(State(s): State<AppState>) -> (StatusCode, Json<Value>) {
    let mut checks = serde_json::Map::new();
    let mut ok = true;
    for c in s.ready.iter() {
        let r = c.check().await;
        ok &= r.is_ok();
        checks.insert(c.name().into(), json!(r.err().unwrap_or_else(|| "ok".into())));
    }
    (if ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE }, Json(json!({ "status": if ok { "ok" } else { "degraded" }, "checks": checks })))
}
