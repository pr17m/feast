//! Ports: the interfaces the application needs from the outside world.
//!
//! Business code depends only on these traits. Infrastructure provides adapters
//! (SQLite today, Postgres tomorrow; in-memory or Redis bus; mock or real payment
//! gateway; embedded or gRPC notification service). Swapping technology means
//! writing a new adapter and choosing it in config — no business code changes.
//!
//! Write methods that change state other parties care about take `events`: the
//! adapter must persist them to the outbox in the *same transaction* as the
//! change (transactional outbox), so an event is never lost or sent for a
//! change that didn't happen.

use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;

use async_trait::async_trait;
use feast_domain::model::*;
use feast_domain::EventEnvelope;

use crate::error::{AppError, AppResult};

// ============================================================ persistence

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn get(&self, id: &str) -> AppResult<Option<User>>;
    async fn find_by_phone(&self, phone: &str) -> AppResult<Option<User>>;
    /// The user and their password hash (if they have one).
    async fn find_by_email(&self, email: &str) -> AppResult<Option<(User, Option<String>)>>;
    async fn list(&self) -> AppResult<Vec<User>>;
    /// Active OWNER users linked to a restaurant.
    async fn owners_of(&self, restaurant_id: &str) -> AppResult<Vec<User>>;
    async fn insert(&self, user: &User, password_hash: Option<&str>) -> AppResult<()>;
    async fn update(&self, user: &User) -> AppResult<()>;
    /// The user linked to a social identity (provider + provider's user id).
    async fn find_by_identity(&self, provider: AuthProvider, subject: &str) -> AppResult<Option<User>>;
    /// Link a social identity to a user (idempotent).
    async fn link_identity(&self, user_id: &str, identity: &ExternalIdentity) -> AppResult<()>;
    /// Providers linked to a user.
    async fn identities_of(&self, user_id: &str) -> AppResult<Vec<AuthProvider>>;
    /// Account deletion, in ONE transaction: mark the user DELETED (sign-in
    /// refused), clear name/phone/email/password, drop social identities,
    /// restaurant links and push tokens, and replace the person's name/phone on
    /// the records that are kept (bookings, counter payments, reviews, dispute
    /// threads) with `display_name`.
    async fn anonymise(&self, user_id: &str, display_name: &str, at: &str) -> AppResult<()>;
}

#[async_trait]
pub trait RestaurantRepository: Send + Sync {
    async fn list(&self) -> AppResult<Vec<Restaurant>>;
    async fn list_by_city(&self, city_id: &str) -> AppResult<Vec<Restaurant>>;
    async fn get(&self, id: &str) -> AppResult<Option<Restaurant>>;
    async fn get_by_slug(&self, slug: &str) -> AppResult<Option<Restaurant>>;
    /// Resolve the code in a restaurant's printed QR (`/r/{code}`).
    async fn find_by_short_code(&self, code: &str) -> AppResult<Option<Restaurant>>;
    async fn insert(&self, r: &Restaurant) -> AppResult<()>;
    async fn update(&self, r: &Restaurant) -> AppResult<()>;
    /// restaurant id → city id, for every restaurant (cheap lookup).
    async fn city_ids(&self) -> AppResult<HashMap<String, String>>;
}

/// One discounted bill's hold on the restaurant's daily deal budget.
#[derive(Debug, Clone, PartialEq)]
pub struct DealClaim {
    pub bill_id: String,
    pub restaurant_id: String,
    /// The restaurant's local date (YYYY-MM-DD).
    pub date: String,
    pub intent_id: String,
    pub claimed_at: String,
    /// The Feast booking the deal belongs to: one discounted bill per booking.
    pub booking_id: Option<String>,
    /// Held for a counter payment awaiting the restaurant: counts until it is
    /// approved (USED) or rejected/expired (released), however long that takes.
    pub pending_offline: bool,
}

/// Capacity rule for one slot, checked atomically when booking.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotRoom {
    /// Tables Feast may sell for the slot.
    pub tables: i64,
    /// Unpaid bookings created at/after this instant still hold a table.
    pub pending_since: String,
}

