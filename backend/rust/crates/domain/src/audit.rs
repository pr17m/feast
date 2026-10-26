//! The audit trail: an append-only record of every state change — who did what,
//! to which booking / payment / case / review, from which status to which, and
//! for how much.
//!
//! Records are derived from the domain events every change already emits
//! ([`records_for`]) and written in the same database transaction as the change
//! and its outbox row, so the trail can't miss a committed change or record one
//! that rolled back. The few changes that have no event (owner edits, admin
//! status changes) are appended directly.
//!
//! The record shape is language-neutral JSON (see backend/spec/AUDIT.md); `id` is the
//! event id plus `#n`, which makes writing it idempotent.

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::events::{DomainEvent, EventEnvelope, Party};
use crate::model::{BookingStatus, CaseChange, IsoDateTime, OfflinePayment, Paise, Role};

/// What kind of actor made a change.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActorKind {
    /// A signed-in person (diner, owner, Feast staff).
    User,
    /// A payment provider's webhook.
    Webhook,
    /// Feast itself: background jobs (deadlines, expiry, reminders), seeding.
    #[default]
    System,
    /// An unauthenticated request (e.g. a guest opening a restaurant QR).
    Anonymous,
}

impl ActorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "USER",
            Self::Webhook => "WEBHOOK",
            Self::System => "SYSTEM",
            Self::Anonymous => "ANONYMOUS",
        }
    }
}

/// Who caused a change, with request context when there was one.
#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub kind: ActorKind,
    pub user_id: Option<String>,
    pub role: Option<Role>,
    pub name: Option<String>,
    /// Client IP (after trusted proxy hops).
    pub ip: Option<String>,
    /// `x-request-id` of the API call.
    pub request_id: Option<String>,
    /// For system/webhook actors: which job or provider, e.g. "sweeper", "stripe".
    pub source: Option<String>,
}

impl Actor {
    pub fn system(source: &str) -> Self {
        Self { kind: ActorKind::System, source: Some(source.to_owned()), ..Self::default() }
    }

    pub fn webhook(provider: &str) -> Self {
        Self { kind: ActorKind::Webhook, source: Some(provider.to_owned()), ..Self::default() }
    }

    /// Short label for lists: "Chloe Nguyen (owner)", "System · sweeper".
    pub fn label(&self) -> String {
        match self.kind {
            ActorKind::User => {
                let role = self.role.map(|r| r.as_str().to_lowercase()).unwrap_or_default();
                format!("{} ({role})", self.name.as_deref().unwrap_or("Someone"))
            }
            ActorKind::Webhook => format!("Payment provider · {}", self.source.as_deref().unwrap_or("webhook")),
            ActorKind::System => format!("Feast · {}", self.source.as_deref().unwrap_or("system")),
            ActorKind::Anonymous => "Guest".to_owned(),
        }
    }
}

/// One immutable audit record.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecord {
    pub id: String,
    pub at: IsoDateTime,
    pub restaurant_id: Option<String>,
    /// booking, bill, bill_payment, payment, offline_payment,
    /// case, review, document, restaurant, subscription_payment, user, menu_item, offer, settings…
    pub entity_type: String,
    pub entity_id: String,
    /// Stable machine name, e.g. `booking.cancelled`, `payment.offline_approved`.
    pub action: String,
    pub actor: Actor,
    /// The diner the record is about (lets them see their own history).
    pub subject_user_id: Option<String>,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub amount: Option<Paise>,
    pub currency: Option<String>,
    /// Reason, note or a human summary.
    pub note: Option<String>,
    /// The domain event it came from (absent for direct records).
    pub event_id: Option<String>,
}

impl AuditRecord {
    /// A record for a change without a domain event.
    pub fn direct(id: String, at: IsoDateTime, actor: Actor, entity_type: &str, entity_id: &str, action: &str) -> Self {
        Self {
            id,
            at,
            restaurant_id: None,
            entity_type: entity_type.to_owned(),
            entity_id: entity_id.to_owned(),
            action: action.to_owned(),
            actor,
            subject_user_id: None,
            from_status: None,
            to_status: None,
            amount: None,
            currency: None,
            note: None,
            event_id: None,
        }
    }
}

