//! Counter (offline) payments and the audit trail.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use feast_application::audit_trail::AuditFilter;
use feast_domain::audit::AuditPage;
use feast_domain::model::{OfflinePayment, OfflinePaymentStatus, Paise, PaymentMethod};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Body, Caller, Q};
use crate::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitReq {
    method: PaymentMethod,
}

/// `POST /v1/bills/{id}/offline-payment` — the diner pays this bill at the counter.
pub async fn submit(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<SubmitReq>,
) -> ApiResult<(StatusCode, Json<OfflinePayment>)> {
    Ok((StatusCode::CREATED, Json(s.feast.submit_offline_payment(&u, &id, r.method).await?)))
}

/// `GET /v1/bills/{id}/offline-payment` — its status (null if none).
pub async fn for_bill(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Option<OfflinePayment>>> {
    Ok(Json(s.feast.offline_payment_for_bill(&u, &id).await?))
}

#[derive(Deserialize)]
pub struct ListQuery {
    status: Option<String>,
}

/// `GET /v1/partner/restaurants/{id}/offline-payments?status=PENDING_APPROVAL`
pub async fn restaurant_list(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Q(q): Q<ListQuery>) -> ApiResult<Json<Vec<OfflinePayment>>> {
    let status = match q.status.as_deref().filter(|s| !s.is_empty()) {
        Some(v) => Some(OfflinePaymentStatus::parse(v).ok_or_else(|| ApiError(feast_application::AppError::validation("INVALID_STATUS", "Unknown status")))?),
        None => None,
    };
    Ok(Json(s.feast.restaurant_offline_payments(&u, &id, status).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveReq {
    amount_received: Option<Paise>,
}

/// `POST /v1/partner/offline-payments/{id}/approve`
pub async fn approve(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Body(r): Body<ApproveReq>) -> ApiResult<Json<OfflinePayment>> {
    Ok(Json(s.feast.approve_offline_payment(&u, &id, r.amount_received).await?))
}

#[derive(Deserialize)]
pub struct RejectReq {
    reason: String,
}

/// `POST /v1/partner/offline-payments/{id}/reject`
pub async fn reject(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Body(r): Body<RejectReq>) -> ApiResult<Json<OfflinePayment>> {
    Ok(Json(s.feast.reject_offline_payment(&u, &id, &r.reason).await?))
}

/// `GET /v1/admin/audit?restaurantId&entityType&entityId&actorId&userId&from&to&cursor&limit`
pub async fn admin_audit(State(s): State<AppState>, Caller(u): Caller, Q(f): Q<AuditFilter>) -> ApiResult<Json<AuditPage>> {
    Ok(Json(s.feast.admin_audit(&u, f).await?))
}

/// `GET /v1/partner/restaurants/{id}/audit?entityType&entityId&from&to&cursor&limit`
pub async fn restaurant_audit(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Q(f): Q<AuditFilter>) -> ApiResult<Json<AuditPage>> {
    Ok(Json(s.feast.restaurant_audit(&u, &id, f).await?))
}

/// `GET /v1/bookings/{id}/history`
pub async fn booking_history(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<AuditPage>> {
    Ok(Json(s.feast.booking_history(&u, &id).await?))
}

/// `GET /v1/bill-payments/{id}/history`
pub async fn bill_payment_history(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<AuditPage>> {
    Ok(Json(s.feast.bill_payment_history(&u, &id).await?))
}