#[async_trait]
pub trait BookingRepository: Send + Sync {
    /// Insert without capacity checks (seeding, imports).
    async fn insert(&self, b: &Booking, idempotency_key: &str, events: &[EventEnvelope]) -> AppResult<()>;
    /// Insert only if the slot still has a table, atomically: the adapter must
    /// serialise concurrent bookings of the same slot (across all instances)
    /// so the last table is sold once. Returns false when the slot is full.
    async fn insert_if_room(&self, b: &Booking, idempotency_key: &str, events: &[EventEnvelope], room: &SlotRoom) -> AppResult<bool>;
    async fn find_by_idempotency_key(&self, user_id: &str, key: &str) -> AppResult<Option<Booking>>;
    async fn get(&self, id: &str) -> AppResult<Option<Booking>>;
    async fn update(&self, b: &Booking, events: &[EventEnvelope]) -> AppResult<()>;
    /// Store `b` (+ events) only if the booking is still in `expected` status:
    /// one winner when several requests change the same booking at once.
    async fn update_if_status(&self, b: &Booking, expected: BookingStatus, events: &[EventEnvelope]) -> AppResult<bool>;
    async fn list_by_user(&self, user_id: &str) -> AppResult<Vec<Booking>>;
    async fn list_by_restaurant_between(&self, restaurant_id: &str, from: &str, to: &str) -> AppResult<Vec<Booking>>;
    /// Tables held per slot time on a date: CONFIRMED/SEATED, plus PENDING_PAYMENT created at/after `pending_since`.
    async fn tables_held(&self, restaurant_id: &str, date: &str, pending_since: &str) -> AppResult<HashMap<String, i64>>;
    async fn list_recent(&self, limit: u32) -> AppResult<Vec<Booking>>;
    /// Every restaurant's bookings for slot dates in `[from, to]` (insights).
    async fn list_between(&self, from: &str, to: &str) -> AppResult<Vec<Booking>>;
}

#[async_trait]
pub trait BillingRepository: Send + Sync {
    async fn insert_bill(&self, b: &Bill) -> AppResult<()>;
    async fn get_bill(&self, id: &str) -> AppResult<Option<Bill>>;
    async fn is_bill_paid(&self, bill_id: &str) -> AppResult<bool>;

    /// Record a new intent (and its `payment.started` event, for the audit trail).
    async fn insert_intent(&self, i: &PaymentIntent, events: &[EventEnvelope]) -> AppResult<()>;
    async fn get_intent(&self, id: &str) -> AppResult<Option<PaymentIntent>>;
    /// The intent a provider's order / payment-intent id belongs to (webhooks).
    async fn find_intent_by_provider_ref(&self, provider: &str, provider_ref: &str) -> AppResult<Option<PaymentIntent>>;
    /// Save a non-final change (e.g. FAILED). Never overwrites a SUCCEEDED intent; returns whether it changed.
    /// Events are written only when it changed.
    async fn update_intent(&self, i: &PaymentIntent, events: &[EventEnvelope]) -> AppResult<bool>;

    // Completing a payment = *claim* the intent (flip it to SUCCEEDED only if it
    // isn't already) + apply the effects + events, in one transaction. Returns
    // false when someone else (the browser confirm or a provider webhook)
    // completed it first — then nothing is applied twice.

    /// Returns `NotApplied` (nothing written) when the bill was already paid by
    /// another payment (online or at the counter).
    async fn complete_bill_payment(&self, i: &PaymentIntent, bp: &BillPayment, events: &[EventEnvelope]) -> AppResult<Settled>;
    /// Confirm the paid booking — but only if it is still PENDING_PAYMENT and,
    /// when its table hold lapsed, a table is still free at its slot (checked
    /// under the same slot lock as booking). Otherwise `NotApplied`, nothing written.
    async fn complete_booking_payment(&self, i: &PaymentIntent, booking: &Booking, room: &SlotRoom, events: &[EventEnvelope]) -> AppResult<Settled>;
    /// A payment the provider took but that can't be applied: claim the intent
    /// (SUCCEEDED, so it is never applied later) and record its refund, in one
    /// transaction. False when someone else settled the intent first.
    async fn record_unapplied_payment(&self, i: &PaymentIntent, refund: &feast_domain::refunds::Refund, events: &[EventEnvelope]) -> AppResult<bool>;
    async fn complete_subscription_payment(
        &self,
        i: &PaymentIntent,
        restaurant_id: &str,
        rec: &SubscriptionRecord,
        payment: &PlatformPayment,
        events: &[EventEnvelope],
    ) -> AppResult<bool>;

