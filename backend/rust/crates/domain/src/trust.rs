//! Trust & safety: content moderation, grievance cases between diners and
//! restaurants, ratings & reviews, and the audit trail of Feast staff actions.
//!
//! Principles:
//! - **Restaurants own their content** (menu, listing, offers). Feast staff can
//!   *see* it and *moderate* it (hide one item/offer with a reason the owner
//!   sees), but only edit it while the owner has granted time-limited support
//!   access — and every such edit is recorded in the audit log.
//! - **Grievances go to the other party first.** A diner's complaint goes to the
//!   restaurant, which has [`RESTAURANT_REPLY_HOURS`] to answer; either side can
//!   escalate to Feast. A restaurant's report about a diner (no-show, abuse…)
//!   goes straight to Feast, and the diner can give their side.
//! - **Reviews are verified**: only for a real visit (completed booking or a bill
//!   paid on Feast), one per visit.
//!
//! Everything here is pure (no I/O) so the rules are unit-tested and portable.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::model::{IsoDate, IsoDateTime, Paise, RestaurantRef, Role};

// ================================================================ moderation

/// Set on a menu item or offer that Feast has hidden from diners.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Moderation {
    /// Shown to the owner: what to fix.
    pub reason: String,
    pub hidden_at: IsoDateTime,
    /// Admin user id.
    pub hidden_by: String,
    /// The owner says it's fixed and asked Feast to look again.
    pub review_requested_at: Option<IsoDateTime>,
    pub review_note: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContentKind {
    MenuItem,
    Offer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModerationAction {
    Hide,
    Restore,
}

/// Admin hides or restores one menu item / offer.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModerateContentRequest {
    pub kind: ContentKind,
    pub id: String,
    pub action: ModerationAction,
    /// Required to hide.
    pub reason: Option<String>,
}

/// Owner asks Feast to review a hidden item/offer again.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModerationAppealRequest {
    pub kind: ContentKind,
    pub id: String,
    pub note: Option<String>,
}

/// A hidden item/offer (admin moderation queue).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModeratedContent {
    pub restaurant: RestaurantRef,
    pub kind: ContentKind,
    pub id: String,
    pub title: String,
    pub moderation: Moderation,
}

pub const MIN_REASON_CHARS: usize = 10;

// ================================================================ support access & audit

/// Longest support access an owner can grant at once.
pub const MAX_SUPPORT_HOURS: i64 = 72;

/// Owner grants (hours 1..=72) or revokes (0) Feast support access.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SupportAccessRequest {
    pub hours: i64,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SupportAccess {
    /// Feast staff may edit until this instant (absent = no access).
    pub active_until: Option<IsoDateTime>,
    /// Whether Feast staff may edit this restaurant right now (access granted,
    /// or no owner linked yet).
    pub staff_can_edit: bool,
    /// Newest first: every Feast staff action on this restaurant.
    pub audit: Vec<AuditEntry>,
}

/// One recorded action by Feast staff (or an owner granting access).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: String,
    pub restaurant_id: String,
    pub actor_id: String,
    pub actor_name: String,
    pub actor_role: Role,
    /// Stable machine name, e.g. `menu.item_updated`, `moderation.hidden`.
    pub action: String,
    /// Human-readable, e.g. "Butter Chicken: price ₹420 → ₹450".
    pub summary: String,
    pub at: IsoDateTime,
}

/// Whether support access granted until `until` is active at `now`.
pub fn support_active(until: Option<&str>, now: DateTime<Utc>) -> bool {
    until.and_then(|u| DateTime::parse_from_rfc3339(u).ok()).is_some_and(|u| u.with_timezone(&Utc) > now)
}

// ================================================================ grievance cases

