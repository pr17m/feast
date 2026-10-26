//! Partner portal: OWNER of the restaurant, or ADMIN (enforced in the use cases).

use axum::extract::{Path, State};
use axum::Json;
use feast_domain::model::*;
use serde::Deserialize;

use crate::error::ApiResult;
use crate::extract::{Body, Caller, Q};
use crate::AppState;

pub async fn restaurants(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<RestaurantDetail>>> {
    Ok(Json(s.feast.partner_restaurants(&u).await?))
}

pub async fn dashboard(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<PartnerDashboard>> {
    Ok(Json(s.feast.partner_dashboard(&u, &id).await?))
}

#[derive(Deserialize)]
pub struct DateQ {
    date: String,
}

pub async fn bookings(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Q(q): Q<DateQ>) -> ApiResult<Json<Vec<Booking>>> {
    Ok(Json(s.feast.partner_bookings(&u, &id, &q.date).await?))
}

#[derive(Deserialize)]
pub struct StatusReq<T> {
    status: T,
}

pub async fn booking_status(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<StatusReq<BookingStatus>>,
) -> ApiResult<Json<Booking>> {
    Ok(Json(s.feast.update_booking_status(&u, &id, r.status).await?))
}

/// The restaurant's printed QR code (one per restaurant).
pub async fn partner_qr(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<RestaurantQr>> {
    Ok(Json(s.feast.partner_qr(&u, &id).await?))
}

pub async fn payments(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Vec<BillPayment>>> {
    Ok(Json(s.feast.partner_payments(&u, &id).await?))
}

pub async fn menu(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Menu>> {
    Ok(Json(s.feast.partner_menu(&u, &id).await?))
}

pub async fn menu_item(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path((id, item)): Path<(String, String)>,
    Body(p): Body<MenuItemPatch>,
) -> ApiResult<Json<MenuItem>> {
    Ok(Json(s.feast.update_menu_item(&u, &id, &item, p).await?))
}

pub async fn offers(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Body(o): Body<Vec<Offer>>) -> ApiResult<Json<Vec<Offer>>> {
    Ok(Json(s.feast.update_offers(&u, &id, o).await?))
}

pub async fn settings(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(p): Body<RestaurantSettingsPatch>,
) -> ApiResult<Json<RestaurantSettings>> {
    Ok(Json(s.feast.update_settings(&u, &id, p).await?))
}

/// Today's deal budget: per-bill cap, daily limit, deals used so far.
pub async fn deal_budget(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<feast_domain::deals::DealBudget>> {
    Ok(Json(s.feast.deal_budget(&u, &id).await?))
}

pub async fn listing(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>, Body(p): Body<ListingPatch>) -> ApiResult<Json<RestaurantDetail>> {
    Ok(Json(s.feast.update_listing(&u, &id, p).await?))
}