    /// Daily deal budget (`feast_domain::deals`). Hold a deal for one bill:
    /// under a per-restaurant-day lock, count the day's USED claims plus HELD
    /// ones made at/after `held_since` (other bills only) and refuse when
    /// `limit` is reached, or when another bill already used (or holds) the
    /// same booking's deal. Re-claiming the same bill refreshes its hold.
    /// Paying the bill (`complete_bill_payment`) turns the claim USED.
    /// `events` are written (same transaction) only when the deal is claimed.
    async fn claim_deal(
        &self,
        claim: &DealClaim,
        limit: Option<i64>,
        held_since: &str,
        events: &[EventEnvelope],
    ) -> AppResult<feast_domain::deals::ClaimOutcome>;
    /// A paid bill already used this booking's deal.
    async fn booking_deal_used(&self, booking_id: &str) -> AppResult<bool>;
    /// The payment for this claim failed: free the deal (only if `intent_id`
    /// still holds it, HELD or PENDING_OFFLINE). Returns whether it was freed;
    /// `events` are written only then.
    async fn release_deal(&self, bill_id: &str, intent_id: &str, events: &[EventEnvelope]) -> AppResult<bool>;
    /// Deals counted against the restaurant's day: USED + HELD at/after `held_since`.
    async fn deals_used(&self, restaurant_id: &str, date: &str, held_since: &str) -> AppResult<i64>;

    async fn get_bill_payment(&self, id: &str) -> AppResult<Option<BillPayment>>;
    async fn payments_by_user(&self, user_id: &str) -> AppResult<Vec<BillPayment>>;
    async fn payments_by_restaurant(&self, restaurant_id: &str) -> AppResult<Vec<BillPayment>>;
    async fn list_recent_payments(&self, limit: u32) -> AppResult<Vec<BillPayment>>;
    /// Every bill paid on Feast at or after `since` (RFC 3339 UTC).
    async fn payments_since(&self, since: &str) -> AppResult<Vec<BillPayment>>;
}

/// Markets (countries) and their cities. One deployment usually serves one country.
#[async_trait]
pub trait MarketRepository: Send + Sync {
    async fn list_markets(&self) -> AppResult<Vec<Market>>;
    async fn get_market(&self, country_code: &str) -> AppResult<Option<Market>>;
    async fn upsert_market(&self, m: &Market) -> AppResult<()>;
    async fn list_cities(&self) -> AppResult<Vec<City>>;
    async fn get_city(&self, id: &str) -> AppResult<Option<City>>;
    async fn upsert_city(&self, c: &City) -> AppResult<()>;
}

/// Restaurant subscriptions and payments to the platform.
#[async_trait]
pub trait SubscriptionRepository: Send + Sync {
    async fn get_record(&self, restaurant_id: &str) -> AppResult<Option<SubscriptionRecord>>;
    async fn all_records(&self) -> AppResult<HashMap<String, SubscriptionRecord>>;
    async fn put_record(&self, restaurant_id: &str, rec: &SubscriptionRecord) -> AppResult<()>;
    /// Atomically: store the payment (+ the new record when paid) + events.
    async fn record_payment(&self, restaurant_id: &str, rec: Option<&SubscriptionRecord>, payment: &PlatformPayment, events: &[EventEnvelope])
        -> AppResult<()>;
    async fn payments_for(&self, restaurant_id: &str) -> AppResult<Vec<PlatformPayment>>;
    async fn all_payments(&self) -> AppResult<Vec<PlatformPayment>>;
    /// Next invoice sequence number (monotonic per deployment).
    async fn next_invoice_seq(&self) -> AppResult<i64>;
}

/// Admin review state of a restaurant's application.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReviewInfo {
    pub notes: Option<String>,
    pub submitted_at: Option<String>,
}

/// Onboarding/compliance documents (metadata; files live in `FileStorage`).
#[async_trait]
pub trait ComplianceRepository: Send + Sync {
    async fn insert_document(&self, d: &ComplianceDocument) -> AppResult<()>;
    async fn get_document(&self, id: &str) -> AppResult<Option<ComplianceDocument>>;
    async fn update_document(&self, d: &ComplianceDocument) -> AppResult<()>;
    async fn documents_for(&self, restaurant_id: &str) -> AppResult<Vec<ComplianceDocument>>;
    async fn all_documents(&self) -> AppResult<Vec<ComplianceDocument>>;
    async fn review_info(&self, restaurant_id: &str) -> AppResult<ReviewInfo>;
    async fn set_review_info(&self, restaurant_id: &str, info: &ReviewInfo) -> AppResult<()>;
}

