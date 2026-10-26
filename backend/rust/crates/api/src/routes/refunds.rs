//! Refunds of online payments: who can see them, and admin refunds by hand.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use feast_domain::refunds::{Refund, RefundRequest};

use crate::error::ApiResult;
use crate::extract::{Body, Caller};
use crate::AppState;

/// `POST /v1/admin/refunds` — refund (part of) an online payment; never more than what is left.
pub async fn admin_create(State(s): State<AppState>, Caller(u): Caller, Body(r): Body<RefundRequest>) -> ApiResult<(StatusCode, Json<Refund>)> {
    Ok((StatusCode::CREATED, Json(s.feast.admin_refund(&u, r).await?)))
}

/// `GET /v1/payments/intents/{id}/refunds` — the payer, the restaurant's staff or an admin.
pub async fn for_intent(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Vec<Refund>>> {
    Ok(Json(s.feast.payment_refunds(&u, &id).await?))
}

/// `GET /v1/admin/refunds`
pub async fn admin_list(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<Refund>>> {
    Ok(Json(s.feast.admin_refunds(&u).await?))
}
