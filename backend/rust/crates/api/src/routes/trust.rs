//! Trust & safety: grievance cases, reviews, content moderation and support
//! access. Who may do what is enforced in the use cases (`application::trust`).

use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use feast_application::trust::CaseUpload;
use feast_domain::model::*;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Body, Caller};
use crate::AppState;

// ------------------------------------------------------------ cases (diner, restaurant, Feast)

pub async fn open_case(State(s): State<AppState>, Caller(u): Caller, Body(r): Body<OpenCaseRequest>) -> ApiResult<(StatusCode, Json<Case>)> {
    Ok((StatusCode::CREATED, Json(s.feast.open_case(&u, r).await?)))
}

pub async fn my_cases(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<Case>>> {
    Ok(Json(s.feast.my_cases(&u).await?))
}

pub async fn restaurant_cases(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Vec<Case>>> {
    Ok(Json(s.feast.restaurant_cases(&u, &id).await?))
}

pub async fn get_case(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Case>> {
    Ok(Json(s.feast.get_case(&u, &id).await?))
}

pub async fn case_message(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Body(r): Body<CaseMessageRequest>) -> ApiResult<Json<Case>> {
    Ok(Json(s.feast.case_message(&u, &id, r).await?))
}

pub async fn escalate_case(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Case>> {
    Ok(Json(s.feast.escalate_case(&u, &id).await?))
}

pub async fn settle_case(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Case>> {
    Ok(Json(s.feast.settle_case(&u, &id).await?))
}

/// `multipart/form-data` with one `file` (JPG, PNG or WebP photo).
pub async fn case_attachment_upload(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, mut form: Multipart) -> ApiResult<Json<Case>> {
    let bad = |m: String| ApiError::bad_request("BAD_REQUEST", m);
    let mut up: Option<CaseUpload> = None;
    while let Some(field) = form.next_field().await.map_err(|e| bad(e.body_text()))? {
        if field.name() == Some("file") {
            let file_name = field.file_name().unwrap_or("photo").to_owned();
            let bytes = field.bytes().await.map_err(|e| bad(e.body_text()))?;
            up = Some(CaseUpload { file_name, bytes: bytes.to_vec() });
        }
    }
    let up = up.ok_or_else(|| bad("Attach a photo".into()))?;
    Ok(Json(s.feast.add_case_attachment(&u, &id, up).await?))
}

pub async fn case_attachment_file(State(s): State<AppState>, Caller(u): Caller, Path((id, attachment_id)): Path<(String, String)>) -> ApiResult<Response> {
    let f = s.feast.case_attachment(&u, &id, &attachment_id).await?;
    let ct = HeaderValue::from_str(&f.content_type).unwrap_or(HeaderValue::from_static("application/octet-stream"));
    Ok((
        [
            (header::CONTENT_TYPE, ct),
            (header::CACHE_CONTROL, HeaderValue::from_static("private, no-store")),
            (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox")),
        ],
        f.bytes,
    )
        .into_response())
}

pub async fn admin_cases(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<AdminCaseItem>>> {
    Ok(Json(s.feast.admin_cases(&u).await?))
}

pub async fn resolve_case(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Body(r): Body<ResolveCaseRequest>) -> ApiResult<Json<Case>> {
    Ok(Json(s.feast.resolve_case(&u, &id, r).await?))
}

// ------------------------------------------------------------ reviews

pub async fn restaurant_reviews(State(s): State<AppState>, Path(slug): Path<String>) -> ApiResult<Json<ReviewList>> {
    Ok(Json(s.feast.restaurant_reviews(&slug).await?))
}

pub async fn create_review(State(s): State<AppState>, Caller(u): Caller, Body(r): Body<CreateReviewRequest>) -> ApiResult<(StatusCode, Json<Review>)> {
    Ok((StatusCode::CREATED, Json(s.feast.create_review(&u, r).await?)))
}

pub async fn my_reviews(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<Review>>> {
    Ok(Json(s.feast.my_reviews(&u).await?))
}

pub async fn partner_reviews(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<ReviewList>> {
    Ok(Json(s.feast.partner_reviews(&u, &id).await?))
}

pub async fn reply_review(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Body(r): Body<ReviewReplyRequest>) -> ApiResult<Json<Review>> {
    Ok(Json(s.feast.reply_review(&u, &id, r).await?))
}

pub async fn report_review(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<ReportReviewRequest>,
) -> ApiResult<Json<Review>> {
    Ok(Json(s.feast.report_review(&u, &id, r).await?))
}

pub async fn admin_reviews(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<Review>>> {
    Ok(Json(s.feast.admin_reviews(&u).await?))
}

pub async fn moderate_review(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<ModerateReviewRequest>,
) -> ApiResult<Json<Review>> {
    Ok(Json(s.feast.moderate_review(&u, &id, r).await?))
}

// ------------------------------------------------------------ moderation & support access

pub async fn moderate_content(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<ModerateContentRequest>,
) -> ApiResult<StatusCode> {
    s.feast.moderate_content(&u, &id, r).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn moderation_queue(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<ModeratedContent>>> {
    Ok(Json(s.feast.moderation_queue(&u).await?))
}

pub async fn appeal_moderation(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<ModerationAppealRequest>,
) -> ApiResult<StatusCode> {
    s.feast.appeal_moderation(&u, &id, r).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn support_access(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<SupportAccess>> {
    Ok(Json(s.feast.support_access(&u, &id).await?))
}

pub async fn set_support_access(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<SupportAccessRequest>,
) -> ApiResult<Json<SupportAccess>> {
    Ok(Json(s.feast.set_support_access(&u, &id, r.hours).await?))
}