/// The transactional outbox, read side. (Writes happen inside the repositories above.)
/// Grievance cases. Updates are optimistic: `update_case` only writes when the
/// stored version is still `c.version - 1`, so two people acting on a case at
/// once can't overwrite each other.
#[async_trait]
pub trait CaseRepository: Send + Sync {
    async fn insert_case(&self, c: &Case, events: &[EventEnvelope]) -> AppResult<()>;
    /// Returns false when someone else changed the case first.
    async fn update_case(&self, c: &Case, events: &[EventEnvelope]) -> AppResult<bool>;
    async fn get_case(&self, id: &str) -> AppResult<Option<Case>>;
    async fn cases_for_restaurant(&self, restaurant_id: &str) -> AppResult<Vec<Case>>;
    async fn cases_for_customer(&self, user_id: &str) -> AppResult<Vec<Case>>;
    async fn all_cases(&self) -> AppResult<Vec<Case>>;
    /// The open case of this kind about this visit, if any.
    async fn open_case_for_visit(&self, kind: CaseKind, booking_id: Option<&str>, bill_payment_id: Option<&str>) -> AppResult<Option<Case>>;
    /// Open cases whose deadline is at or before `now` (worker sweep).
    async fn overdue_cases(&self, now: &str) -> AppResult<Vec<Case>>;
}

/// Star counts of a restaurant's published reviews.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RatingStats {
    pub sum: i64,
    pub count: i64,
    /// index 0 = 1★ … 4 = 5★
    pub distribution: [i64; 5],
}

#[async_trait]
pub trait ReviewRepository: Send + Sync {
    /// Returns false when this visit (`visit_key`) already has a review.
    async fn insert_review(&self, r: &Review, visit_key: &str, events: &[EventEnvelope]) -> AppResult<bool>;
    async fn update_review(&self, r: &Review, events: &[EventEnvelope]) -> AppResult<()>;
    async fn get_review(&self, id: &str) -> AppResult<Option<Review>>;
    async fn reviews_for_restaurant(&self, restaurant_id: &str, published_only: bool, limit: u32) -> AppResult<Vec<Review>>;
    async fn reviews_by_user(&self, user_id: &str) -> AppResult<Vec<Review>>;
    /// Every review (published or hidden) posted at or after `since` (RFC 3339 UTC).
    async fn reviews_since(&self, since: &str) -> AppResult<Vec<Review>>;
    /// Reported or hidden reviews, newest first (admin moderation).
    async fn reviews_needing_attention(&self) -> AppResult<Vec<Review>>;
    async fn rating_stats(&self, restaurant_id: &str) -> AppResult<RatingStats>;
}

#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn record(&self, e: &AuditEntry) -> AppResult<()>;
    /// Newest first.
    async fn audit_for(&self, restaurant_id: &str, limit: u32) -> AppResult<Vec<AuditEntry>>;
}

#[async_trait]
pub trait OutboxRepository: Send + Sync {
    /// Append events that aren't tied to another write.
    async fn append(&self, events: &[EventEnvelope]) -> AppResult<()>;
    /// Oldest unpublished events, in order.
    async fn fetch_pending(&self, limit: u32) -> AppResult<Vec<(i64, EventEnvelope)>>;
    async fn mark_published(&self, seqs: &[i64]) -> AppResult<()>;
    async fn record_failure(&self, seq: i64, error: &str) -> AppResult<()>;
}

/// Wipes and re-seeds demo data (demo builds only).
#[async_trait]
pub trait DemoDataPort: Send + Sync {
    async fn reset(&self) -> AppResult<()>;
}

// ============================================================ messaging

