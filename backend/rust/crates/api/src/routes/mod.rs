//! Route table. Paths and payloads match the webapp's `FeastApi` contract
//! (`webapp/src/lib/api/client.ts`) and `docs/openapi.yaml`.

mod admin;
mod auth;
mod customer;
mod health;
mod insights;
mod markets;
mod me;
mod media;
mod offline;
mod onboarding;
mod partner;
mod trust;
mod webhooks;

use axum::routing::{get, patch, post, put};
use axum::Router;

use crate::AppState;

pub fn api() -> Router<AppState> {
    Router::new()
        // auth
        .route("/v1/auth/logout", post(auth::logout))
        .route("/v1/auth/providers", get(auth::providers))
        .route("/v1/me", get(me::me))
        .route("/v1/me/identities", get(me::identities))
        .route("/v1/me/bookings", get(customer::my_bookings))
        .route("/v1/me/insights", get(insights::diner))
        .route("/v1/me/bill-payments", get(customer::my_bill_payments))
        .route("/v1/me/notifications", get(me::notifications))
        .route("/v1/me/notifications/read", post(me::mark_read))
        // discovery
        .route("/v1/cities", get(markets::cities))
        .route("/v1/markets", get(markets::markets))
        .route("/v1/geo", get(markets::geo))
        .route("/v1/home", get(customer::home))
        .route("/v1/restaurants", get(customer::search))
        .route("/v1/restaurants/{slug}", get(customer::restaurant))
        .route("/v1/restaurants/{id}/menu", get(customer::menu))
        .route("/v1/restaurants/{id}/slots", get(customer::slots))
        // booking
        .route("/v1/bookings", post(customer::create_booking))
        .route("/v1/bookings/{id}", get(customer::booking))
        .route("/v1/bookings/{id}/cancel", post(customer::cancel_booking))
        .route("/v1/bookings/{id}/check-in", post(customer::check_in_booking))
        // the restaurant's printed QR
        .route("/v1/qr/{code}", get(customer::restaurant_qr))
        // bills & payments
        .route("/v1/bills/quote", post(customer::quote_bill))
        .route("/v1/payments/intents", post(customer::create_intent))
        .route("/v1/payments/intents/{id}/confirm", post(customer::confirm_payment))
        .route("/v1/bill-payments/{id}", get(customer::bill_payment))
        .route("/v1/bill-payments/{id}/history", get(offline::bill_payment_history))
        .route("/v1/bills/{id}/offline-payment", get(offline::for_bill).post(offline::submit))
        .route("/v1/bookings/{id}/history", get(offline::booking_history))
        .route("/v1/partner/restaurants/{id}/offline-payments", get(offline::restaurant_list))
        .route("/v1/partner/offline-payments/{id}/approve", post(offline::approve))
        .route("/v1/partner/offline-payments/{id}/reject", post(offline::reject))
        .route("/v1/partner/restaurants/{id}/audit", get(offline::restaurant_audit))
        .route("/v1/admin/audit", get(offline::admin_audit))
        // payment provider webhooks (signature-verified, no sign-in)
        .route("/v1/webhooks/payments/{provider}", post(webhooks::payment_webhook))
        // partner
        .route("/v1/partner/restaurants", get(partner::restaurants))
        .route("/v1/partner/restaurants/{id}/dashboard", get(partner::dashboard))
        .route("/v1/partner/restaurants/{id}/insights", get(insights::owner))
        .route("/v1/partner/restaurants/{id}/bookings", get(partner::bookings))
        .route("/v1/partner/bookings/{id}/status", post(partner::booking_status))
        .route("/v1/partner/restaurants/{id}/qr", get(partner::partner_qr))
        .route("/v1/partner/restaurants/{id}/payments", get(partner::payments))
        .route("/v1/partner/restaurants/{id}/menu", get(partner::menu))
        .route("/v1/partner/restaurants/{id}/menu/items/{item_id}", patch(partner::menu_item))
        .route("/v1/partner/restaurants/{id}/menu/items/{item_id}/photo/find", post(media::find_menu_photo))
        .route("/v1/media/{*path}", get(media::media_file))
        .route("/v1/partner/restaurants/{id}/offers", put(partner::offers))
        .route("/v1/partner/restaurants/{id}/settings", patch(partner::settings))
        .route("/v1/partner/restaurants/{id}/deal-budget", get(partner::deal_budget))
        .route("/v1/partner/restaurants/{id}/listing", patch(partner::listing))
        // onboarding & subscriptions (partner)
        .route("/v1/partner/restaurants/{id}/compliance", get(onboarding::compliance))
        .route("/v1/partner/restaurants/{id}/submit", post(onboarding::submit))
        .route("/v1/documents/{id}/file", get(onboarding::document_file))
        .route("/v1/partner/restaurants/{id}/subscription", get(onboarding::subscription))
        .route("/v1/partner/restaurants/{id}/subscription/checkout", post(onboarding::subscription_checkout))
        // admin
        .route("/v1/admin/metrics", get(admin::metrics))
        .route("/v1/admin/insights", get(insights::admin))
        .route("/v1/admin/restaurants", get(admin::restaurants).post(admin::create_restaurant))
        .route("/v1/admin/restaurants/{id}", patch(admin::update_restaurant))
        .route("/v1/admin/restaurants/{id}/status", post(admin::restaurant_status))
        .route("/v1/admin/users", get(admin::users).post(admin::create_user))
        .route("/v1/admin/users/{id}", patch(admin::update_user))
        .route("/v1/admin/bookings", get(admin::bookings))
        .route("/v1/admin/payments", get(admin::payments))
        .route("/v1/admin/demo/reset", post(admin::reset_demo))
        .route("/v1/admin/approvals", get(onboarding::approvals))
        .route("/v1/admin/documents/{id}/review", post(onboarding::review_document))
        .route("/v1/admin/restaurants/{id}/request-changes", post(onboarding::request_changes))
        .route("/v1/admin/subscriptions", get(onboarding::subscriptions))
        .route("/v1/admin/platform-payments", get(onboarding::platform_payments))
        .route("/v1/admin/restaurants/{id}/subscription/extend", post(onboarding::extend))
        .route("/v1/admin/payment-providers", get(markets::payment_providers))
        .route("/v1/admin/markets/{cc}", put(markets::save_market))
        .route("/v1/admin/cities", post(markets::create_city))
        // grievances
        .route("/v1/cases", post(trust::open_case))
        .route("/v1/me/cases", get(trust::my_cases))
        .route("/v1/cases/{id}", get(trust::get_case))
        .route("/v1/cases/{id}/messages", post(trust::case_message))
        .route("/v1/cases/{id}/escalate", post(trust::escalate_case))
        .route("/v1/cases/{id}/settle", post(trust::settle_case))
        .route("/v1/cases/{id}/attachments/{attachment_id}", get(trust::case_attachment_file))
        .route("/v1/partner/restaurants/{id}/cases", get(trust::restaurant_cases))
        .route("/v1/admin/cases", get(trust::admin_cases))
        .route("/v1/admin/cases/{id}/resolve", post(trust::resolve_case))
        // reviews
        .route("/v1/restaurants/{slug}/reviews", get(trust::restaurant_reviews))
        .route("/v1/reviews", post(trust::create_review))
        .route("/v1/me/reviews", get(trust::my_reviews))
        .route("/v1/partner/restaurants/{id}/reviews", get(trust::partner_reviews))
        .route("/v1/partner/reviews/{id}/reply", post(trust::reply_review))
        .route("/v1/partner/reviews/{id}/report", post(trust::report_review))
        .route("/v1/admin/reviews", get(trust::admin_reviews))
        .route("/v1/admin/reviews/{id}/moderate", post(trust::moderate_review))
        // content moderation & support access
        .route("/v1/admin/restaurants/{id}/moderation", post(trust::moderate_content))
        .route("/v1/admin/moderation", get(trust::moderation_queue))
        .route("/v1/partner/restaurants/{id}/moderation/appeal", post(trust::appeal_moderation))
        .route("/v1/partner/restaurants/{id}/support-access", get(trust::support_access).put(trust::set_support_access))
}

/// Multipart uploads (bigger body limit).
pub fn uploads() -> Router<AppState> {
    Router::new()
        .route("/v1/partner/restaurants/{id}/documents", post(onboarding::upload))
        .route("/v1/cases/{id}/attachments", post(trust::case_attachment_upload))
        .route("/v1/partner/restaurants/{id}/menu/items/{item_id}/photo", post(media::upload_menu_photo).delete(media::remove_menu_photo))
}

/// Sign-in and signup: rate limited per client IP.
pub fn limited() -> Router<AppState> {
    Router::new()
        .route("/v1/auth/otp", post(auth::request_otp))
        .route("/v1/auth/otp/verify", post(auth::verify_otp))
        .route("/v1/auth/login", post(auth::login))
        .route("/v1/auth/oauth/{provider}", post(auth::oauth))
        .route("/v1/partner/signup", post(onboarding::signup))
}

/// Long-lived streams (no request timeout).
pub fn streams() -> Router<AppState> {
    Router::new().route("/v1/me/events", get(me::events))
}

pub fn health() -> Router<AppState> {
    Router::new().route("/healthz", get(health::live)).route("/readyz", get(health::ready))
}