/// Filters for reading the trail (newest first, keyset-paginated).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuditQuery {
    pub restaurant_id: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    /// Any of these entity ids (e.g. a payment's bill, intent and receipt).
    pub entity_ids: Vec<String>,
    pub actor_id: Option<String>,
    pub subject_user_id: Option<String>,
    /// Inclusive lower bound (RFC 3339 or date).
    pub from: Option<String>,
    /// Exclusive upper bound.
    pub to: Option<String>,
    /// Continue after this cursor (from a previous page).
    pub cursor: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuditPage {
    pub items: Vec<AuditRecord>,
    pub next_cursor: Option<String>,
}

/// Page size bounds.
pub const DEFAULT_PAGE: u32 = 50;
pub const MAX_PAGE: u32 = 200;

pub fn page_size(requested: Option<u32>) -> u32 {
    requested.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE)
}

/// Cursor for keyset pagination: "<at>|<id>" of the last item.
pub fn cursor_of(r: &AuditRecord) -> String {
    format!("{}|{}", r.at, r.id)
}

pub fn parse_cursor(c: &str) -> Option<(String, String)> {
    let (at, id) = c.split_once('|')?;
    (!at.is_empty() && !id.is_empty()).then(|| (at.to_owned(), id.to_owned()))
}

/// Serialized enum tag, e.g. `BookingStatus::NoShow` → "NO_SHOW".
fn tag<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::new(),
    }
}

fn party(p: Party) -> &'static str {
    match p {
        Party::Customer => "diner",
        Party::Restaurant => "restaurant",
        Party::Admin => "Feast",
    }
}

fn offline_record(base: &AuditRecord, p: &OfflinePayment, action: &str, from: Option<&str>) -> AuditRecord {
    AuditRecord {
        restaurant_id: Some(p.restaurant.id.clone()),
        entity_type: "offline_payment".into(),
        entity_id: p.id.clone(),
        action: action.into(),
        subject_user_id: Some(p.user_id.clone()),
        from_status: from.map(str::to_owned),
        to_status: Some(p.status.as_str().into()),
        amount: Some(p.amount),
        currency: Some(p.currency.clone()),
        note: Some(match &p.reason {
            Some(r) => format!("{} · bill {} · {r}", p.method.as_str(), p.bill_id),
            None => format!("{} · bill {}", p.method.as_str(), p.bill_id),
        }),
        ..base.clone()
    }
}