/// Async messaging between services (customer side ⇄ restaurant side).
#[async_trait]
pub trait MessageBus: Send + Sync {
    async fn publish(&self, event: &EventEnvelope) -> AppResult<()>;
    async fn subscribe(&self, mode: SubscriptionMode) -> AppResult<Box<dyn Subscription>>;
    /// Connectivity check for readiness probes.
    async fn health(&self) -> AppResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum SubscriptionMode {
    /// Competing consumers: each event goes to one member of the group
    /// (e.g. the notification projector, however many workers run).
    Group { group: String, consumer: String },
    /// Every subscriber gets every event published after it subscribed
    /// (e.g. each gateway instance pushing live updates to its own clients).
    Broadcast,
}

#[derive(Debug, Clone)]
pub struct Delivery {
    /// Bus-specific id (Redis stream entry id, sequence number…).
    pub id: String,
    pub envelope: EventEnvelope,
}

#[async_trait]
pub trait Subscription: Send {
    /// Next event; `None` when the bus shuts down.
    async fn next(&mut self) -> AppResult<Option<Delivery>>;
    /// Acknowledge successful processing (no-op for broadcast).
    async fn ack(&mut self, delivery: &Delivery) -> AppResult<()>;
}

// ============================================================ other services

/// Draft notification sent to many recipients.
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationDraft {
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    pub link: Option<String>,
    pub restaurant_id: Option<String>,
}

/// The notification service, seen from the core. Adapters: embedded (same
/// process) or gRPC client (separate service) — chosen in config.
#[async_trait]
pub trait NotificationGateway: Send + Sync {
    async fn publish(&self, event_id: &str, event_type: &str, recipients: &[String], draft: &NotificationDraft) -> AppResult<Vec<Notification>>;
    async fn list(&self, user_id: &str, unread_only: bool, limit: u32) -> AppResult<NotificationList>;
    async fn mark_read(&self, user_id: &str, ids: &[String]) -> AppResult<i64>;
    async fn unread_count(&self, user_id: &str) -> AppResult<i64>;
}

/// A user identity asserted by a social login provider, after verification.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalIdentity {
    pub provider: AuthProvider,
    /// The provider's stable user id (`sub` claim / Facebook user id).
    pub subject: String,
    pub email: Option<String>,
    /// Only a provider-verified email may be used to link an existing account.
    pub email_verified: bool,
    pub name: Option<String>,
}

/// Verifies a credential from one social login provider (Google, Apple,
/// Facebook…). One adapter per provider; add providers without touching the
/// use case.
#[async_trait]
pub trait IdentityProvider: Send + Sync {
    fn info(&self) -> AuthProviderInfo;
    async fn verify(&self, req: &OAuthLoginRequest) -> AppResult<ExternalIdentity>;
}

/// Outcome of charging a payment intent.
#[derive(Debug, Clone, PartialEq)]
pub enum ChargeOutcome {
    /// Paid. `method` is what the provider reports (falls back to the client's choice).
    Succeeded {
        method: Option<PaymentMethod>,
        provider_payment_id: Option<String>,
    },
    Failed(String),
}

/// What completing a payment did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settled {
    /// This call applied it.
    Applied,
    /// Someone else (browser confirm or webhook) settled the intent first.
    AlreadySettled,
    /// The money arrived but there is nothing left to apply it to: refund it.
    NotApplied,
}

/// The provider's answer to a refund request.
#[derive(Debug, Clone, PartialEq)]
pub struct RefundOutcome {
    pub provider_refund_id: Option<String>,
    pub status: feast_domain::refunds::RefundStatus,
    pub failure: Option<String>,
}

/// A verified notification from a payment provider.
#[derive(Debug, Clone, PartialEq)]
pub enum WebhookEvent {
    /// Authentic but not about a payment we track (or an event type we don't use).
    Ignored(String),
    /// A refund changed status at the provider.
    Refund { provider_refund_id: String, status: feast_domain::refunds::RefundStatus, failure: Option<String> },
    Payment {
        /// The provider's order / payment-intent id (`PaymentIntent::provider_ref`).
        provider_ref: String,
        outcome: ChargeOutcome,
        /// What the provider says was paid (checked against the intent).
        amount: Option<Paise>,
        currency: Option<String>,
    },
}

/// What the client needs to open the provider's checkout.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CheckoutSession {
    /// Provider-side order / payment-intent id.
    pub provider_ref: Option<String>,
    /// Public data for the client SDK (never secrets): e.g. `keyId`, `orderId`,
    /// `clientSecret`, `publishableKey`, or `mode = "test"` for the simulator.
    pub checkout: BTreeMap<String, String>,
}

/// One payment provider (mock, Razorpay, Stripe…).
#[async_trait]
pub trait PaymentGateway: Send + Sync {
    fn info(&self) -> PaymentProviderInfo;
    /// Public client key for the provider's SDK (Razorpay key id, Stripe publishable key).
    fn public_key(&self) -> Option<String> {
        None
    }
    /// Create the provider-side order for an intent.
    async fn create_order(&self, intent: &PaymentIntent) -> AppResult<CheckoutSession>;
    /// Verify the payment with the provider (signature, or fetch status). Never
    /// trust the client: `payload` is only a pointer to what to verify.
    async fn charge(
        &self,
        intent: &PaymentIntent,
        method: PaymentMethod,
        simulate_failure: bool,
        payload: &BTreeMap<String, String>,
    ) -> AppResult<ChargeOutcome>;
    /// Send `refund.amount` back for an intent that SUCCEEDED. `refund.id` is
    /// the idempotency key: retrying the same refund never pays twice.
    async fn refund(&self, _intent: &PaymentIntent, _refund: &feast_domain::refunds::Refund) -> AppResult<RefundOutcome> {
        Err(AppError::new(crate::error::ErrorKind::Unavailable, "REFUNDS_UNAVAILABLE", "This payment provider can't refund automatically"))
    }
    /// Verify a webhook's signature (headers lower-cased) and read it. Providers
    /// without webhooks keep the default.
    fn parse_webhook(&self, _headers: &[(String, String)], _body: &[u8]) -> AppResult<WebhookEvent> {
        Err(AppError::not_found("Webhook"))
    }
}

