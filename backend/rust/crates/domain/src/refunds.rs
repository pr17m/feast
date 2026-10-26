//! Refunds of money Feast took through its payment provider. Pure rules; the
//! application layer applies them and the provider adapters move the money.
//!
//! ```text
//! PENDING ──provider confirms──▶ SUCCEEDED   (money on its way back to the card)
//!    └──────provider refuses───▶ FAILED      (support follows up)
//! ```
//!
//! A refund is created once per reason (`idempotency_key`), never for more than
//! what is left of the payment, and only for payments Feast settled online.
//! Counter (offline) payments never went through Feast, so Feast never refunds
//! them: that is between the diner and the restaurant.

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::events::Party;
use crate::model::{IsoDateTime, Paise, PaymentPurpose};

/// A diner who cancels at least this long before the slot gets an upfront
/// table charge back in full (the refunds policy page says the same).
pub const FREE_CANCEL_HOURS: i64 = 2;

/// Shortest note an admin must give for a manual refund.
pub const MIN_NOTE_CHARS: usize = 10;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RefundStatus {
    Pending,
    Succeeded,
    Failed,
}

impl RefundStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Succeeded => "SUCCEEDED",
            Self::Failed => "FAILED",
        }
    }
    /// Money that is (or will be) going back: counts against what is refundable.
    pub fn counts(self) -> bool {
        matches!(self, Self::Pending | Self::Succeeded)
    }
}

/// Why money goes back.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RefundReason {
    /// The diner cancelled at least `FREE_CANCEL_HOURS` before the slot.
    CancelledInTime,
    /// The restaurant (or Feast) cancelled the booking.
    CancelledByRestaurant,
    /// The diner deleted their account; upcoming paid bookings were cancelled in time.
    AccountDeleted,
    /// The payment arrived after the booking was cancelled, expired or its table was gone.
    PaidAfterCancellation,
    /// The bill had already been paid (another payment, or at the counter).
    AlreadyPaid,
    /// Feast decided a complaint in the diner's favour.
    CaseDecision,
    /// An admin refunded it by hand.
    Manual,
}

impl RefundReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::CancelledInTime => "Cancelled in time",
            Self::CancelledByRestaurant => "Cancelled by the restaurant",
            Self::AccountDeleted => "Account deleted",
            Self::PaidAfterCancellation => "Paid after the booking was cancelled",
            Self::AlreadyPaid => "The bill was already paid",
            Self::CaseDecision => "Feast's decision on a complaint",
            Self::Manual => "Refunded by Feast",
        }
    }
}

/// One refund of (part of) an online payment.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Refund {
    pub id: String,
    /// The payment intent refunded.
    pub intent_id: String,
    pub purpose: PaymentPurpose,
    /// Booking or bill the payment was for.
    pub reference_id: String,
    /// The diner who gets the money back.
    pub user_id: Option<String>,
    pub restaurant_id: Option<String>,
    pub amount: Paise,
    pub currency: String,
    pub provider: String,
    /// The provider's refund id once it accepted the request.
    pub provider_refund_id: Option<String>,
    pub status: RefundStatus,
    pub reason: RefundReason,
    /// One refund per reason: e.g. `booking-cancel:<booking id>`.
    pub idempotency_key: String,
    pub note: Option<String>,
    /// Complaint case that decided it.
    pub case_id: Option<String>,
    /// User id of the admin (or the system) who asked for it.
    pub requested_by: Option<String>,
    /// Why the provider refused, or why it hasn't been sent yet.
    pub failure: Option<String>,
    /// Provider calls made (retried by the sweeper while PENDING and unsent).
    #[serde(default)]
    pub attempts: i64,
    pub created_at: IsoDateTime,
    pub updated_at: IsoDateTime,
}

/// Admin: refund (part of) a payment by hand.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RefundRequest {
    pub intent_id: String,
    /// Minor units; omitted = everything still refundable.
    pub amount: Option<Paise>,
    pub note: String,
    pub case_id: Option<String>,
}

/// Does cancelling this paid booking give the upfront charge back?
/// Restaurant/Feast cancellations always do; a diner's only when it is at
/// least `FREE_CANCEL_HOURS` before the slot.
pub fn booking_cancel_refund(by: Party, slot_start_ms: Option<i64>, now_ms: i64) -> Option<RefundReason> {
    match by {
        Party::Restaurant | Party::Admin => Some(RefundReason::CancelledByRestaurant),
        Party::Customer => match slot_start_ms {
            Some(at) if at - now_ms >= FREE_CANCEL_HOURS * 3_600_000 => Some(RefundReason::CancelledInTime),
            Some(_) => None,
            // Unknown slot time: be generous.
            None => Some(RefundReason::CancelledInTime),
        },
    }
}

