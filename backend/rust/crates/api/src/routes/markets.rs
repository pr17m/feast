//! Country (market) config: public read, geolocation, admin edits.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use feast_domain::model::*;

use crate::error::ApiResult;
use crate::extract::{Body, Caller, Geo, MaybeCaller};
use crate::AppState;

pub async fn cities(State(s): State<AppState>, MaybeCaller(u): MaybeCaller) -> ApiResult<Json<Vec<City>>> {
    Ok(Json(s.feast.list_cities(u.as_ref()).await?))
}

pub async fn markets(State(s): State<AppState>, MaybeCaller(u): MaybeCaller) -> ApiResult<Json<Vec<Market>>> {
    Ok(Json(s.feast.list_markets(u.as_ref()).await?))
}

/// Visitor's country (CDN header / GeoIP) and, if this deployment doesn't
/// serve it, where the right one is.
pub async fn geo(State(s): State<AppState>, Geo(req): Geo) -> ApiResult<Json<GeoResult>> {
    Ok(Json(s.feast.locate(&req).await?))
}

pub async fn payment_providers(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<PaymentProviderInfo>>> {
    Ok(Json(s.feast.payment_providers(&u)?))
}

pub async fn save_market(State(s): State<AppState>, Caller(u): Caller, Path(cc): Path<String>, Body(m): Body<Market>) -> ApiResult<Json<Market>> {
    Ok(Json(s.feast.save_market(&u, &cc, m).await?))
}

pub async fn create_city(State(s): State<AppState>, Caller(u): Caller, Body(r): Body<CreateCityRequest>) -> ApiResult<(StatusCode, Json<City>)> {
    Ok((StatusCode::CREATED, Json(s.feast.create_city(&u, r).await?)))
}
