//! Offline ("pay at the counter") payments. Pure rules; the application layer
//! applies them.
//!
//! A diner can settle a Feast bill in cash or on the restaurant's own card
//! terminal. Feast never holds that money, so the bill only counts as paid once
//! the restaurant's owner (or Feast staff with support access) confirms it:
//!
//! ```text
//! PENDING_APPROVAL ──approve──▶ APPROVED      (bill paid, deal used)
//!        │  └──────reject────▶ REJECTED      (reason required; diner can pay again)
//!        └─────expire (2 h)───▶ EXPIRED       (nobody decided; diner can pay again)
//! ```
//!
//! Only `PENDING_APPROVAL` can change; the other states are final. A bill's
//! Feast deal is held while the payment waits, used on approval and freed on
//! rejection or expiry.

use crate::model::{OfflinePaymentStatus, Paise};

/// How long a counter payment may wait for the restaurant before it lapses.
pub const APPROVAL_WINDOW_MINUTES: i64 = 120;

/// Shortest rejection reason we accept (the diner sees it).
pub const MIN_REASON_CHARS: usize = 5;

/// What the restaurant (or the sweeper) decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approve,
    Reject,
    Expire,
}

/// Why a decision isn't allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfflineError {
    AlreadyDecided(OfflinePaymentStatus),
    ReasonRequired,
    WrongAmount { expected: Paise, received: Paise },
    NotExpiredYet,
}

impl OfflineError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::AlreadyDecided(_) => "ALREADY_DECIDED",
            Self::ReasonRequired => "REASON_REQUIRED",
            Self::WrongAmount { .. } => "WRONG_AMOUNT",
            Self::NotExpiredYet => "NOT_EXPIRED",
        }
    }
}

/// The next status, or why the decision isn't allowed.
///
/// - Approving may state the amount received; it must equal what the diner owes.
/// - Rejecting needs a reason of at least [`MIN_REASON_CHARS`] characters.
/// - Expiring is only for payments still pending at/after `expires_at`
///   (`now_ms >= expires_ms`).
pub fn decide(
    status: OfflinePaymentStatus,
    decision: Decision,
    owed: Paise,
    amount_received: Option<Paise>,
    reason: Option<&str>,
    now_ms: i64,
    expires_ms: i64,
) -> Result<OfflinePaymentStatus, OfflineError> {
    if status != OfflinePaymentStatus::PendingApproval {
        return Err(OfflineError::AlreadyDecided(status));
    }
    match decision {
        Decision::Approve => match amount_received {
            Some(r) if r != owed => Err(OfflineError::WrongAmount { expected: owed, received: r }),
            _ => Ok(OfflinePaymentStatus::Approved),
        },
        Decision::Reject => match reason.map(str::trim) {
            Some(r) if r.chars().count() >= MIN_REASON_CHARS => Ok(OfflinePaymentStatus::Rejected),
            _ => Err(OfflineError::ReasonRequired),
        },
        Decision::Expire if now_ms >= expires_ms => Ok(OfflinePaymentStatus::Expired),
        Decision::Expire => Err(OfflineError::NotExpiredYet),
    }
}

/// Is this payment still waiting (and so blocks paying the same bill online)?
pub fn is_open(status: OfflinePaymentStatus) -> bool {
    status == OfflinePaymentStatus::PendingApproval
}

#[cfg(test)]
mod tests {
    use super::*;
    use OfflinePaymentStatus::*;

    #[test]
    fn approve_reject_expire_from_pending() {
        assert_eq!(decide(PendingApproval, Decision::Approve, 1000, None, None, 0, 10), Ok(Approved));
        assert_eq!(decide(PendingApproval, Decision::Approve, 1000, Some(1000), None, 0, 10), Ok(Approved));
        assert_eq!(decide(PendingApproval, Decision::Reject, 1000, None, Some("Diner left without paying"), 0, 10), Ok(Rejected));
        assert_eq!(decide(PendingApproval, Decision::Expire, 1000, None, None, 10, 10), Ok(Expired));
    }

    #[test]
    fn final_states_never_change() {
        for s in [Approved, Rejected, Expired] {
            for d in [Decision::Approve, Decision::Reject, Decision::Expire] {
                assert_eq!(decide(s, d, 1000, None, Some("long enough"), 99, 0), Err(OfflineError::AlreadyDecided(s)));
            }
        }
    }

    #[test]
    fn approval_amount_must_match() {
        assert_eq!(decide(PendingApproval, Decision::Approve, 1000, Some(900), None, 0, 10), Err(OfflineError::WrongAmount { expected: 1000, received: 900 }));
    }

    #[test]
    fn rejection_needs_a_reason() {
        assert_eq!(decide(PendingApproval, Decision::Reject, 1000, None, None, 0, 10), Err(OfflineError::ReasonRequired));
        assert_eq!(decide(PendingApproval, Decision::Reject, 1000, None, Some("  no  "), 0, 10), Err(OfflineError::ReasonRequired));
    }

    #[test]
    fn expiry_waits_for_the_window() {
        assert_eq!(decide(PendingApproval, Decision::Expire, 1000, None, None, 9, 10), Err(OfflineError::NotExpiredYet));
        assert!(is_open(PendingApproval));
        assert!(!is_open(Approved));
    }
}