/// What can still be refunded of `paid` given earlier refunds (pending or done).
pub fn refundable(paid: Paise, refunds: &[Refund]) -> Paise {
    let taken: Paise = refunds.iter().filter(|r| r.status.counts()).map(|r| r.amount).sum();
    (paid - taken).max(0)
}

/// Validate a requested refund amount against what is left.
pub fn check_amount(requested: Option<Paise>, left: Paise) -> Result<Paise, String> {
    if left <= 0 {
        return Err("This payment has already been refunded in full".into());
    }
    match requested {
        None => Ok(left),
        Some(a) if a <= 0 => Err("Enter an amount to refund".into()),
        Some(a) if a > left => Err("The refund can't be more than what's left of the payment".into()),
        Some(a) => Ok(a),
    }
}

/// Can a payment that just succeeded confirm its booking? Only while the
/// booking still awaits payment (`status` as stored, e.g. "PENDING_PAYMENT"),
/// and — when its table hold lapsed — only if a table is still free. Otherwise
/// the money is refunded: a cancelled or expired booking is never brought back.
pub fn booking_payment_applies(status: &str, hold_lapsed: bool, table_free: bool) -> bool {
    status == "PENDING_PAYMENT" && (!hold_lapsed || table_free)
}

/// A provider's verdict can't move a refund backwards: a final refund stays
/// final (webhooks may arrive late or twice).
pub fn may_move(from: RefundStatus, to: RefundStatus) -> bool {
    from == RefundStatus::Pending && to != RefundStatus::Pending
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(amount: Paise, status: RefundStatus) -> Refund {
        Refund {
            id: "rf".into(),
            intent_id: "pi".into(),
            purpose: PaymentPurpose::Booking,
            reference_id: "b".into(),
            user_id: None,
            restaurant_id: None,
            amount,
            currency: "AUD".into(),
            provider: "mock".into(),
            provider_refund_id: None,
            status,
            reason: RefundReason::Manual,
            idempotency_key: "k".into(),
            note: None,
            case_id: None,
            requested_by: None,
            failure: None,
            attempts: 0,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn cancellation_rule() {
        let h = 3_600_000;
        assert_eq!(booking_cancel_refund(Party::Customer, Some(3 * h), 0), Some(RefundReason::CancelledInTime));
        assert_eq!(booking_cancel_refund(Party::Customer, Some(2 * h), 0), Some(RefundReason::CancelledInTime), "exactly 2 h counts");
        assert_eq!(booking_cancel_refund(Party::Customer, Some(2 * h - 1), 0), None, "late cancellation: the restaurant keeps it");
        assert_eq!(booking_cancel_refund(Party::Restaurant, Some(0), 10 * h), Some(RefundReason::CancelledByRestaurant));
        assert_eq!(booking_cancel_refund(Party::Admin, Some(0), 10 * h), Some(RefundReason::CancelledByRestaurant));
    }

    #[test]
    fn never_refund_more_than_paid() {
        let done = [r(300, RefundStatus::Succeeded), r(200, RefundStatus::Pending), r(900, RefundStatus::Failed)];
        assert_eq!(refundable(1000, &done), 500, "failed refunds don't count");
        assert_eq!(check_amount(None, 500), Ok(500));
        assert_eq!(check_amount(Some(400), 500), Ok(400));
        assert!(check_amount(Some(501), 500).is_err());
        assert!(check_amount(Some(0), 500).is_err());
        assert!(check_amount(None, 0).is_err());
    }

    #[test]
    fn late_payments_never_resurrect_bookings() {
        assert!(booking_payment_applies("PENDING_PAYMENT", false, false), "within the hold the table is ours");
        assert!(booking_payment_applies("PENDING_PAYMENT", true, true), "hold lapsed but a table is free");
        assert!(!booking_payment_applies("PENDING_PAYMENT", true, false), "hold lapsed and the table was sold");
        assert!(!booking_payment_applies("CANCELLED", false, true));
        assert!(!booking_payment_applies("CONFIRMED", false, true), "already paid by another payment");
    }

    #[test]
    fn final_states_stay_final() {
        assert!(may_move(RefundStatus::Pending, RefundStatus::Succeeded));
        assert!(may_move(RefundStatus::Pending, RefundStatus::Failed));
        assert!(!may_move(RefundStatus::Succeeded, RefundStatus::Failed));
        assert!(!may_move(RefundStatus::Failed, RefundStatus::Succeeded));
        assert!(!may_move(RefundStatus::Pending, RefundStatus::Pending));
    }
}
