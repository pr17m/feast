//! Dish photos: owners upload/remove/find a photo per menu item; anyone can
//! load uploaded photos from `/v1/media/…` (public, cacheable, images only).

use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::Json;
use feast_application::media::PhotoUpload;
use feast_domain::model::MenuItem;

use crate::error::{ApiError, ApiResult};
use crate::extract::Caller;
use crate::AppState;

pub async fn upload_menu_photo(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path((id, item)): Path<(String, String)>,
    mut form: Multipart,
) -> ApiResult<Json<MenuItem>> {
    let bad = |m: String| ApiError::bad_request("BAD_REQUEST", m);
    let mut bytes: Option<Vec<u8>> = None;
    while let Some(field) = form.next_field().await.map_err(|e| bad(e.body_text()))? {
        if field.name() == Some("file") {
            bytes = Some(field.bytes().await.map_err(|e| bad(e.body_text()))?.to_vec());
        }
    }
    let bytes = bytes.ok_or_else(|| bad("Attach a photo".into()))?;
    Ok(Json(s.feast.upload_menu_photo(&u, &id, &item, PhotoUpload { bytes }).await?))
}

pub async fn remove_menu_photo(State(s): State<AppState>, Caller(u): Caller, Path((id, item)): Path<(String, String)>) -> ApiResult<Json<MenuItem>> {
    Ok(Json(s.feast.remove_menu_photo(&u, &id, &item).await?))
}

pub async fn find_menu_photo(State(s): State<AppState>, Caller(u): Caller, Path((id, item)): Path<(String, String)>) -> ApiResult<Json<MenuItem>> {
    Ok(Json(s.feast.find_menu_photo(&u, &id, &item).await?))
}

/// Public. Keys are content-addressed by a new id per upload, so they never
/// change: cache for a year.
pub async fn media_file(State(s): State<AppState>, Path(path): Path<String>) -> ApiResult<Response> {
    let f = s.feast.media_file(&path).await?;
    let ct = HeaderValue::from_str(&f.content_type).unwrap_or(HeaderValue::from_static("application/octet-stream"));
    Ok((
        [
            (header::CONTENT_TYPE, ct),
            (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=31536000, immutable")),
            (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
            (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox")),
        ],
        f.bytes,
    )
        .into_response())
}
