//! Notifications: who hears about what.
//!
//! The worker feeds every domain event through [`Feast::project_notification`],
//! which decides recipients and wording, then hands the draft to the
//! notification service (embedded or over gRPC). The notification service
//! stores it idempotently and publishes `notification.created` for live delivery.

use chrono::NaiveDate;
use feast_domain::model::*;
use feast_domain::{DomainEvent, EventEnvelope, Party};

use crate::context::Feast;
use crate::error::AppResult;
use crate::ports::NotificationDraft;

/// Money in the event's currency (e.g. "₹1,23,456", "A$45.50").
pub fn fmt_money(minor: Paise, currency: &str) -> String {
    feast_domain::market::format_currency(minor, currency)
}

fn fmt_date(iso: &str) -> String {
    NaiveDate::parse_from_str(iso, "%Y-%m-%d").map(|d| d.format("%a, %-d %b").to_string()).unwrap_or_else(|_| iso.to_owned())
}

fn fmt_time(hhmm: &str) -> String {
    let m = feast_domain::slots::parse_hhmm(hhmm);
    let (h, min) = (m / 60, m % 60);
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    format!("{h12}:{min:02} {}", if h >= 12 { "PM" } else { "AM" })
}

fn plural(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A notification to send: who gets it and what it says.
pub struct Plan {
    pub recipients: Recipients,
    pub draft: NotificationDraft,
}

pub enum Recipients {
    OwnersOf(String),
    User(String),
    /// Every active admin of this deployment.
    Admins,
}

/// Pure mapping from a domain event to the notification it should produce.
pub fn plan_for(env: &EventEnvelope) -> Option<Plan> {
    let owners = |rid: &str| Recipients::OwnersOf(rid.to_owned());
    match &env.event {
        DomainEvent::BookingCreated { booking: b } => Some(Plan {
            recipients: owners(&b.restaurant.id),
            draft: NotificationDraft {
                kind: NotificationKind::BookingCreated,
                title: format!("New booking · {}", plural(b.guests, "guest", "guests")),
                body: format!(
                    "{} booked {} at {}{}. Code {}.",
                    b.contact.name,
                    fmt_date(&b.date),
                    fmt_time(&b.time),
                    b.deal.as_ref().map(|d| format!(" with {}", d.title.to_lowercase())).unwrap_or_default(),
                    b.code
                ),
                link: Some("/partner/bookings".into()),
                restaurant_id: Some(b.restaurant.id.clone()),
            },
        }),
        DomainEvent::BookingCancelled { booking: b, by } => Some(match by {
            Party::Customer => Plan {
                recipients: owners(&b.restaurant.id),
                draft: NotificationDraft {
                    kind: NotificationKind::BookingCancelled,
                    title: "Booking cancelled".into(),
                    body: format!("{} cancelled their table for {} on {} at {}.", b.contact.name, b.guests, fmt_date(&b.date), fmt_time(&b.time)),
                    link: Some("/partner/bookings".into()),
                    restaurant_id: Some(b.restaurant.id.clone()),
                },
            },
            _ => Plan {
                recipients: Recipients::User(b.user_id.clone()),
                draft: NotificationDraft {
                    kind: NotificationKind::BookingCancelled,
                    title: "Your booking was cancelled".into(),
                    body: format!(
                        "{} cancelled your table on {} at {}. Please contact the restaurant.",
                        b.restaurant.name,
                        fmt_date(&b.date),
                        fmt_time(&b.time)
                    ),
                    link: Some(format!("/bookings/{}", b.id)),
                    restaurant_id: Some(b.restaurant.id.clone()),
                },
            },
        }),
        DomainEvent::BookingCheckedIn { booking: b } => Some(Plan {
            recipients: owners(&b.restaurant.id),
            draft: NotificationDraft {
                kind: NotificationKind::BookingUpdated,
                title: format!("{} has arrived", b.contact.name),
                body: format!(
                    "Booking {} · {} at {}. Seat them and they can pay the bill on Feast to get their deal.",
                    b.code,
                    plural(b.guests, "guest", "guests"),
                    fmt_time(&b.time)
                ),
                link: Some("/partner/bookings".into()),
                restaurant_id: Some(b.restaurant.id.clone()),
            },
        }),
        DomainEvent::BookingStatusChanged { booking: b, .. } => {
            let (title, body, link) = match b.status {
                BookingStatus::Seated => (
                    "Enjoy your meal!".to_owned(),
                    format!(
                        "You're seated at {}.{}",
                        b.restaurant.name,
                        b.deal.as_ref().map(|d| format!(" Pay your bill on Feast to get {}.", d.title.to_lowercase())).unwrap_or_default()
                    ),
                    format!("/pay/{}", b.restaurant.slug),
                ),
                BookingStatus::NoShow => (
                    "Marked as no-show".to_owned(),
                    format!("{} marked your {} booking as a no-show. Something wrong? Contact the restaurant.", b.restaurant.name, fmt_date(&b.date)),
                    format!("/bookings/{}", b.id),
                ),
                _ => return None,
            };
            Some(Plan {
                recipients: Recipients::User(b.user_id.clone()),
                draft: NotificationDraft {
                    kind: NotificationKind::BookingUpdated,
                    title,
                    body,
                    link: Some(link),
                    restaurant_id: Some(b.restaurant.id.clone()),
                },
            })
        }
        // The restaurant confirmed a counter payment itself: nothing to tell it.
        DomainEvent::PaymentSucceeded { offline: true, .. } => None,
        DomainEvent::PaymentSucceeded { amount, currency, booking_id, .. } => {
            let rid = env.restaurant_id.clone()?;
            let (title, body, link) = if booking_id.is_some() {
                ("Cover charge received".to_owned(), format!("{} paid to hold a table.", fmt_money(*amount, currency)), "/partner/bookings")
            } else {
                (
                    format!("Payment received · {}", fmt_money(*amount, currency)),
                    format!("A diner paid {} on Feast.", fmt_money(*amount, currency)),
                    "/partner/payments",
                )
            };
            Some(Plan {
                recipients: owners(&rid),
                draft: NotificationDraft { kind: NotificationKind::PaymentReceived, title, body, link: Some(link.into()), restaurant_id: Some(rid) },
            })
        }
        DomainEvent::SubscriptionPaid { payment: p } => Some(Plan {
            recipients: owners(&p.restaurant.id),
            draft: NotificationDraft {
                kind: NotificationKind::SubscriptionPaid,
                title: format!("Subscription renewed · {}", fmt_money(p.total, &p.currency)),
                body: format!(
                    "{} stays listed until {}. Invoice {}.",
                    p.restaurant.name,
                    fmt_date(&p.period_end[..10.min(p.period_end.len())]),
                    p.invoice_number
                ),
                link: Some("/partner/subscription".into()),
                restaurant_id: Some(p.restaurant.id.clone()),
            },
        }),
        DomainEvent::SubscriptionExpiring { restaurant: r, ends_at, days_left, trial } => Some(Plan {
            recipients: owners(&r.id),
            draft: NotificationDraft {
                kind: NotificationKind::SubscriptionExpiring,
                title: if *trial {
                    format!("Free trial ends in {}", plural(*days_left, "day", "days"))
                } else {
                    format!("Subscription ends in {}", plural(*days_left, "day", "days"))
                },
                body: format!("Choose a plan to keep {} visible to diners after {}.", r.name, fmt_date(&ends_at[..10.min(ends_at.len())])),
                link: Some("/partner/subscription".into()),
                restaurant_id: Some(r.id.clone()),
            },
        }),
        DomainEvent::ApplicationSubmitted { restaurant: r } => Some(Plan {
            recipients: Recipients::Admins,
            draft: NotificationDraft {
                kind: NotificationKind::ApplicationSubmitted,
                title: format!("New application · {}", r.name),
                body: format!("{} ({}) submitted documents for review.", r.name, r.area),
                link: Some("/admin/approvals".into()),
                restaurant_id: Some(r.id.clone()),
            },
        }),
        DomainEvent::DocumentReviewed { document: d, requirement_name } => {
            let approved = d.status == DocumentStatus::Approved;
            Some(Plan {
                recipients: owners(&d.restaurant_id),
                draft: NotificationDraft {
                    kind: NotificationKind::DocumentReviewed,
                    title: format!("{requirement_name} {}", if approved { "approved" } else { "needs attention" }),
                    body: if approved {
                        "Thanks — this document is verified.".into()
                    } else {
                        format!("Please upload it again. {}", d.review_note.as_deref().unwrap_or(""))
                    },
                    link: Some("/partner/documents".into()),
                    restaurant_id: Some(d.restaurant_id.clone()),
                },
            })
        }
        DomainEvent::ChangesRequested { restaurant: r, notes } => Some(Plan {
            recipients: owners(&r.id),
            draft: NotificationDraft {
                kind: NotificationKind::DocumentReviewed,
                title: "Changes needed to your application".into(),
                body: notes.chars().take(200).collect(),
                link: Some("/partner/documents".into()),
                restaurant_id: Some(r.id.clone()),
            },
        }),
        DomainEvent::RestaurantApproved { restaurant: r, trial_ends_at } => Some(Plan {
            recipients: owners(&r.id),
            draft: NotificationDraft {
                kind: NotificationKind::RestaurantApproved,
                title: format!("{} is live on Feast", r.name),
                body: match trial_ends_at {
                    Some(t) => format!("Diners can now find you. Your free trial runs until {}.", fmt_date(&t[..10.min(t.len())])),
                    None => "Diners can now find you.".into(),
                },
                link: Some("/partner".into()),
                restaurant_id: Some(r.id.clone()),
            },
        }),
        DomainEvent::DocumentExpiring { restaurant: r, requirement_name, expires_on, days_left } => Some(Plan {
            recipients: owners(&r.id),
            draft: NotificationDraft {
                kind: NotificationKind::DocumentExpiring,
                title: format!("{requirement_name} expires in {}", plural(*days_left, "day", "days")),
                body: format!("Upload the renewed document before {} to stay listed.", fmt_date(expires_on)),
                link: Some("/partner/documents".into()),
                restaurant_id: Some(r.id.clone()),
            },
        }),
        DomainEvent::NotificationCreated { .. } => None,
        // Trust & safety events can notify several parties: see `trust_plans`.
        DomainEvent::CaseOpened { .. }
        | DomainEvent::CaseUpdated { .. }
        | DomainEvent::ReviewPosted { .. }
        | DomainEvent::ReviewReplied { .. }
        | DomainEvent::ReviewReported { .. }
        | DomainEvent::ReviewModerated { .. }
        | DomainEvent::ContentModerated { .. }
        | DomainEvent::ModerationAppealed { .. }
        | DomainEvent::SupportAccessChanged { .. }
        | DomainEvent::PaymentStarted { .. }
        | DomainEvent::PaymentFailed { .. }
        | DomainEvent::DealClaimed { .. }
        | DomainEvent::DealUsed { .. }
        | DomainEvent::DealReleased { .. } => None,
        DomainEvent::OfflinePaymentSubmitted { payment: p } => Some(Plan {
            recipients: owners(&p.restaurant.id),
            draft: NotificationDraft {
                kind: NotificationKind::PaymentReceived,
                title: format!("Confirm a counter payment · {}", fmt_money(p.amount, &p.currency)),
                body: format!(
                    "{} says they're paying {} {}. Confirm it once you have the money.",
                    p.diner_name,
                    fmt_money(p.amount, &p.currency),
                    offline_method(p.method),
                ),
                link: Some("/partner/payments".into()),
                restaurant_id: Some(p.restaurant.id.clone()),
            },
        }),
        DomainEvent::OfflinePaymentApproved { payment: p } => Some(Plan {
            recipients: Recipients::User(p.user_id.clone()),
            draft: NotificationDraft {
                kind: NotificationKind::PaymentReceived,
                title: "Payment confirmed".into(),
                body: format!(
                    "{} confirmed your {} payment of {}{}.",
                    p.restaurant.name,
                    offline_method(p.method),
                    fmt_money(p.amount, &p.currency),
                    if p.savings > 0 { format!(" — your Feast deal saved {}", fmt_money(p.savings, &p.currency)) } else { String::new() }
                ),
                link: p.bill_payment_id.as_ref().map(|id| format!("/payments/{id}")),
                restaurant_id: Some(p.restaurant.id.clone()),
            },
        }),
        DomainEvent::OfflinePaymentRejected { payment: p } | DomainEvent::OfflinePaymentExpired { payment: p } => Some(Plan {
            recipients: Recipients::User(p.user_id.clone()),
            draft: NotificationDraft {
                kind: NotificationKind::PaymentReceived,
                title: "Counter payment not confirmed".into(),
                body: format!(
                    "{} didn't confirm your payment of {}{}. Your bill is still open — pay again on Feast.",
                    p.restaurant.name,
                    fmt_money(p.amount, &p.currency),
                    p.reason.as_ref().map(|r| format!(" ({r})")).unwrap_or_default()
                ),
                link: Some(format!("/pay/{}", p.restaurant.slug)),
                restaurant_id: Some(p.restaurant.id.clone()),
            },
        }),
    }
}

fn offline_method(m: PaymentMethod) -> &'static str {
    match m {
        PaymentMethod::Cash => "in cash at the counter",
        _ => "by card at the counter",
    }
}

