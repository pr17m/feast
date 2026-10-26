//! Extractors: authenticated caller, and JSON/query parsing with our error shape.

use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::request::Parts;
use axum::Json;
use feast_domain::model::User;
use serde::de::DeserializeOwned;

use crate::error::ApiError;
use crate::AppState;

/// The signed-in user (401 if missing/invalid).
pub struct Caller(pub User);

fn bearer(parts: &Parts) -> Option<&str> {
    parts.headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ").map(str::trim).filter(|t| !t.is_empty())
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = bearer(parts).ok_or_else(|| ApiError(feast_application::AppError::unauthenticated("Please sign in")))?;
        Ok(Caller(state.feast.authenticate(token).await?))
    }
}

/// The signed-in user if there is a valid token, else `None` (public endpoints
/// that show more to admins).
pub struct MaybeCaller(pub Option<User>);

impl FromRequestParts<AppState> for MaybeCaller {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        match bearer(parts) {
            Some(t) => Ok(MaybeCaller(state.feast.authenticate(t).await.ok())),
            None => Ok(MaybeCaller(None)),
        }
    }
}

/// What geolocation may use: headers (CDN country headers) and the client IP
/// (see `ratelimit::client_ip_from`).
pub struct Geo(pub feast_application::ports::GeoRequest);

impl FromRequestParts<AppState> for Geo {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let headers: Vec<(String, String)> =
            parts.headers.iter().filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_lowercase(), v.to_owned()))).collect();
        let peer = parts.extensions.get::<axum::extract::ConnectInfo<std::net::SocketAddr>>().map(|c| c.0.ip());
        let ip = crate::ratelimit::client_ip_from(&parts.headers, peer, state.proxy_hops);
        Ok(Geo(feast_application::ports::GeoRequest { ip, headers }))
    }
}

/// JSON body with `{code, message}` errors instead of axum's plain-text rejections.
pub struct Body<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for Body<T> {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(v)) => Ok(Body(v)),
            Err(e) => Err(ApiError::bad_request("BAD_REQUEST", e.body_text())),
        }
    }
}

/// Query string with `{code, message}` errors.
pub struct Q<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequestParts<S> for Q<T> {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(v)) => Ok(Q(v)),
            Err(e) => Err(ApiError::bad_request("BAD_REQUEST", e.body_text())),
        }
    }
}

/// Optional JSON body: an empty body yields `T::default()`.
pub struct OptBody<T>(pub T);

impl<T: DeserializeOwned + Default, S: Send + Sync> FromRequest<S> for OptBody<T> {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::Bytes::from_request(req, state).await.map_err(|e| ApiError::bad_request("BAD_REQUEST", e.body_text()))?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(OptBody(T::default()));
        }
        serde_json::from_slice(&bytes).map(OptBody).map_err(|e| ApiError::bad_request("BAD_REQUEST", format!("Invalid JSON body: {e}")))
    }
}
