//! Discovery, booking, the restaurant QR, bills and payments (diner-facing).

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use feast_domain::model::*;
use serde::Deserialize;

use crate::error::ApiResult;
use crate::extract::{Body, Caller, Q};
use crate::AppState;

#[derive(Deserialize)]
pub struct CityQ {
    city: String,
}

pub async fn home(State(s): State<AppState>, Q(q): Q<CityQ>) -> ApiResult<Json<HomeResponse>> {
    Ok(Json(s.feast.home(&q.city).await?))
}

/// Explicit fields (not `#[serde(flatten)]`): flattening breaks typed query values.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQ {
    city: String,
    q: Option<String>,
    cuisine: Option<String>,
    tag: Option<String>,
    area: Option<String>,
    price_band: Option<i64>,
    offer: Option<OfferScope>,
    service: Option<SearchService>,
    pure_veg: Option<bool>,
    min_rating: Option<f64>,
    sort: Option<SearchSort>,
}

pub async fn search(State(s): State<AppState>, Q(p): Q<SearchQ>) -> ApiResult<Json<SearchResponse>> {
    let q = SearchQuery {
        q: p.q,
        cuisine: p.cuisine,
        tag: p.tag,
        area: p.area,
        price_band: p.price_band,
        offer: p.offer,
        service: p.service,
        pure_veg: p.pure_veg,
        min_rating: p.min_rating,
        sort: p.sort,
    };
    Ok(Json(s.feast.search(&p.city, &q).await?))
}

pub async fn restaurant(State(s): State<AppState>, Path(slug): Path<String>) -> ApiResult<Json<RestaurantDetail>> {
    Ok(Json(s.feast.restaurant(&slug).await?))
}

pub async fn menu(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Menu>> {
    Ok(Json(s.feast.menu(&id).await?))
}

#[derive(Deserialize)]
pub struct SlotsQ {
    date: String,
    guests: i64,
}

pub async fn slots(State(s): State<AppState>, Path(id): Path<String>, Q(q): Q<SlotsQ>) -> ApiResult<Json<SlotAvailability>> {
    Ok(Json(s.feast.slots(&id, &q.date, q.guests).await?))
}

// ------------------------------------------------------------ booking

pub async fn create_booking(
    State(s): State<AppState>,
    Caller(u): Caller,
    Body(r): Body<CreateBookingRequest>,
) -> ApiResult<(StatusCode, Json<CreateBookingResponse>)> {
    Ok((StatusCode::CREATED, Json(s.feast.create_booking(&u, r).await?)))
}

pub async fn booking(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Booking>> {
    Ok(Json(s.feast.booking(&u, &id).await?))
}

pub async fn my_bookings(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<Booking>>> {
    Ok(Json(s.feast.my_bookings(&u).await?))
}

pub async fn cancel_booking(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Booking>> {
    Ok(Json(s.feast.cancel_booking(&u, &id).await?))
}

pub async fn check_in_booking(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<Booking>> {
    Ok(Json(s.feast.check_in_booking(&u, &id).await?))
}

// ------------------------------------------------------------ restaurant QR

/// Public: what a scanned restaurant QR (`/r/{code}`, or an old table code) opens.
pub async fn restaurant_qr(State(s): State<AppState>, Path(code): Path<String>) -> ApiResult<Json<RestaurantQr>> {
    Ok(Json(s.feast.restaurant_qr(&code).await?))
}

// ------------------------------------------------------------ bills & payments

pub async fn quote_bill(State(s): State<AppState>, Caller(u): Caller, Body(r): Body<QuoteBillRequest>) -> ApiResult<Json<Bill>> {
    Ok(Json(s.feast.quote_bill(&u, r).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentReq {
    purpose: PaymentPurpose,
    reference_id: String,
}

pub async fn create_intent(State(s): State<AppState>, Caller(u): Caller, Body(r): Body<IntentReq>) -> ApiResult<(StatusCode, Json<PaymentIntent>)> {
    Ok((StatusCode::CREATED, Json(s.feast.create_payment_intent(&u, r.purpose, &r.reference_id).await?)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmReq {
    method: PaymentMethod,
    /// Test payments only: "failure" simulates a declined payment.
    simulate: Option<String>,
    /// What the provider's checkout returned (e.g. Razorpay payment id +
    /// signature). Verified server-side; never trusted as proof on its own.
    #[serde(default)]
    provider_payload: BTreeMap<String, String>,
}

pub async fn confirm_payment(
    State(s): State<AppState>,
    Caller(u): Caller,
    Path(id): Path<String>,
    Body(r): Body<ConfirmReq>,
) -> ApiResult<Json<PaymentIntent>> {
    Ok(Json(s.feast.confirm_payment(&u, &id, r.method, r.simulate.as_deref() == Some("failure"), &r.provider_payload).await?))
}

pub async fn bill_payment(State(s): State<AppState>, Caller(u): Caller, Path(id): Path<String>) -> ApiResult<Json<BillPayment>> {
    Ok(Json(s.feast.bill_payment(&u, &id).await?))
}

pub async fn my_bill_payments(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<BillPayment>>> {
    Ok(Json(s.feast.my_bill_payments(&u).await?))
}