fn case_link(role: CaseRole, c: &CaseRef) -> String {
    match role {
        CaseRole::Customer => format!("/account/cases/{}", c.id),
        CaseRole::Restaurant => format!("/partner/cases/{}", c.id),
        CaseRole::Feast => format!("/admin/cases/{}", c.id),
    }
}

fn recipients_for(role: CaseRole, c: &CaseRef) -> Recipients {
    match role {
        CaseRole::Customer => Recipients::User(c.customer_id.clone()),
        CaseRole::Restaurant => Recipients::OwnersOf(c.restaurant.id.clone()),
        CaseRole::Feast => Recipients::Admins,
    }
}

fn case_plan(role: CaseRole, c: &CaseRef, title: String, body: String) -> Plan {
    Plan {
        recipients: recipients_for(role, c),
        draft: NotificationDraft {
            kind: NotificationKind::CaseUpdated,
            title,
            body,
            link: Some(case_link(role, c)),
            restaurant_id: Some(c.restaurant.id.clone()),
        },
    }
}

/// Notifications for grievance, review and moderation events (one per party).
pub fn trust_plans(env: &EventEnvelope) -> Vec<Plan> {
    let owners = |r: &RestaurantRef, kind, title: String, body: String, link: &str| Plan {
        recipients: Recipients::OwnersOf(r.id.clone()),
        draft: NotificationDraft { kind, title, body, link: Some(link.into()), restaurant_id: Some(r.id.clone()) },
    };
    let admins = |r: &RestaurantRef, kind, title: String, body: String, link: &str| Plan {
        recipients: Recipients::Admins,
        draft: NotificationDraft { kind, title, body, link: Some(link.into()), restaurant_id: Some(r.id.clone()) },
    };
    let user = |uid: &str, r: &RestaurantRef, kind, title: String, body: String, link: String| Plan {
        recipients: Recipients::User(uid.to_owned()),
        draft: NotificationDraft { kind, title, body, link: Some(link), restaurant_id: Some(r.id.clone()) },
    };
    match &env.event {
        DomainEvent::CaseOpened { case: c } => {
            let reason = c.reason.label().to_lowercase();
            match c.kind {
                CaseKind::Complaint => vec![case_plan(
                    CaseRole::Restaurant,
                    c,
                    format!("New complaint · {}", c.reason.label()),
                    format!("A diner raised {} ({reason}). Reply within {} hours or Feast steps in.", c.number, feast_domain::trust::RESTAURANT_REPLY_HOURS),
                )],
                CaseKind::Report => vec![
                    case_plan(
                        CaseRole::Customer,
                        c,
                        format!("{} reported a problem with your visit", c.restaurant.name),
                        format!("Report {} ({reason}). Feast will review it — tell us your side.", c.number),
                    ),
                    case_plan(
                        CaseRole::Feast,
                        c,
                        format!("New report · {}", c.reason.label()),
                        format!("{} reported a diner ({reason}). Case {}.", c.restaurant.name, c.number),
                    ),
                ],
            }
        }
        DomainEvent::CaseUpdated { case: c, change, by } => {
            let parties = [CaseRole::Customer, CaseRole::Restaurant, CaseRole::Feast];
            let involved = |r: CaseRole| r != CaseRole::Feast || c.status == CaseStatus::WithFeast;
            let who = match by {
                CaseRole::Customer => "The diner".to_owned(),
                CaseRole::Restaurant => c.restaurant.name.clone(),
                CaseRole::Feast => "Feast".to_owned(),
            };
            let (title, body, roles): (String, String, Vec<CaseRole>) = match change {
                CaseChange::Message => (
                    format!("New reply on {}", c.number),
                    format!("{who} replied about {}.", c.reason.label().to_lowercase()),
                    parties.into_iter().filter(|r| r != by && involved(*r)).collect(),
                ),
                CaseChange::Escalated => (
                    format!("{} is now with Feast", c.number),
                    format!("{who} asked Feast to step in. We aim to decide within {} hours.", feast_domain::trust::FEAST_DECISION_HOURS),
                    parties.into_iter().filter(|r| r != by).collect(),
                ),
                CaseChange::AutoEscalated => (
                    format!("{} is now with Feast", c.number),
                    format!(
                        "{} didn't reply within {} hours, so Feast is handling this complaint.",
                        c.restaurant.name,
                        feast_domain::trust::RESTAURANT_REPLY_HOURS
                    ),
                    parties.to_vec(),
                ),
                CaseChange::Resolved => {
                    (format!("{} resolved", c.number), "See the outcome and Feast's note.".to_owned(), vec![CaseRole::Customer, CaseRole::Restaurant])
                }
                CaseChange::AutoClosed => (
                    format!("{} closed", c.number),
                    format!("No reply from the diner within {} days, so this case was closed as settled.", feast_domain::trust::CUSTOMER_REPLY_DAYS),
                    vec![CaseRole::Customer, CaseRole::Restaurant],
                ),
            };
            roles.into_iter().map(|r| case_plan(r, c, title.clone(), body.clone())).collect()
        }
        DomainEvent::ReviewPosted { restaurant: r, rating, author_name, .. } => vec![owners(
            r,
            NotificationKind::ReviewUpdated,
            format!("New {rating}★ review"),
            format!("{author_name} reviewed {}. Reply to thank them or respond.", r.name),
            "/partner/reviews",
        )],
        DomainEvent::ReviewReplied { restaurant: r, user_id, .. } => vec![user(
            user_id,
            r,
            NotificationKind::ReviewUpdated,
            format!("{} replied to your review", r.name),
            "Tap to read their reply.".into(),
            format!("/restaurants/{}#reviews", r.slug),
        )],
        DomainEvent::ReviewReported { restaurant: r, reason, .. } => {
            vec![admins(r, NotificationKind::ReviewUpdated, format!("Review reported · {}", r.name), reason.chars().take(160).collect(), "/admin/reviews")]
        }
        DomainEvent::ReviewModerated { restaurant: r, user_id, hidden, reason, .. } => {
            let why = reason.clone().unwrap_or_default();
            vec![
                user(
                    user_id,
                    r,
                    NotificationKind::ReviewUpdated,
                    if *hidden { format!("Your review of {} was hidden", r.name) } else { format!("Your review of {} is visible again", r.name) },
                    if *hidden { format!("It breaks Feast's review guidelines: {why}") } else { "Thanks for your patience.".into() },
                    "/account".into(),
                ),
                owners(
                    r,
                    NotificationKind::ReviewUpdated,
                    if *hidden { "A review was hidden".into() } else { "A review was restored".into() },
                    if *hidden { format!("Feast hid a review: {why}") } else { "Feast restored a review after checking it.".into() },
                    "/partner/reviews",
                ),
            ]
        }
        DomainEvent::ContentModerated { restaurant: r, kind, title, hidden, reason } => {
            let what = match kind {
                ContentKind::MenuItem => "menu item",
                ContentKind::Offer => "offer",
            };
            let link = match kind {
                ContentKind::MenuItem => "/partner/menu",
                ContentKind::Offer => "/partner/offers",
            };
            vec![owners(
                r,
                NotificationKind::ContentModerated,
                if *hidden { format!("Feast hid your {what} \"{title}\"") } else { format!("Your {what} \"{title}\" is visible again") },
                if *hidden {
                    format!("{} Fix it, then ask Feast to review it again.", reason.clone().unwrap_or_default())
                } else {
                    "Diners can see it again.".into()
                },
                link,
            )]
        }
        DomainEvent::ModerationAppealed { restaurant: r, title, note, .. } => vec![admins(
            r,
            NotificationKind::ContentModerated,
            format!("{} asks to restore \"{title}\"", r.name),
            note.clone().unwrap_or_else(|| "The owner says it's fixed.".into()),
            "/admin/moderation",
        )],
        DomainEvent::SupportAccessChanged { restaurant: r, until: Some(t) } => vec![admins(
            r,
            NotificationKind::ContentModerated,
            format!("{} granted support access", r.name),
            format!("Feast staff can edit {} until {} (UTC). Every change is recorded.", r.name, t[..16.min(t.len())].replace('T', " ")),
            "/admin/restaurants",
        )],
        _ => vec![],
    }
}

