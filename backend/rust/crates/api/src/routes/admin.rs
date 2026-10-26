//! Admin console (ADMIN only, enforced in the use cases).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use feast_domain::model::*;
use serde::Deserialize;

use crate::error::ApiResult;
use crate::extract::{Body, Caller};
use crate::AppState;

pub async fn metrics(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<AdminMetrics>> {
    Ok(Json(s.feast.admin_metrics(&u).await?))
}

pub async fn restaurants(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<RestaurantDetail>>> {
    Ok(Json(s.feast.admin_restaurants(&u).await?))
}

pub async fn create_restaurant(
    State(s): State<AppState>,
    Caller(u): Caller,
    Body(r): Body<CreateRestaurantRequest>,
) -> ApiResult<(StatusCode, Json<RestaurantDetail>)> {
    Ok((StatusCode::CREATED, Json(s.feast.create_restaurant(&u, r).await?)))
}

pub async fn update_restaurant(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(p): Body<RestaurantPatch>,
) -> ApiResult<Json<RestaurantDetail>> {
    Ok(Json(s.feast.update_restaurant(&u, &id, p).await?))
}

#[derive(Deserialize)]
pub struct StatusReq {
    status: RestaurantStatus,
}

pub async fn restaurant_status(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<StatusReq>,
) -> ApiResult<Json<RestaurantDetail>> {
    Ok(Json(s.feast.set_restaurant_status(&u, &id, r.status).await?))
}

pub async fn users(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<User>>> {
    Ok(Json(s.feast.admin_users(&u).await?))
}

pub async fn create_user(State(s): State<AppState>, Caller(u): Caller, Body(r): Body<CreateUserRequest>) -> ApiResult<(StatusCode, Json<User>)> {
    Ok((StatusCode::CREATED, Json(s.feast.create_user(&u, r).await?)))
}

pub async fn update_user(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Body(p): Body<UserPatch>) -> ApiResult<Json<User>> {
    Ok(Json(s.feast.update_user(&u, &id, p).await?))
}

pub async fn bookings(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<Booking>>> {
    Ok(Json(s.feast.admin_bookings(&u).await?))
}

pub async fn payments(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<BillPayment>>> {
    Ok(Json(s.feast.admin_payments(&u).await?))
}

pub async fn reset_demo(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<StatusCode> {
    s.feast.reset_demo_data(&u).await?;
    Ok(StatusCode::NO_CONTENT)
}
