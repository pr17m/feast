//! Insights screens: owner scorecard, diner savings, admin marketplace health.
//! Access rules are enforced in the use cases.

use axum::extract::{Path, State};
use axum::Json;
use feast_domain::insights::{AdminInsights, DinerInsights, OwnerInsights};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::extract::{Caller, Q};
use crate::AppState;

#[derive(Deserialize)]
pub struct PeriodQ {
    days: Option<i64>,
}

/// `GET /v1/partner/restaurants/{id}/insights?days=7|30|90` (default 30).
pub async fn owner(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Q(q): Q<PeriodQ>) -> ApiResult<Json<OwnerInsights>> {
    Ok(Json(s.feast.owner_insights(&u, &id, q.days.unwrap_or(30)).await?))
}

/// `GET /v1/me/insights?days=30|90|365|0` (default 0 = all time).
pub async fn diner(State(s): State<AppState>, Caller(u): Caller, Q(q): Q<PeriodQ>) -> ApiResult<Json<DinerInsights>> {
    Ok(Json(s.feast.diner_insights(&u, q.days.unwrap_or(0)).await?))
}

/// `GET /v1/admin/insights?days=7|30|90` (default 30).
pub async fn admin(State(s): State<AppState>, Caller(u): Caller, Q(q): Q<PeriodQ>) -> ApiResult<Json<AdminInsights>> {
    Ok(Json(s.feast.admin_insights(&u, q.days.unwrap_or(30)).await?))
}
