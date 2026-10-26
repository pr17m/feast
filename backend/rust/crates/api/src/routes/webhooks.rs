//! Payment provider webhooks: `POST /v1/webhooks/payments/{razorpay|stripe}`.
//! No sign-in — authenticity comes from the provider's signature, checked by
//! its adapter over the *raw* body (so this handler must not parse/re-encode it).

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::error::ApiResult;
use crate::AppState;

pub async fn payment_webhook(State(s): State<AppState>, Path(provider): Path<String>, headers: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    let headers: Vec<(String, String)> = headers.iter().filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_lowercase(), v.to_owned()))).collect();
    let ack = s.feast.payment_webhook(&provider.to_lowercase(), &headers, &body).await?;
    // 2xx for every authentic event (even ignored ones) so the provider stops retrying.
    Ok(Json(json!({ "received": true, "result": ack })))
}
