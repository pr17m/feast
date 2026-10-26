//! Restaurant onboarding (signup, documents, review) and platform
//! subscriptions — partner and admin sides.

use std::collections::BTreeMap;

use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use feast_application::compliance::{DocumentUpload, Review};
use feast_domain::model::*;
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Body, Caller};
use crate::AppState;

// ------------------------------------------------------------ partner

pub async fn signup(State(s): State<AppState>, Body(r): Body<PartnerSignupRequest>) -> ApiResult<(StatusCode, Json<AuthSession>)> {
    Ok((StatusCode::CREATED, Json(s.feast.partner_signup(r).await?)))
}

pub async fn compliance(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<ComplianceStatus>> {
    Ok(Json(s.feast.compliance(&u, &id).await?))
}

/// `multipart/form-data`: `requirementId`, `file`, optional `number`, `expiresOn`.
pub async fn upload(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, mut form: Multipart) -> ApiResult<Json<ComplianceStatus>> {
    let bad = |m: String| ApiError::bad_request("BAD_REQUEST", m);
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut file: Option<(String, String, Vec<u8>)> = None;
    while let Some(field) = form.next_field().await.map_err(|e| bad(e.body_text()))? {
        let name = field.name().unwrap_or_default().to_owned();
        if name == "file" {
            let file_name = field.file_name().unwrap_or("document").to_owned();
            let ct = field.content_type().unwrap_or("application/octet-stream").to_owned();
            let bytes = field.bytes().await.map_err(|e| bad(e.body_text()))?;
            file = Some((file_name, ct, bytes.to_vec()));
        } else {
            fields.insert(name, field.text().await.map_err(|e| bad(e.body_text()))?);
        }
    }
    let (file_name, content_type, bytes) = file.ok_or_else(|| bad("Attach the document file".into()))?;
    let up = DocumentUpload {
        requirement_id: fields.remove("requirementId").ok_or_else(|| bad("requirementId is required".into()))?,
        file_name,
        content_type,
        bytes,
        number: fields.remove("number"),
        expires_on: fields.remove("expiresOn"),
    };
    Ok(Json(s.feast.upload_document(&u, &id, up).await?))
}

pub async fn submit(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<ComplianceStatus>> {
    Ok(Json(s.feast.submit_for_review(&u, &id).await?))
}

/// The document file, for the restaurant's managers and admins.
pub async fn document_file(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Response> {
    let f = s.feast.document_file(&u, &id).await?;
    let name: String = f.file_name.chars().filter(|c| c.is_ascii_alphanumeric() || "._- ".contains(*c)).collect();
    let ct = HeaderValue::from_str(&f.content_type).unwrap_or(HeaderValue::from_static("application/octet-stream"));
    let disposition = HeaderValue::from_str(&format!("inline; filename=\"{name}\"")).unwrap_or(HeaderValue::from_static("inline"));
    Ok((
        [
            (header::CONTENT_TYPE, ct),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CACHE_CONTROL, HeaderValue::from_static("private, no-store")),
            (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox")),
        ],
        f.bytes,
    )
        .into_response())
}

pub async fn subscription(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<SubscriptionOverview>> {
    Ok(Json(s.feast.partner_subscription(&u, &id).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutReq {
    plan_id: PlanId,
}

/// Start paying for a plan; complete with `POST /v1/payments/intents/{id}/confirm`.
pub async fn subscription_checkout(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<CheckoutReq>,
) -> ApiResult<(StatusCode, Json<PaymentIntent>)> {
    Ok((StatusCode::CREATED, Json(s.feast.subscription_checkout(&u, &id, r.plan_id).await?)))
}

// ------------------------------------------------------------ admin

pub async fn approvals(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<ApprovalQueueItem>>> {
    Ok(Json(s.feast.approval_queue(&u).await?))
}

#[derive(Deserialize)]
pub struct ReviewReq {
    decision: DocumentStatus,
    note: Option<String>,
}

pub async fn review_document(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<ReviewReq>,
) -> ApiResult<Json<ComplianceStatus>> {
    let decision = match r.decision {
        DocumentStatus::Approved => Review::Approve,
        DocumentStatus::Rejected => Review::Reject,
        DocumentStatus::Submitted => return Err(ApiError::bad_request("INVALID_DECISION", "Use APPROVED or REJECTED")),
    };
    Ok(Json(s.feast.review_document(&u, &id, decision, r.note).await?))
}

#[derive(Deserialize)]
pub struct NoteReq {
    note: String,
}

pub async fn request_changes(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<NoteReq>,
) -> ApiResult<Json<ComplianceStatus>> {
    Ok(Json(s.feast.request_changes(&u, &id, &r.note).await?))
}

pub async fn subscriptions(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<AdminSubscriptionRow>>> {
    Ok(Json(s.feast.admin_subscriptions(&u).await?))
}

pub async fn platform_payments(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<PlatformPayment>>> {
    Ok(Json(s.feast.platform_payments(&u).await?))
}

#[derive(Deserialize)]
pub struct ExtendReq {
    days: i64,
    note: Option<String>,
}

pub async fn extend(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Body(r): Body<ExtendReq>) -> ApiResult<Json<PaySubscriptionResult>> {
    Ok(Json(s.feast.extend_subscription(&u, &id, r.days, r.note).await?))
}