/// Payment providers configured on this server, chosen per market.
pub trait PaymentProviders: Send + Sync {
    fn get(&self, id: &str) -> Option<std::sync::Arc<dyn PaymentGateway>>;
    fn list(&self) -> Vec<PaymentProviderInfo>;
}

/// Uploaded files (onboarding documents). Local disk in dev, S3-compatible in prod.
#[async_trait]
pub trait FileStorage: Send + Sync {
    async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> AppResult<()>;
    /// The bytes and content type.
    async fn get(&self, key: &str) -> AppResult<(Vec<u8>, String)>;
    async fn health(&self) -> AppResult<()> {
        Ok(())
    }
}

/// What the gateway knows about a request, for geolocation.
#[derive(Debug, Clone, Default)]
pub struct GeoRequest {
    pub ip: Option<IpAddr>,
    /// Lower-cased header names.
    pub headers: Vec<(String, String)>,
}

/// Country of a visitor (CDN header, GeoIP database…).
pub trait GeoLocator: Send + Sync {
    /// ISO country code and how it was found ("header", "geoip").
    fn locate(&self, req: &GeoRequest) -> Option<(String, &'static str)>;
}

/// One-time-password delivery and verification for phone sign-in.
#[async_trait]
pub trait OtpService: Send + Sync {
    /// Send an OTP. May return a hint to show in demo builds.
    async fn send(&self, phone: &str) -> AppResult<Option<String>>;
    async fn verify(&self, phone: &str, code: &str) -> AppResult<bool>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenClaims {
    pub user_id: String,
    pub role: Role,
}

pub trait TokenService: Send + Sync {
    fn issue(&self, user: &User) -> AppResult<String>;
    fn verify(&self, token: &str) -> AppResult<TokenClaims>;
}

pub trait PasswordHasher: Send + Sync {
    fn hash(&self, password: &str) -> AppResult<String>;
    fn verify(&self, password: &str, hash: &str) -> bool;
}

/// Time source (injectable for tests). "Local" means a city's IANA time zone
/// (`tz`); an empty or unknown `tz` uses the deployment's default zone.
pub trait Clock: Send + Sync {
    /// RFC 3339 UTC with milliseconds.
    fn now_iso(&self) -> String;
    fn now_ms(&self) -> i64;
    /// Local calendar date in `tz`, offset by `days`.
    fn local_date(&self, tz: &str, days: i64) -> String;
    /// Minutes since local midnight in `tz`.
    fn local_minutes(&self, tz: &str) -> i64;
    /// Milliseconds since epoch of a local date + `HH:MM` in `tz`.
    fn local_to_ms(&self, tz: &str, date: &str, hhmm: &str) -> Option<i64>;
    /// Local date (in `tz`) of a UTC timestamp.
    fn local_date_of(&self, tz: &str, iso: &str) -> Option<String>;
}

pub trait IdGenerator: Send + Sync {
    /// Sortable unique id with a type prefix, e.g. `bk_01J…`.
    fn new_id(&self, prefix: &str) -> String;
    /// Short human code (uppercase letters/digits), e.g. for booking codes.
    fn short_code(&self, len: usize) -> String;
}

/// What to look for when a dish has no photo.
#[derive(Debug, Clone, PartialEq)]
pub struct DishQuery {
    pub restaurant_id: String,
    pub item_id: String,
    pub dish: String,
    pub description: String,
    /// The restaurant's main cuisine, for context ("Masala dosa South Indian food").
    pub cuisine: Option<String>,
}

/// A photo a finder came up with.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundImage {
    /// URL diners load the image from (stock photos are hotlinked; a generator
    /// would store its bytes under `media/` and return that URL).
    pub url: String,
    /// Credit shown with the photo, e.g. "Unsplash · @name".
    pub credit: Option<String>,
    pub source: ImageSource,
}

/// Finds a picture for a dish the owner hasn't photographed: a stock-photo
/// search today (Unsplash), and the place an AI image generator plugs in later.
/// Implementations must never return a photo of the wrong dish rather than none.
#[async_trait]
pub trait DishImageFinder: Send + Sync {
    /// False when not configured (no API key): the apps show generated art.
    fn enabled(&self) -> bool;
    async fn find(&self, q: &DishQuery) -> AppResult<Option<FoundImage>>;
}

/// No finder configured.
pub struct NoDishImages;

#[async_trait]
impl DishImageFinder for NoDishImages {
    fn enabled(&self) -> bool {
        false
    }
    async fn find(&self, _q: &DishQuery) -> AppResult<Option<FoundImage>> {
        Ok(None)
    }
}

/// Counter (offline) payments awaiting the restaurant's confirmation.
#[async_trait]
pub trait OfflinePaymentRepository: Send + Sync {
    /// Record a submitted counter payment, its internal intent (provider
    /// "offline") and events in one transaction. Fails with `OFFLINE_PENDING`
    /// if the bill already has one waiting.
    async fn insert_offline(&self, p: &OfflinePayment, intent: &PaymentIntent, events: &[EventEnvelope]) -> AppResult<()>;
    async fn get_offline(&self, id: &str) -> AppResult<Option<OfflinePayment>>;
    /// The bill's most recent counter payment.
    async fn latest_for_bill(&self, bill_id: &str) -> AppResult<Option<OfflinePayment>>;
    /// Newest first; `status` filters (None = all).
    async fn offline_for_restaurant(&self, restaurant_id: &str, status: Option<OfflinePaymentStatus>, limit: u32) -> AppResult<Vec<OfflinePayment>>;
    /// Still pending with `expires_at` <= `now`.
    async fn expired_pending(&self, now: &str, limit: u32) -> AppResult<Vec<OfflinePayment>>;
    /// A diner's counter payments still waiting for the restaurant.
    async fn pending_for_user(&self, user_id: &str) -> AppResult<Vec<OfflinePayment>>;
    /// Approve: in ONE transaction flip the payment PENDING_APPROVAL → APPROVED
    /// (false if someone decided first), claim its intent, insert the bill
    /// payment (fails ALREADY_PAID if the bill was paid meanwhile, e.g. online),
    /// mark the bill's deal USED, write events.
    async fn approve_offline(&self, p: &OfflinePayment, intent: &PaymentIntent, bp: &BillPayment, events: &[EventEnvelope]) -> AppResult<bool>;
    /// Reject or expire: in ONE transaction flip PENDING_APPROVAL → the new
    /// status (false if decided already), fail the intent, free the deal it
    /// held, write events.
    async fn close_offline(&self, p: &OfflinePayment, intent: &PaymentIntent, events: &[EventEnvelope]) -> AppResult<bool>;
}

/// The append-only audit trail. There is deliberately no update or delete.
/// Records derived from events are written by the repositories together with
/// the events (`feast_domain::audit::records_for`); `append` is for changes
/// that have no event.
#[async_trait]
pub trait AuditTrailRepository: Send + Sync {
    async fn append(&self, r: &feast_domain::audit::AuditRecord) -> AppResult<()>;
    /// Newest first, keyset-paginated.
    async fn query(&self, q: &feast_domain::audit::AuditQuery) -> AppResult<feast_domain::audit::AuditPage>;
}

// ============================================================ push notifications

/// Phones registered for push, and each user's on/off switch.
#[async_trait]
pub trait PushTokenRepository: Send + Sync {
    /// Register or refresh a token (a token moves to the user who registered it last).
    async fn upsert_token(&self, t: &feast_domain::push::PushToken) -> AppResult<()>;
    /// Remove the caller's token; false if it wasn't theirs/registered.
    async fn delete_token(&self, user_id: &str, token: &str) -> AppResult<bool>;
    /// Forget a token the push service says is gone.
    async fn forget_token(&self, token: &str) -> AppResult<()>;
    async fn tokens_for(&self, user_id: &str) -> AppResult<Vec<feast_domain::push::PushToken>>;
    /// Default true.
    async fn push_enabled(&self, user_id: &str) -> AppResult<bool>;
    async fn set_push_enabled(&self, user_id: &str, enabled: bool) -> AppResult<()>;
    /// Claim the right to push notification `n` to phone `token`: true exactly
    /// once per (notification, token), however many workers see its
    /// `notification.created`. The claim (SENDING) lasts until `lease_until_ms`.
    async fn claim_push(&self, n: &Notification, token: &str, lease_until_ms: i64) -> AppResult<bool>;
    /// Record how a claimed send went: SENT, RETRY (at `next_at_ms`), FAILED,
    /// GAVE_UP or DROPPED.
    async fn finish_push(&self, notification_id: &str, token: &str, status: &str, next_at_ms: i64, error: Option<&str>) -> AppResult<()>;
    /// Claim pushes due for another try (RETRY whose time has come, or SENDING
    /// whose worker died): each is handed to one caller only, attempts + 1.
    async fn claim_due_pushes(&self, now_ms: i64, lease_until_ms: i64, limit: u32) -> AppResult<Vec<DuePush>>;
}

/// A push to send again (see `PushTokenRepository::claim_due_pushes`).
#[derive(Debug, Clone)]
pub struct DuePush {
    pub notification: Notification,
    pub token: String,
    /// Including this one.
    pub attempts: i64,
}

/// Refunds of online payments (`feast_domain::refunds`).
#[async_trait]
pub trait RefundRepository: Send + Sync {
    /// Insert a PENDING refund (+ events) unless one with the same
    /// `idempotency_key` exists; returns the stored one and whether it is new.
    async fn insert_refund(&self, r: &feast_domain::refunds::Refund, events: &[EventEnvelope]) -> AppResult<(feast_domain::refunds::Refund, bool)>;
    /// Save provider progress. Only a PENDING refund changes; returns whether it did.
    async fn update_refund(&self, r: &feast_domain::refunds::Refund, events: &[EventEnvelope]) -> AppResult<bool>;
    async fn get_refund(&self, id: &str) -> AppResult<Option<feast_domain::refunds::Refund>>;
    async fn find_refund_by_provider_id(&self, provider: &str, provider_refund_id: &str) -> AppResult<Option<feast_domain::refunds::Refund>>;
    async fn refunds_for_intent(&self, intent_id: &str) -> AppResult<Vec<feast_domain::refunds::Refund>>;
    /// PENDING refunds the provider hasn't accepted yet (no provider id), oldest first.
    async fn unsent_refunds(&self, limit: u32) -> AppResult<Vec<feast_domain::refunds::Refund>>;
    async fn list_refunds(&self, limit: u32) -> AppResult<Vec<feast_domain::refunds::Refund>>;
}

/// State every API instance and worker must share (not process memory):
/// rate-limit counters, one-time-code challenges and short leases.
#[async_trait]
pub trait SharedStateRepository: Send + Sync {
    /// Count one hit for `key` in the fixed window `window` (window index);
    /// returns the hits so far in that window.
    async fn rate_hit(&self, key: &str, window: i64, expires_ms: i64) -> AppResult<i64>;
    /// Store a code challenge (replacing any earlier one for `subject`).
    async fn otp_put(&self, subject: &str, code_hash: &str, expires_ms: i64) -> AppResult<()>;
    /// Check a code: counts one attempt atomically; true (and the challenge is
    /// consumed) only for the right code within the expiry and attempt limit.
    async fn otp_check(&self, subject: &str, code_hash: &str, now_ms: i64, max_attempts: i64) -> AppResult<bool>;
    /// Take or renew `name` for `holder` until `until_ms` (when free, expired or
    /// already ours). Only one holder at a time, across all processes.
    async fn lease(&self, name: &str, holder: &str, now_ms: i64, until_ms: i64) -> AppResult<bool>;
    /// Drop expired counters and challenges. Returns rows removed.
    async fn purge_expired(&self, now_ms: i64) -> AppResult<u64>;
}

/// Delivers a push to one phone (Firebase Cloud Messaging today).
/// Delivers a push to one phone (Firebase Cloud Messaging today).
#[async_trait]
pub trait PushSender: Send + Sync {
    /// "fcm", "log" or "off" (for logs and health).
    fn name(&self) -> &'static str;
    /// False when push is switched off: tokens are still kept.
    fn enabled(&self) -> bool {
        true
    }
    async fn send(&self, to: &feast_domain::push::PushToken, msg: &feast_domain::push::PushMessage) -> feast_domain::push::PushOutcome;
}

/// Push switched off (no provider configured).
pub struct NoPush;

#[async_trait]
impl PushSender for NoPush {
    fn name(&self) -> &'static str {
        "off"
    }
    fn enabled(&self) -> bool {
        false
    }
    async fn send(&self, _to: &feast_domain::push::PushToken, _msg: &feast_domain::push::PushMessage) -> feast_domain::push::PushOutcome {
        feast_domain::push::PushOutcome::Failed("push is off".into())
    }
}
