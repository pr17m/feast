//! Application errors. Every failure crossing a port or reaching a client is an
//! `AppError` with a stable machine-readable `code` and a human message.
//! The HTTP layer maps `kind` to a status code; gRPC maps it to a status.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 400 — bad input.
    Validation,
    /// 401 — not signed in / bad credentials.
    Unauthenticated,
    /// 403 — signed in but not allowed.
    Forbidden,
    /// 404
    NotFound,
    /// 409 — state conflict (slot taken, invalid transition, already paid…).
    Conflict,
    /// 503 — a dependency is down.
    Unavailable,
    /// 500
    Internal,
}

#[derive(Debug, Clone)]
pub struct AppError {
    pub kind: ErrorKind,
    pub code: String,
    pub message: String,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(kind: ErrorKind, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { kind, code: code.into(), message: message.into() }
    }
    pub fn validation(code: &str, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Validation, code, message)
    }
    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unauthenticated, "UNAUTHENTICATED", message)
    }
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Forbidden, "FORBIDDEN", message)
    }
    pub fn not_found(what: &str) -> Self {
        Self::new(ErrorKind::NotFound, "NOT_FOUND", format!("{what} not found"))
    }
    pub fn conflict(code: &str, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Conflict, code, message)
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unavailable, "UNAVAILABLE", message)
    }
    /// Wrap an infrastructure failure. The detail is logged, not shown to clients.
    pub fn internal(err: impl fmt::Display) -> Self {
        tracing::error!(error = %err, "internal error");
        Self::new(ErrorKind::Internal, "INTERNAL", "Something went wrong. Please try again.")
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}