/// The audit records for one event (empty for events that aren't state
/// changes, e.g. notifications and reminders).
pub fn records_for(env: &EventEnvelope) -> Vec<AuditRecord> {
    let base = AuditRecord {
        id: String::new(),
        at: env.occurred_at.clone(),
        restaurant_id: env.restaurant_id.clone(),
        entity_type: String::new(),
        entity_id: String::new(),
        action: env.type_name().to_owned(),
        actor: env.actor.clone().unwrap_or_else(|| Actor::system("unknown")),
        subject_user_id: env.user_id.clone(),
        from_status: None,
        to_status: None,
        amount: None,
        currency: None,
        note: None,
        event_id: Some(env.id.clone()),
    };
    let rec = |entity_type: &str, entity_id: &str| AuditRecord { entity_type: entity_type.into(), entity_id: entity_id.into(), ..base.clone() };
    let mut out: Vec<AuditRecord> = match &env.event {
        DomainEvent::BookingCreated { booking: b } => {
            let paid = b.payment.is_some() && b.status == BookingStatus::Confirmed && b.cover_charge > 0;
            vec![AuditRecord {
                action: if paid { "booking.confirmed".into() } else { "booking.created".into() },
                from_status: paid.then(|| BookingStatus::PendingPayment.as_str().into()),
                to_status: Some(b.status.as_str().into()),
                amount: (b.cover_charge > 0).then_some(b.cover_charge),
                currency: (b.cover_charge > 0).then(|| b.currency.clone()),
                note: Some(format!("{} · {} {} · {} guests", b.code, b.date, b.time, b.guests)),
                restaurant_id: Some(b.restaurant.id.clone()),
                subject_user_id: Some(b.user_id.clone()),
                ..rec("booking", &b.id)
            }]
        }
        DomainEvent::BookingCancelled { booking: b, by } => vec![AuditRecord {
            to_status: Some(b.status.as_str().into()),
            note: Some(format!("{} · cancelled by the {}", b.code, party(*by))),
            restaurant_id: Some(b.restaurant.id.clone()),
            subject_user_id: Some(b.user_id.clone()),
            ..rec("booking", &b.id)
        }],
        DomainEvent::BookingStatusChanged { booking: b, from } => vec![AuditRecord {
            from_status: Some(from.as_str().into()),
            to_status: Some(b.status.as_str().into()),
            note: Some(b.code.clone()),
            restaurant_id: Some(b.restaurant.id.clone()),
            subject_user_id: Some(b.user_id.clone()),
            ..rec("booking", &b.id)
        }],
        DomainEvent::BookingCheckedIn { booking: b } => vec![AuditRecord {
            to_status: Some(b.status.as_str().into()),
            note: Some(format!("{} · diner tapped \"I'm here\"", b.code)),
            restaurant_id: Some(b.restaurant.id.clone()),
            subject_user_id: Some(b.user_id.clone()),
            ..rec("booking", &b.id)
        }],
        DomainEvent::PaymentSucceeded { bill_payment_id, booking_id, amount, currency, .. } => {
            let (t, id) = match (bill_payment_id, booking_id) {
                (Some(bp), _) => ("bill_payment", bp.as_str()),
                (None, Some(b)) => ("booking", b.as_str()),
                (None, None) => ("payment", env.id.as_str()),
            };
            vec![AuditRecord { to_status: Some("SUCCEEDED".into()), amount: Some(*amount), currency: Some(currency.clone()), ..rec(t, id) }]
        }
        DomainEvent::SubscriptionPaid { payment: p } => vec![AuditRecord {
            to_status: Some(tag(&p.status)),
            amount: Some(p.total),
            currency: Some(p.currency.clone()),
            note: Some(format!("{} · {} plan", p.invoice_number, p.plan_id)),
            restaurant_id: Some(p.restaurant.id.clone()),
            subject_user_id: None,
            ..rec("subscription_payment", &p.id)
        }],
        DomainEvent::ApplicationSubmitted { restaurant } => vec![AuditRecord { subject_user_id: None, ..rec("restaurant", &restaurant.id) }],
        DomainEvent::DocumentReviewed { document: d, requirement_name } => vec![AuditRecord {
            to_status: Some(tag(&d.status)),
            note: Some(match &d.review_note {
                Some(n) => format!("{requirement_name} · {n}"),
                None => requirement_name.clone(),
            }),
            restaurant_id: Some(d.restaurant_id.clone()),
            subject_user_id: None,
            ..rec("document", &d.id)
        }],
        DomainEvent::ChangesRequested { restaurant, notes } => {
            vec![AuditRecord { note: Some(notes.clone()), subject_user_id: None, ..rec("restaurant", &restaurant.id) }]
        }
        DomainEvent::RestaurantApproved { restaurant, trial_ends_at } => vec![AuditRecord {
            to_status: Some("LIVE".into()),
            note: trial_ends_at.as_ref().map(|t| format!("Free trial until {}", &t[..10.min(t.len())])),
            subject_user_id: None,
            ..rec("restaurant", &restaurant.id)
        }],
        DomainEvent::CaseOpened { case } => vec![AuditRecord {
            to_status: Some(tag(&case.status)),
            note: Some(format!("{} · {} · {}", case.number, tag(&case.kind), tag(&case.reason))),
            restaurant_id: Some(case.restaurant.id.clone()),
            subject_user_id: Some(case.customer_id.clone()),
            ..rec("case", &case.id)
        }],
        DomainEvent::CaseUpdated { case, change, by } => vec![AuditRecord {
            action: match change {
                CaseChange::Message => "case.message".into(),
                CaseChange::Escalated => "case.escalated".into(),
                CaseChange::AutoEscalated => "case.auto_escalated".into(),
                CaseChange::Resolved => "case.resolved".into(),
                CaseChange::AutoClosed => "case.auto_closed".into(),
            },
            to_status: Some(tag(&case.status)),
            note: Some(format!("{} · by {}", case.number, tag(by).to_lowercase())),
            restaurant_id: Some(case.restaurant.id.clone()),
            subject_user_id: Some(case.customer_id.clone()),
            ..rec("case", &case.id)
        }],
        DomainEvent::ReviewPosted { review_id, restaurant, rating, .. } => vec![AuditRecord {
            note: Some(format!("{rating}★")),
            restaurant_id: Some(restaurant.id.clone()),
            subject_user_id: env.user_id.clone(),
            ..rec("review", review_id)
        }],
        DomainEvent::ReviewReplied { review_id, restaurant, user_id } => {
            vec![AuditRecord { restaurant_id: Some(restaurant.id.clone()), subject_user_id: Some(user_id.clone()), ..rec("review", review_id) }]
        }
        DomainEvent::ReviewReported { review_id, restaurant, reason } => {
            vec![AuditRecord { note: Some(reason.clone()), restaurant_id: Some(restaurant.id.clone()), ..rec("review", review_id) }]
        }
        DomainEvent::ReviewModerated { review_id, restaurant, user_id, hidden, reason } => vec![AuditRecord {
            to_status: Some(if *hidden { "HIDDEN".into() } else { "VISIBLE".into() }),
            note: reason.clone(),
            restaurant_id: Some(restaurant.id.clone()),
            subject_user_id: Some(user_id.clone()),
            ..rec("review", review_id)
        }],
        DomainEvent::ContentModerated { restaurant, kind, title, hidden, reason } => vec![AuditRecord {
            to_status: Some(if *hidden { "HIDDEN".into() } else { "VISIBLE".into() }),
            note: Some(match reason {
                Some(r) => format!("{title} · {r}"),
                None => title.clone(),
            }),
            restaurant_id: Some(restaurant.id.clone()),
            subject_user_id: None,
            ..rec(&tag(kind).to_lowercase(), title)
        }],
        DomainEvent::ModerationAppealed { restaurant, kind, title, note } => vec![AuditRecord {
            note: Some(match note {
                Some(n) => format!("{title} · {n}"),
                None => title.clone(),
            }),
            restaurant_id: Some(restaurant.id.clone()),
            subject_user_id: None,
            ..rec(&tag(kind).to_lowercase(), title)
        }],
        DomainEvent::SupportAccessChanged { restaurant, until } => vec![AuditRecord {
            note: Some(match until {
                Some(u) => format!("Feast staff may edit until {}", u.replace('T', " ")),
                None => "Support access revoked".into(),
            }),
            subject_user_id: None,
            ..rec("restaurant", &restaurant.id)
        }],
        DomainEvent::PaymentStarted { intent_id, purpose, reference_id, amount, currency, provider } => vec![AuditRecord {
            to_status: Some("CREATED".into()),
            amount: Some(*amount),
            currency: Some(currency.clone()),
            note: Some(format!("{} {reference_id} · {provider}", tag(purpose).to_lowercase())),
            ..rec("payment", intent_id)
        }],
        DomainEvent::PaymentFailed { intent_id, purpose, reference_id, amount, currency, reason } => vec![AuditRecord {
            from_status: Some("CREATED".into()),
            to_status: Some("FAILED".into()),
            amount: Some(*amount),
            currency: Some(currency.clone()),
            note: Some(match reason {
                Some(r) => format!("{} {reference_id} · {r}", tag(purpose).to_lowercase()),
                None => format!("{} {reference_id}", tag(purpose).to_lowercase()),
            }),
            ..rec("payment", intent_id)
        }],
        DomainEvent::OfflinePaymentSubmitted { payment } => vec![offline_record(&base, payment, "payment.offline_submitted", None)],
        DomainEvent::OfflinePaymentApproved { payment } => {
            vec![offline_record(&base, payment, "payment.offline_approved", Some("PENDING_APPROVAL"))]
        }
        DomainEvent::OfflinePaymentRejected { payment } => {
            vec![offline_record(&base, payment, "payment.offline_rejected", Some("PENDING_APPROVAL"))]
        }
        DomainEvent::OfflinePaymentExpired { payment } => vec![offline_record(&base, payment, "payment.offline_expired", Some("PENDING_APPROVAL"))],
        DomainEvent::DealClaimed { bill_id, booking_id, intent_id, offline } => vec![AuditRecord {
            to_status: Some(if *offline { "PENDING_OFFLINE".into() } else { "HELD".into() }),
            note: Some(match booking_id {
                Some(b) => format!("booking {b} · payment {intent_id}"),
                None => format!("payment {intent_id}"),
            }),
            ..rec("bill", bill_id)
        }],
        DomainEvent::DealUsed { bill_id, booking_id, discount, currency } => vec![AuditRecord {
            to_status: Some("USED".into()),
            amount: Some(*discount),
            currency: Some(currency.clone()),
            note: booking_id.as_ref().map(|b| format!("booking {b}")),
            ..rec("bill", bill_id)
        }],
        DomainEvent::DealReleased { bill_id, intent_id, reason } => {
            vec![AuditRecord { to_status: Some("RELEASED".into()), note: Some(format!("{reason} · payment {intent_id}")), ..rec("bill", bill_id) }]
        }
        // Not state changes: notifications, reminders.
        DomainEvent::NotificationCreated { .. } | DomainEvent::SubscriptionExpiring { .. } | DomainEvent::DocumentExpiring { .. } => vec![],
    };
    for (i, r) in out.iter_mut().enumerate() {
        r.id = format!("{}#{i}", env.id);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventEnvelope;

    fn env(event: DomainEvent) -> EventEnvelope {
        EventEnvelope::new("evt_1".into(), "2026-10-25T08:00:00.000Z".into(), Some("r_1".into()), Some("u_1".into()), event)
    }

    #[test]
    fn deal_events_map_to_bill_records_with_actor() {
        let actor = Actor { kind: ActorKind::User, user_id: Some("u_1".into()), role: Some(Role::Customer), ..Actor::default() };
        let e = env(DomainEvent::DealClaimed { bill_id: "bill_1".into(), booking_id: Some("bk_1".into()), intent_id: "pi_1".into(), offline: true })
            .by(actor.clone());
        let r = records_for(&e);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].entity_type.as_str(), r[0].entity_id.as_str(), r[0].action.as_str()), ("bill", "bill_1", "deal.claimed"));
        assert_eq!(r[0].to_status.as_deref(), Some("PENDING_OFFLINE"));
        assert_eq!(r[0].actor, actor);
        assert_eq!(r[0].id, "evt_1#0");
        assert_eq!(r[0].event_id.as_deref(), Some("evt_1"));
    }

    #[test]
    fn events_without_actor_are_attributed_to_the_system() {
        let e = env(DomainEvent::PaymentFailed {
            intent_id: "pi_1".into(),
            purpose: crate::model::PaymentPurpose::Bill,
            reference_id: "bill_1".into(),
            amount: 500,
            currency: "AUD".into(),
            reason: Some("Card declined".into()),
        });
        let r = &records_for(&e)[0];
        assert_eq!(r.actor.kind, ActorKind::System);
        assert_eq!((r.from_status.as_deref(), r.to_status.as_deref()), (Some("CREATED"), Some("FAILED")));
        assert_eq!(r.amount, Some(500));
    }

    #[test]
    fn notifications_are_not_audited() {
        let e = env(DomainEvent::SubscriptionExpiring {
            restaurant: crate::model::RestaurantRef {
                id: "r_1".into(),
                slug: "r".into(),
                name: "R".into(),
                area: "A".into(),
                art: crate::model::Art { hue: 1, motif: crate::model::CuisineMotif::Cafe, image: None, image_credit: None },
            },
            ends_at: "2026-11-01T00:00:00Z".into(),
            days_left: 7,
            trial: true,
        });
        assert!(records_for(&e).is_empty());
    }

    #[test]
    fn cursors_round_trip() {
        let r = AuditRecord::direct("aud_1".into(), "2026-10-25T08:00:00.000Z".into(), Actor::system("test"), "user", "u_1", "user.blocked");
        assert_eq!(parse_cursor(&cursor_of(&r)), Some(("2026-10-25T08:00:00.000Z".into(), "aud_1".into())));
        assert_eq!(parse_cursor("nope"), None);
        assert_eq!(page_size(None), DEFAULT_PAGE);
        assert_eq!(page_size(Some(10_000)), MAX_PAGE);
        assert_eq!(Actor::system("sweeper").label(), "Feast · sweeper");
    }
}
