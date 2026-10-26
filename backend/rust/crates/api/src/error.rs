//! Error → HTTP response mapping. Body is always `{ "code", "message" }`,
//! which the web/mobile clients turn into typed errors.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use feast_application::{AppError, ErrorKind};
use serde_json::json;

#[derive(Debug)]
pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        Self(e)
    }
}

pub fn status_of(kind: ErrorKind) -> StatusCode {
    match kind {
        ErrorKind::Validation => StatusCode::BAD_REQUEST,
        ErrorKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        ErrorKind::Forbidden => StatusCode::FORBIDDEN,
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::Conflict => StatusCode::CONFLICT,
        ErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl ApiError {
    pub fn bad_request(code: &str, message: impl Into<String>) -> Self {
        Self(AppError::validation(code, message))
    }

    pub fn into_response_raw(self) -> Response {
        self.into_response()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = status_of(self.0.kind);
        // Internal details (SQL errors, provider responses…) are logged, never sent.
        let message = if self.0.kind == ErrorKind::Internal {
            tracing::error!(code = %self.0.code, "internal error: {}", self.0.message);
            "Something went wrong. Please try again.".to_owned()
        } else {
            if status.is_server_error() {
                tracing::warn!(code = %self.0.code, "request failed: {}", self.0.message);
            }
            self.0.message
        };
        (status, Json(json!({ "code": self.0.code, "message": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