/// Hours a restaurant has to answer a complaint before Feast takes over.
pub const RESTAURANT_REPLY_HOURS: i64 = 48;
/// Days a diner has to answer the restaurant before the case closes as settled.
pub const CUSTOMER_REPLY_DAYS: i64 = 7;
/// Feast's target to decide an escalated case.
pub const FEAST_DECISION_HOURS: i64 = 72;
/// A grievance must be filed within this many days of the visit.
pub const FILE_WITHIN_DAYS: i64 = 30;
pub const MAX_CASE_ATTACHMENTS: usize = 3;
pub const MAX_MESSAGE_CHARS: usize = 2000;
pub const MIN_MESSAGE_CHARS: usize = 10;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CaseKind {
    /// A diner's complaint about a restaurant.
    Complaint,
    /// A restaurant's report about a diner.
    Report,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CaseStatus {
    AwaitingRestaurant,
    AwaitingCustomer,
    /// Escalated: Feast decides.
    WithFeast,
    Resolved,
}

impl CaseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingRestaurant => "AWAITING_RESTAURANT",
            Self::AwaitingCustomer => "AWAITING_CUSTOMER",
            Self::WithFeast => "WITH_FEAST",
            Self::Resolved => "RESOLVED",
        }
    }
    pub fn is_open(self) -> bool {
        self != Self::Resolved
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CaseReason {
    // Diner → restaurant
    TableNotHonoured,
    OfferNotHonoured,
    WrongBill,
    FoodQuality,
    Hygiene,
    StaffBehaviour,
    // Restaurant → diner
    NoShow,
    AbusiveBehaviour,
    PaymentDispute,
    PropertyDamage,
    // Either
    Other,
}

impl CaseReason {
    pub fn allowed_for(self, kind: CaseKind) -> bool {
        use CaseReason::*;
        match kind {
            CaseKind::Complaint => matches!(self, TableNotHonoured | OfferNotHonoured | WrongBill | FoodQuality | Hygiene | StaffBehaviour | Other),
            CaseKind::Report => matches!(self, NoShow | AbusiveBehaviour | PaymentDispute | PropertyDamage | Other),
        }
    }
    pub fn label(self) -> &'static str {
        use CaseReason::*;
        match self {
            TableNotHonoured => "Table not honoured",
            OfferNotHonoured => "Offer not honoured",
            WrongBill => "Wrong bill",
            FoodQuality => "Food quality",
            Hygiene => "Hygiene",
            StaffBehaviour => "Staff behaviour",
            NoShow => "No-show",
            AbusiveBehaviour => "Abusive behaviour",
            PaymentDispute => "Payment dispute",
            PropertyDamage => "Property damage",
            Other => "Other",
        }
    }
}

/// Who wrote a message / acted on a case.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CaseRole {
    Customer,
    Restaurant,
    Feast,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaseMessage {
    pub id: String,
    pub author: CaseRole,
    pub author_name: String,
    pub text: String,
    pub at: IsoDateTime,
    /// Feast-only note, never shown to the diner or the restaurant.
    #[serde(default)]
    pub internal: bool,
    /// Written by the system (auto-escalation, auto-close).
    #[serde(default)]
    pub system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaseAttachment {
    pub id: String,
    pub file_name: String,
    pub content_type: String,
    pub size: i64,
    pub uploaded_by: CaseRole,
    pub at: IsoDateTime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CaseDecision {
    /// The diner marked it resolved, or it closed after no reply.
    SettledBetweenParties,
    NoAction,
    /// Feast asks the restaurant to refund the diner.
    RefundAdvised,
    RestaurantWarned,
    /// The restaurant is hidden from diners until Feast reinstates it.
    RestaurantSuspended,
    CustomerWarned,
    /// The diner's account is blocked.
    CustomerBlocked,
}

impl CaseDecision {
    pub fn label(self) -> &'static str {
        match self {
            Self::SettledBetweenParties => "Settled",
            Self::NoAction => "No action",
            Self::RefundAdvised => "Refund advised",
            Self::RestaurantWarned => "Restaurant warned",
            Self::RestaurantSuspended => "Restaurant suspended",
            Self::CustomerWarned => "Diner warned",
            Self::CustomerBlocked => "Diner blocked",
        }
    }
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaseOutcome {
    pub decision: CaseDecision,
    /// Shown to both parties.
    pub note: String,
    pub refund_amount: Option<Paise>,
    pub decided_by: CaseRole,
    pub at: IsoDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaseParty {
    pub id: String,
    pub name: String,
}

/// A grievance between a diner and a restaurant about one visit.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Case {
    pub id: String,
    /// Human reference, e.g. `FC-7KQ2M9`.
    pub number: String,
    pub kind: CaseKind,
    pub status: CaseStatus,
    pub reason: CaseReason,
    pub restaurant: RestaurantRef,
    pub customer: CaseParty,
    pub booking_id: Option<String>,
    pub bill_payment_id: Option<String>,
    pub visit_date: IsoDate,
    /// The bill or cover charge involved, if any.
    pub amount: Option<Paise>,
    pub currency: String,
    pub messages: Vec<CaseMessage>,
    #[serde(default)]
    pub attachments: Vec<CaseAttachment>,
    pub outcome: Option<CaseOutcome>,
    /// When the party whose turn it is must respond (Feast's target when WITH_FEAST).
    pub due_at: Option<IsoDateTime>,
    pub escalated_at: Option<IsoDateTime>,
    pub resolved_at: Option<IsoDateTime>,
    pub created_at: IsoDateTime,
    pub updated_at: IsoDateTime,
    /// Bumped on every change (optimistic locking).
    #[serde(default)]
    pub version: i64,
}

/// Slim case reference carried in events (never the messages: internal notes
/// must not travel to parties' live streams).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaseRef {
    pub id: String,
    pub number: String,
    pub kind: CaseKind,
    pub status: CaseStatus,
    pub reason: CaseReason,
    pub restaurant: RestaurantRef,
    pub customer_id: String,
}

/// What just happened to a case (drives notifications).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CaseChange {
    Message,
    Escalated,
    /// Nobody from the restaurant answered in time.
    AutoEscalated,
    Resolved,
    /// The diner didn't answer the restaurant in time.
    AutoClosed,
}

