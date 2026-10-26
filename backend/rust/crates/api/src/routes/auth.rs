use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use feast_application::AppError;
use feast_domain::model::{AuthProvider, AuthProviderInfo, AuthSession, OAuthLoginRequest};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiResult;
use crate::extract::Body;
use crate::AppState;

#[derive(Deserialize)]
pub struct OtpReq {
    phone: String,
}

pub async fn request_otp(State(s): State<AppState>, Body(r): Body<OtpReq>) -> ApiResult<Json<Value>> {
    Ok(Json(match s.feast.request_otp(&r.phone).await? {
        Some(hint) => json!({ "hint": hint }),
        None => json!({}),
    }))
}

#[derive(Deserialize)]
pub struct VerifyReq {
    phone: String,
    otp: String,
    name: Option<String>,
}

pub async fn verify_otp(State(s): State<AppState>, Body(r): Body<VerifyReq>) -> ApiResult<Json<AuthSession>> {
    Ok(Json(s.feast.verify_otp(&r.phone, &r.otp, r.name.as_deref()).await?))
}

#[derive(Deserialize)]
pub struct LoginReq {
    email: String,
    password: String,
}

pub async fn login(State(s): State<AppState>, Body(r): Body<LoginReq>) -> ApiResult<Json<AuthSession>> {
    Ok(Json(s.feast.login(&r.email, &r.password).await?))
}

/// Tokens are stateless JWTs: the client drops its token. (Add a deny-list in
/// Redis here if you need server-side revocation.)
pub async fn logout() -> StatusCode {
    StatusCode::NO_CONTENT
}

pub async fn providers(State(s): State<AppState>) -> Json<Vec<AuthProviderInfo>> {
    Json(s.feast.auth_providers())
}

pub async fn oauth(State(s): State<AppState>, Path(provider): Path<String>, Body(r): Body<OAuthLoginRequest>) -> ApiResult<Json<AuthSession>> {
    let p = AuthProvider::parse(&provider).ok_or_else(|| AppError::not_found("Sign-in provider"))?;
    Ok(Json(s.feast.oauth_login(p, &r).await?))
}