impl Feast {
    /// Turn a domain event into notifications. Idempotent (keyed on event id).
    pub async fn project_notification(&self, env: &EventEnvelope) -> AppResult<usize> {
        let mut sent = 0;
        for plan in plan_for(env).into_iter().chain(trust_plans(env)) {
            let recipients: Vec<String> = match plan.recipients {
                Recipients::OwnersOf(rid) => self.p.users.owners_of(&rid).await?.into_iter().map(|u| u.id).collect(),
                Recipients::User(uid) => vec![uid],
                Recipients::Admins => self.p.users.list().await?.into_iter().filter(|u| u.role == Role::Admin && u.is_active()).map(|u| u.id).collect(),
            };
            if recipients.is_empty() {
                continue;
            }
            sent += self.p.notifications.publish(&env.id, env.type_name(), &recipients, &plan.draft).await?.len();
        }
        Ok(sent)
    }

    pub async fn my_notifications(&self, user: &User, unread_only: bool, limit: u32) -> AppResult<NotificationList> {
        self.p.notifications.list(&user.id, unread_only, limit.clamp(1, 100)).await
    }

    pub async fn mark_notifications_read(&self, user: &User, ids: &[String]) -> AppResult<i64> {
        self.p.notifications.mark_read(&user.id, ids).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_per_currency() {
        assert_eq!(fmt_money(101_800, "INR"), "₹1,018");
        assert_eq!(fmt_money(1_234_567_800, "INR"), "₹1,23,45,678");
        assert_eq!(fmt_money(6_925, "INR"), "₹69.25");
        assert!(fmt_money(4_550, "AUD").contains("45.50"));
    }

    #[test]
    fn check_in_tells_the_restaurant() {
        let booking: Booking = serde_json::from_value(serde_json::json!({
            "id": "b_1", "userId": "u_1", "code": "FST123",
            "restaurant": { "id": "r_1", "slug": "r-1", "name": "Pho Republic", "area": "Newtown", "art": { "hue": 1, "motif": "noodles" } },
            "date": "2026-10-25", "time": "19:30", "guests": 2,
            "contact": { "name": "Bea", "phone": "+61400000000" },
            "status": "CONFIRMED", "coverCharge": 0, "currency": "AUD", "createdAt": "2026-10-25T08:00:00Z", "checkedInAt": "2026-10-25T08:20:00Z"
        }))
        .unwrap();
        let env = EventEnvelope::new(
            "evt_1".into(),
            "2026-10-25T08:20:00Z".into(),
            Some("r_1".into()),
            Some("u_1".into()),
            DomainEvent::BookingCheckedIn { booking },
        );
        let plan = plan_for(&env).expect("owners are told");
        assert!(matches!(plan.recipients, Recipients::OwnersOf(ref r) if r == "r_1"));
        assert_eq!(plan.draft.title, "Bea has arrived");
        assert!(plan.draft.body.contains("FST123") && plan.draft.body.contains("7:30 PM"));
    }

    #[test]
    fn time_format() {
        assert_eq!(fmt_time("19:30"), "7:30 PM");
        assert_eq!(fmt_time("00:05"), "12:05 AM");
        assert_eq!(fmt_time("12:00"), "12:00 PM");
    }
}