impl Case {
    pub fn reference(&self) -> CaseRef {
        CaseRef {
            id: self.id.clone(),
            number: self.number.clone(),
            kind: self.kind,
            status: self.status,
            reason: self.reason,
            restaurant: self.restaurant.clone(),
            customer_id: self.customer.id.clone(),
        }
    }

    /// The case as a diner or restaurant sees it (no Feast-internal notes).
    pub fn for_party(mut self) -> Self {
        self.messages.retain(|m| !m.internal);
        self
    }

    /// Whether `now` is past the response deadline.
    pub fn overdue(&self, now: DateTime<Utc>) -> bool {
        self.status.is_open() && self.due_at.as_deref().and_then(parse).is_some_and(|d| d <= now)
    }
}

fn parse(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

pub fn iso(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Initial status and deadline of a new case.
pub fn opening(kind: CaseKind, now: DateTime<Utc>) -> (CaseStatus, IsoDateTime) {
    match kind {
        CaseKind::Complaint => (CaseStatus::AwaitingRestaurant, iso(now + Duration::hours(RESTAURANT_REPLY_HOURS))),
        CaseKind::Report => (CaseStatus::WithFeast, iso(now + Duration::hours(FEAST_DECISION_HOURS))),
    }
}

/// Why a case action isn't allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseRuleError {
    pub code: &'static str,
    pub message: &'static str,
}

const fn err(code: &'static str, message: &'static str) -> CaseRuleError {
    CaseRuleError { code, message }
}

/// A party (or Feast) adds a message: the turn passes to the other side.
pub fn after_message(c: &Case, author: CaseRole, internal: bool, now: DateTime<Utc>) -> Result<(CaseStatus, Option<IsoDateTime>), CaseRuleError> {
    if c.status == CaseStatus::Resolved {
        return Err(err("CASE_RESOLVED", "This case is resolved. Open a new one if something else went wrong."));
    }
    if internal && author != CaseRole::Feast {
        return Err(err("FORBIDDEN", "Only Feast can add internal notes"));
    }
    if internal || c.kind == CaseKind::Report || c.status == CaseStatus::WithFeast {
        return Ok((c.status, c.due_at.clone()));
    }
    Ok(match (c.status, author) {
        (CaseStatus::AwaitingRestaurant, CaseRole::Restaurant) => (CaseStatus::AwaitingCustomer, Some(iso(now + Duration::days(CUSTOMER_REPLY_DAYS)))),
        (CaseStatus::AwaitingCustomer, CaseRole::Customer) => (CaseStatus::AwaitingRestaurant, Some(iso(now + Duration::hours(RESTAURANT_REPLY_HOURS)))),
        _ => (c.status, c.due_at.clone()),
    })
}

/// A party hands the case to Feast.
pub fn escalate(c: &Case, by: CaseRole, now: DateTime<Utc>) -> Result<(CaseStatus, IsoDateTime), CaseRuleError> {
    if by == CaseRole::Feast {
        return Err(err("FORBIDDEN", "Feast takes over by replying"));
    }
    match c.status {
        CaseStatus::Resolved => Err(err("CASE_RESOLVED", "This case is resolved")),
        CaseStatus::WithFeast => Err(err("ALREADY_ESCALATED", "Feast is already looking into this")),
        _ => Ok((CaseStatus::WithFeast, iso(now + Duration::hours(FEAST_DECISION_HOURS)))),
    }
}

/// The diner says their complaint is sorted.
pub fn settle_by_customer(c: &Case) -> Result<(), CaseRuleError> {
    if c.kind != CaseKind::Complaint {
        return Err(err("FORBIDDEN", "Only Feast can close a restaurant's report"));
    }
    if c.status == CaseStatus::Resolved {
        return Err(err("CASE_RESOLVED", "This case is already resolved"));
    }
    Ok(())
}

/// Feast's decision is valid for this kind of case.
pub fn decision_allowed(kind: CaseKind, d: CaseDecision) -> bool {
    use CaseDecision::*;
    match kind {
        CaseKind::Complaint => matches!(d, NoAction | RefundAdvised | RestaurantWarned | RestaurantSuspended | CustomerWarned),
        CaseKind::Report => matches!(d, NoAction | CustomerWarned | CustomerBlocked | RestaurantWarned),
    }
}

/// What the worker does to an overdue case.
pub fn overdue_transition(c: &Case, now: DateTime<Utc>) -> Option<CaseChange> {
    if !c.overdue(now) {
        return None;
    }
    match c.status {
        CaseStatus::AwaitingRestaurant => Some(CaseChange::AutoEscalated),
        CaseStatus::AwaitingCustomer => Some(CaseChange::AutoClosed),
        _ => None, // Feast's own SLA is shown as overdue, never auto-decided
    }
}

/// Request: open a case about a visit (exactly one of booking / bill payment).
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenCaseRequest {
    pub booking_id: Option<String>,
    pub bill_payment_id: Option<String>,
    pub reason: CaseReason,
    pub text: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaseMessageRequest {
    pub text: String,
    /// Admin only: a note the parties never see.
    pub internal: Option<bool>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResolveCaseRequest {
    pub decision: CaseDecision,
    pub note: String,
    pub refund_amount: Option<Paise>,
}

/// A case with the history Feast needs to decide fairly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdminCaseItem {
    pub case: Case,
    pub overdue: bool,
    /// Complaints against this restaurant (all time, including this one).
    pub restaurant_complaints: i64,
    /// Reports about this diner (all time, including this one).
    pub customer_reports: i64,
}

// ================================================================ reviews

/// A review must be written within this many days of the visit.
pub const REVIEW_WITHIN_DAYS: i64 = 60;
pub const MAX_REVIEW_CHARS: usize = 1000;
pub const MAX_REPLY_CHARS: usize = 600;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewStatus {
    Published,
    /// Hidden by Feast (reason in `moderation`).
    Hidden,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReply {
    pub text: String,
    pub at: IsoDateTime,
}

/// The restaurant reported a review to Feast.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFlag {
    pub reason: String,
    pub at: IsoDateTime,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub id: String,
    pub restaurant: RestaurantRef,
    /// Absent in public lists.
    pub user_id: Option<String>,
    /// "Kavya R."
    pub author_name: String,
    pub booking_id: Option<String>,
    pub bill_payment_id: Option<String>,
    pub visit_date: IsoDate,
    /// 1–5 stars.
    pub rating: i64,
    pub text: String,
    pub created_at: IsoDateTime,
    pub reply: Option<ReviewReply>,
    pub status: ReviewStatus,
    pub moderation: Option<Moderation>,
    pub flag: Option<ReviewFlag>,
}

impl Review {
    /// What any visitor sees.
    pub fn public(mut self) -> Self {
        self.user_id = None;
        self.booking_id = None;
        self.bill_payment_id = None;
        self.moderation = None;
        self.flag = None;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSummary {
    /// Shown rating (blends ratings from before Feast reviews with new ones).
    pub average: f64,
    pub count: i64,
    /// Published Feast reviews by stars: index 0 = 1★ … 4 = 5★.
    pub distribution: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewList {
    pub summary: ReviewSummary,
    pub reviews: Vec<Review>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateReviewRequest {
    pub booking_id: Option<String>,
    pub bill_payment_id: Option<String>,
    pub rating: i64,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReplyRequest {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReportReviewRequest {
    pub reason: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModerateReviewRequest {
    pub action: ModerationAction,
    pub reason: Option<String>,
}

/// The rating a restaurant had before Feast reviews (imported/seeded), kept so
/// new reviews blend into it instead of replacing it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RatingBase {
    pub average: f64,
    pub count: i64,
}

/// Blend the base rating with Feast reviews (`sum` of stars over `count`),
/// rounded to one decimal.
pub fn blended_rating(base: RatingBase, sum: i64, count: i64) -> (f64, i64) {
    let total = base.count.max(0) + count.max(0);
    if total == 0 {
        return (0.0, 0);
    }
    let avg = (base.average * base.count.max(0) as f64 + sum as f64) / total as f64;
    ((avg * 10.0).round() / 10.0, total)
}

/// "Kavya Rao" → "Kavya R."
pub fn short_name(full: &str) -> String {
    let mut parts = full.split_whitespace();
    match (parts.next(), parts.last()) {
        (Some(first), Some(last)) => format!("{first} {}.", last.chars().next().unwrap_or_default().to_uppercase()),
        (Some(first), None) => first.to_owned(),
        _ => "Feast diner".into(),
    }
}

/// Whether a visit on `visit_date` is still within `days` of `today` (both `YYYY-MM-DD`).
pub fn within_days(visit_date: &str, today: &str, days: i64) -> bool {
    let p = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    match (p(visit_date), p(today)) {
        (Some(v), Some(t)) => v <= t && (t - v).num_days() <= days,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Art, CuisineMotif};

    fn t(s: &str) -> DateTime<Utc> {
        parse(s).unwrap()
    }

    fn case(kind: CaseKind, now: DateTime<Utc>) -> Case {
        let (status, due) = opening(kind, now);
        Case {
            id: "case_1".into(),
            number: "FC-1".into(),
            kind,
            status,
            reason: if kind == CaseKind::Complaint { CaseReason::WrongBill } else { CaseReason::NoShow },
            restaurant: RestaurantRef {
                id: "r".into(),
                slug: "r".into(),
                name: "R".into(),
                area: "A".into(),
                art: Art { hue: 1, motif: CuisineMotif::Cafe, image: None, image_credit: None },
            },
            customer: CaseParty { id: "u".into(), name: "U".into() },
            booking_id: Some("bk".into()),
            bill_payment_id: None,
            visit_date: "2026-10-20".into(),
            amount: None,
            currency: "AUD".into(),
            messages: vec![],
            attachments: vec![],
            outcome: None,
            due_at: Some(due),
            escalated_at: None,
            resolved_at: None,
            created_at: iso(now),
            updated_at: iso(now),
            version: 1,
        }
    }

    #[test]
    fn complaint_turns_pass_between_parties() {
        let now = t("2026-10-24T10:00:00Z");
        let mut c = case(CaseKind::Complaint, now);
        assert_eq!(c.status, CaseStatus::AwaitingRestaurant);
        assert_eq!(c.due_at.as_deref(), Some("2026-10-26T10:00:00.000Z"));

        // Diner adds detail: still the restaurant's turn, deadline unchanged.
        assert_eq!(after_message(&c, CaseRole::Customer, false, now).unwrap(), (CaseStatus::AwaitingRestaurant, c.due_at.clone()));
        // Restaurant replies: diner's turn for 7 days.
        let (s, due) = after_message(&c, CaseRole::Restaurant, false, now).unwrap();
        assert_eq!((s, due.as_deref()), (CaseStatus::AwaitingCustomer, Some("2026-10-31T10:00:00.000Z")));
        c.status = s;
        c.due_at = due;
        // Diner answers: back to the restaurant.
        assert_eq!(after_message(&c, CaseRole::Customer, false, now).unwrap().0, CaseStatus::AwaitingRestaurant);
        // Internal notes are Feast-only and never move the turn.
        assert_eq!(after_message(&c, CaseRole::Customer, true, now).unwrap_err().code, "FORBIDDEN");
        assert_eq!(after_message(&c, CaseRole::Feast, true, now).unwrap().0, CaseStatus::AwaitingCustomer);
    }

    #[test]
    fn escalation_and_deadlines() {
        let now = t("2026-10-24T10:00:00Z");
        let c = case(CaseKind::Complaint, now);
        assert!(overdue_transition(&c, now).is_none());
        assert_eq!(overdue_transition(&c, t("2026-10-26T10:00:01Z")), Some(CaseChange::AutoEscalated));
        let (s, due) = escalate(&c, CaseRole::Customer, now).unwrap();
        assert_eq!((s, due.as_str()), (CaseStatus::WithFeast, "2026-10-27T10:00:00.000Z"));
        let mut w = c.clone();
        w.status = CaseStatus::WithFeast;
        w.due_at = Some(due);
        assert_eq!(escalate(&w, CaseRole::Restaurant, now).unwrap_err().code, "ALREADY_ESCALATED");
        // Feast's own deadline is flagged, never auto-decided.
        assert!(w.overdue(t("2026-10-28T00:00:00Z")));
        assert!(overdue_transition(&w, t("2026-10-28T00:00:00Z")).is_none());

        let mut a = c.clone();
        a.status = CaseStatus::AwaitingCustomer;
        a.due_at = Some("2026-10-31T10:00:00.000Z".into());
        assert_eq!(overdue_transition(&a, t("2026-11-01T00:00:00Z")), Some(CaseChange::AutoClosed));
    }

    #[test]
    fn reports_go_straight_to_feast() {
        let now = t("2026-10-24T10:00:00Z");
        let r = case(CaseKind::Report, now);
        assert_eq!(r.status, CaseStatus::WithFeast);
        assert_eq!(after_message(&r, CaseRole::Customer, false, now).unwrap().0, CaseStatus::WithFeast);
        assert!(settle_by_customer(&r).is_err());
        assert!(decision_allowed(CaseKind::Report, CaseDecision::CustomerBlocked));
        assert!(!decision_allowed(CaseKind::Complaint, CaseDecision::CustomerBlocked));
        assert!(!decision_allowed(CaseKind::Report, CaseDecision::RestaurantSuspended));
        assert!(CaseReason::NoShow.allowed_for(CaseKind::Report));
        assert!(!CaseReason::NoShow.allowed_for(CaseKind::Complaint));
        assert!(CaseReason::Other.allowed_for(CaseKind::Complaint));
    }

    #[test]
    fn resolved_cases_are_closed() {
        let now = t("2026-10-24T10:00:00Z");
        let mut c = case(CaseKind::Complaint, now);
        c.status = CaseStatus::Resolved;
        assert_eq!(after_message(&c, CaseRole::Customer, false, now).unwrap_err().code, "CASE_RESOLVED");
        assert!(escalate(&c, CaseRole::Customer, now).is_err());
        assert!(!c.overdue(t("2030-01-01T00:00:00Z")));
    }

    #[test]
    fn parties_never_see_internal_notes() {
        let now = t("2026-10-24T10:00:00Z");
        let mut c = case(CaseKind::Complaint, now);
        let m = |internal| CaseMessage {
            id: "m".into(),
            author: CaseRole::Feast,
            author_name: "Feast".into(),
            text: "x".into(),
            at: iso(now),
            internal,
            system: false,
        };
        c.messages = vec![m(false), m(true)];
        assert_eq!(c.for_party().messages.len(), 1);
    }

    #[test]
    fn ratings_blend_and_round() {
        let base = RatingBase { average: 4.4, count: 1200 };
        assert_eq!(blended_rating(base, 0, 0), (4.4, 1200));
        assert_eq!(blended_rating(base, 1, 1), (4.4, 1201));
        assert_eq!(blended_rating(RatingBase { average: 0.0, count: 0 }, 9, 2), (4.5, 2));
        assert_eq!(blended_rating(RatingBase { average: 0.0, count: 0 }, 0, 0), (0.0, 0));
    }

    #[test]
    fn names_dates_and_access() {
        assert_eq!(short_name("Kavya Rao"), "Kavya R.");
        assert_eq!(short_name("Chloe  van der Berg"), "Chloe B.");
        assert_eq!(short_name("Jack"), "Jack");
        assert_eq!(short_name("  "), "Feast diner");
        assert!(within_days("2026-10-01", "2026-10-24", 30));
        assert!(!within_days("2026-08-01", "2026-10-24", 30));
        assert!(!within_days("2026-10-25", "2026-10-24", 30), "future visits can't be reviewed");
        let now = t("2026-10-24T10:00:00Z");
        assert!(support_active(Some("2026-10-24T11:00:00.000Z"), now));
        assert!(!support_active(Some("2026-10-24T09:00:00.000Z"), now));
        assert!(!support_active(None, now));
    }
}
