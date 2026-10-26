//! Domain events — the messages the customer side and the restaurant side exchange.
//!
//! Every state change that another party cares about emits an event. Events are
//! written to the outbox in the same database transaction as the change, then
//! relayed to the message bus (in-memory or Redis Streams). Consumers must be
//! idempotent: delivery is at-least-once.
//!
//! Wire format (language-neutral, see docs/EVENTS.md):
//! ```json
//! { "id": "evt_…", "type": "booking.created", "occurredAt": "…Z",
//!   "restaurantId": "r_…", "userId": "u_…", "data": { …event-specific… } }
//! ```

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::audit::Actor;
use crate::model::{
    Booking, BookingStatus, CaseChange, CaseRef, CaseRole, ComplianceDocument, ContentKind, IsoDateTime, Notification, OfflinePayment, Paise, PaymentPurpose,
    PlatformPayment, RestaurantRef,
};
use crate::refunds::Refund;

/// Typed domain events. `type` names are stable API — never rename them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum DomainEvent {
    #[serde(rename = "booking.created")]
    BookingCreated { booking: Booking },
    #[serde(rename = "booking.cancelled")]
    BookingCancelled { booking: Booking, by: Party },
    #[serde(rename = "booking.status_changed")]
    BookingStatusChanged { booking: Booking, from: BookingStatus },
    /// The diner tapped "I'm here" at the restaurant.
    #[serde(rename = "booking.checked_in")]
    BookingCheckedIn { booking: Booking },
    #[serde(rename = "payment.succeeded")]
    PaymentSucceeded {
        #[serde(rename = "billPaymentId")]
        bill_payment_id: Option<String>,
        #[serde(rename = "bookingId")]
        booking_id: Option<String>,
        amount: Paise,
        #[serde(default = "default_currency")]
        currency: String,
        /// Paid at the counter and confirmed by the restaurant (not settled by Feast).
        #[serde(default)]
        offline: bool,
    },
    #[serde(rename = "notification.created")]
    NotificationCreated { notification: Notification },
    /// A restaurant paid Feast for its listing.
    #[serde(rename = "subscription.paid")]
    SubscriptionPaid { payment: PlatformPayment },
    /// Trial or paid period ends soon (worker reminder: 7, 3, 1 days).
    #[serde(rename = "subscription.expiring")]
    SubscriptionExpiring {
        restaurant: RestaurantRef,
        #[serde(rename = "endsAt")]
        ends_at: IsoDateTime,
        #[serde(rename = "daysLeft")]
        days_left: i64,
        trial: bool,
    },
    /// An owner submitted documents for review.
    #[serde(rename = "application.submitted")]
    ApplicationSubmitted { restaurant: RestaurantRef },
    /// An admin approved or rejected one document.
    #[serde(rename = "document.reviewed")]
    DocumentReviewed {
        document: ComplianceDocument,
        #[serde(rename = "requirementName")]
        requirement_name: String,
    },
    /// An admin sent the application back.
    #[serde(rename = "application.changes_requested")]
    ChangesRequested { restaurant: RestaurantRef, notes: String },
    /// Approved and live (free trial started).
    #[serde(rename = "restaurant.approved")]
    RestaurantApproved {
        restaurant: RestaurantRef,
        #[serde(rename = "trialEndsAt")]
        trial_ends_at: Option<IsoDateTime>,
    },
    /// A required licence expires soon (worker reminder: 30, 7, 1 days).
    #[serde(rename = "document.expiring")]
    DocumentExpiring {
        restaurant: RestaurantRef,
        #[serde(rename = "requirementName")]
        requirement_name: String,
        #[serde(rename = "expiresOn")]
        expires_on: String,
        #[serde(rename = "daysLeft")]
        days_left: i64,
    },
    /// A diner complained about a restaurant, or a restaurant reported a diner.
    #[serde(rename = "case.opened")]
    CaseOpened { case: CaseRef },
    /// A grievance moved on (reply, escalation, decision, deadline).
    #[serde(rename = "case.updated")]
    CaseUpdated { case: CaseRef, change: CaseChange, by: CaseRole },
    #[serde(rename = "review.posted")]
    ReviewPosted {
        #[serde(rename = "reviewId")]
        review_id: String,
        restaurant: RestaurantRef,
        rating: i64,
        #[serde(rename = "authorName")]
        author_name: String,
    },
    #[serde(rename = "review.replied")]
    ReviewReplied {
        #[serde(rename = "reviewId")]
        review_id: String,
        restaurant: RestaurantRef,
        #[serde(rename = "userId")]
        user_id: String,
    },
    /// The restaurant asked Feast to look at a review.
    #[serde(rename = "review.reported")]
    ReviewReported {
        #[serde(rename = "reviewId")]
        review_id: String,
        restaurant: RestaurantRef,
        reason: String,
    },
    #[serde(rename = "review.moderated")]
    ReviewModerated {
        #[serde(rename = "reviewId")]
        review_id: String,
        restaurant: RestaurantRef,
        #[serde(rename = "userId")]
        user_id: String,
        hidden: bool,
        reason: Option<String>,
    },
    /// Feast hid or restored a menu item / offer.
    #[serde(rename = "content.moderated")]
    ContentModerated { restaurant: RestaurantRef, kind: ContentKind, title: String, hidden: bool, reason: Option<String> },
    /// The owner asked Feast to review hidden content again.
    #[serde(rename = "content.appealed")]
    ModerationAppealed { restaurant: RestaurantRef, kind: ContentKind, title: String, note: Option<String> },
    /// The owner granted (until = Some) or revoked Feast support access.
    #[serde(rename = "support_access.changed")]
    SupportAccessChanged { restaurant: RestaurantRef, until: Option<IsoDateTime> },
    /// A payment was started with a provider (or recorded for the counter).
    #[serde(rename = "payment.started")]
    PaymentStarted {
        #[serde(rename = "intentId")]
        intent_id: String,
        purpose: PaymentPurpose,
        #[serde(rename = "referenceId")]
        reference_id: String,
        amount: Paise,
        currency: String,
        provider: String,
    },
    /// The provider declined or the diner abandoned it.
    #[serde(rename = "payment.failed")]
    PaymentFailed {
        #[serde(rename = "intentId")]
        intent_id: String,
        purpose: PaymentPurpose,
        #[serde(rename = "referenceId")]
        reference_id: String,
        amount: Paise,
        currency: String,
        reason: Option<String>,
    },
    /// A diner says they paid at the counter; the restaurant must confirm.
    #[serde(rename = "payment.offline_submitted")]
    OfflinePaymentSubmitted { payment: OfflinePayment },
    /// The restaurant confirmed a counter payment: the bill is paid.
    #[serde(rename = "payment.offline_approved")]
    OfflinePaymentApproved { payment: OfflinePayment },
    /// The restaurant said it wasn't paid (reason given).
    #[serde(rename = "payment.offline_rejected")]
    OfflinePaymentRejected { payment: OfflinePayment },
    /// Nobody confirmed it in time.
    #[serde(rename = "payment.offline_expired")]
    OfflinePaymentExpired { payment: OfflinePayment },
    /// One of today's deals is held for a bill while it's being paid.
    #[serde(rename = "deal.claimed")]
    DealClaimed {
        #[serde(rename = "billId")]
        bill_id: String,
        #[serde(rename = "bookingId")]
        booking_id: Option<String>,
        #[serde(rename = "intentId")]
        intent_id: String,
        /// Held for a counter payment awaiting the restaurant.
        offline: bool,
    },
    /// A paid bill used its deal for good.
    #[serde(rename = "deal.used")]
    DealUsed {
        #[serde(rename = "billId")]
        bill_id: String,
        #[serde(rename = "bookingId")]
        booking_id: Option<String>,
        discount: Paise,
        currency: String,
    },
    /// A held deal went back to today's budget (payment failed, rejected or expired).
    #[serde(rename = "deal.released")]
    DealReleased {
        #[serde(rename = "billId")]
        bill_id: String,
        #[serde(rename = "intentId")]
        intent_id: String,
        reason: String,
    },
    /// A payment succeeded but couldn't be applied (its booking was cancelled
    /// or its table gone, or the bill was already paid): it is refunded.
    #[serde(rename = "payment.not_applied")]
    PaymentNotApplied {
        #[serde(rename = "intentId")]
        intent_id: String,
        purpose: PaymentPurpose,
        #[serde(rename = "referenceId")]
        reference_id: String,
        amount: Paise,
        currency: String,
        reason: String,
    },
    /// Feast asked the payment provider to send money back.
    #[serde(rename = "refund.requested")]
    RefundRequested { refund: Refund },
    /// The provider confirmed the refund.
    #[serde(rename = "refund.succeeded")]
    RefundSucceeded { refund: Refund },
    /// The provider refused the refund (support follows up).
    #[serde(rename = "refund.failed")]
    RefundFailed { refund: Refund },
}

fn default_currency() -> String {
    "INR".into()
}

/// Who initiated a change.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Party {
    Customer,
    Restaurant,
    Admin,
}

impl DomainEvent {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::BookingCreated { .. } => "booking.created",
            Self::BookingCancelled { .. } => "booking.cancelled",
            Self::BookingStatusChanged { .. } => "booking.status_changed",
            Self::BookingCheckedIn { .. } => "booking.checked_in",
            Self::PaymentSucceeded { .. } => "payment.succeeded",
            Self::NotificationCreated { .. } => "notification.created",
            Self::SubscriptionPaid { .. } => "subscription.paid",
            Self::SubscriptionExpiring { .. } => "subscription.expiring",
            Self::ApplicationSubmitted { .. } => "application.submitted",
            Self::DocumentReviewed { .. } => "document.reviewed",
            Self::ChangesRequested { .. } => "application.changes_requested",
            Self::RestaurantApproved { .. } => "restaurant.approved",
            Self::DocumentExpiring { .. } => "document.expiring",
            Self::CaseOpened { .. } => "case.opened",
            Self::CaseUpdated { .. } => "case.updated",
            Self::ReviewPosted { .. } => "review.posted",
            Self::ReviewReplied { .. } => "review.replied",
            Self::ReviewReported { .. } => "review.reported",
            Self::ReviewModerated { .. } => "review.moderated",
            Self::ContentModerated { .. } => "content.moderated",
            Self::ModerationAppealed { .. } => "content.appealed",
            Self::SupportAccessChanged { .. } => "support_access.changed",
            Self::PaymentStarted { .. } => "payment.started",
            Self::PaymentFailed { .. } => "payment.failed",
            Self::OfflinePaymentSubmitted { .. } => "payment.offline_submitted",
            Self::OfflinePaymentApproved { .. } => "payment.offline_approved",
            Self::OfflinePaymentRejected { .. } => "payment.offline_rejected",
            Self::OfflinePaymentExpired { .. } => "payment.offline_expired",
            Self::DealClaimed { .. } => "deal.claimed",
            Self::DealUsed { .. } => "deal.used",
            Self::DealReleased { .. } => "deal.released",
            Self::PaymentNotApplied { .. } => "payment.not_applied",
            Self::RefundRequested { .. } => "refund.requested",
            Self::RefundSucceeded { .. } => "refund.succeeded",
            Self::RefundFailed { .. } => "refund.failed",
        }
    }
}

/// The envelope that travels through the outbox and the message bus.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    pub id: String,
    pub occurred_at: IsoDateTime,
    /// Routing: owners of this restaurant receive it.
    pub restaurant_id: Option<String>,
    /// Routing: this user receives it.
    pub user_id: Option<String>,
    #[serde(flatten)]
    pub event: DomainEvent,
    /// Who caused it (signed-in user, webhook, background job). Feeds the audit trail.
    #[serde(default)]
    pub actor: Option<Actor>,
}

impl EventEnvelope {
    pub fn new(id: String, occurred_at: IsoDateTime, restaurant_id: Option<String>, user_id: Option<String>, event: DomainEvent) -> Self {
        Self { id, occurred_at, restaurant_id, user_id, event, actor: None }
    }

    pub fn by(mut self, actor: Actor) -> Self {
        self.actor = Some(actor);
        self
    }

    pub fn type_name(&self) -> &'static str {
        self.event.type_name()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("event serializes")
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}
