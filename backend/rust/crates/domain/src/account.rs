//! Deleting an account (App Store 5.1.1(v), Google Play account deletion).
//!
//! What happens, in order:
//! 1. The caller proves it's them: diners with a phone confirm a fresh
//!    one-time code, staff their password; everyone types the confirmation word.
//! 2. Future bookings are cancelled (the restaurants are told, the tables go
//!    back on sale).
//! 3. Personal data is removed: name, phone, email, password, social sign-in
//!    links and push tokens. Past bookings, receipts, reviews and disputes stay
//!    (tax and consumer-law records), with the name replaced by
//!    [`anonymised_name`] and the phone number blanked.
//! 4. The account can't be used again: every issued token stops working.
//!
//! Pure rules here; the application layer runs the steps.

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::model::{BookingStatus, IsoDate, Role};

/// The word the user types to confirm (case-insensitive).
pub const CONFIRM_WORD: &str = "DELETE";

/// The name shown on records kept after an account is deleted.
pub fn anonymised_name(role: Role) -> &'static str {
    match role {
        Role::Customer => "Former Feast diner",
        Role::Owner | Role::Admin => "Former Feast partner",
    }
}

/// The user typed the confirmation word.
pub fn confirmed(word: &str) -> bool {
    word.trim().eq_ignore_ascii_case(CONFIRM_WORD)
}

/// A booking the deletion cancels: still upcoming (today or later) and not
/// finished, cancelled or missed.
pub fn cancels_booking(status: BookingStatus, date: &IsoDate, today: &IsoDate) -> bool {
    matches!(status, BookingStatus::Confirmed | BookingStatus::PendingPayment) && date.as_str() >= today.as_str()
}

/// Why an owner can't delete their account yet (one per restaurant they alone run).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerBlocker {
    /// Counter payments waiting for the owner to confirm or reject.
    PendingCounterPayments { restaurant: String, count: usize },
    /// Disputes with diners still open.
    OpenCases { restaurant: String, count: usize },
}

impl OwnerBlocker {
    pub fn message(&self) -> String {
        match self {
            Self::PendingCounterPayments { restaurant, count } => format!(
                "{restaurant} has {count} counter payment{} waiting for you. Confirm or reject {} first.",
                if *count == 1 { "" } else { "s" },
                if *count == 1 { "it" } else { "them" }
            ),
            Self::OpenCases { restaurant, count } => format!(
                "{restaurant} has {count} open dispute{} with diners. Reply until {} closed, or add another owner who can take over.",
                if *count == 1 { "" } else { "s" },
                if *count == 1 { "it's" } else { "they're" }
            ),
        }
    }
}

/// Request body for `DELETE /v1/me`.
#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAccountRequest {
    /// Must be "DELETE".
    #[serde(default)]
    pub confirm: String,
    /// Diners with a phone: a fresh code from `POST /v1/auth/otp`.
    pub otp: Option<String>,
    /// Owners: their current password.
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAccountResult {
    pub deleted: bool,
    /// Upcoming bookings that were cancelled.
    pub cancelled_bookings: usize,
    /// Names of the restaurants paused because this was their only owner.
    pub paused_restaurants: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_word() {
        assert!(confirmed("DELETE"));
        assert!(confirmed(" delete "));
        assert!(!confirmed("del"));
        assert!(!confirmed(""));
    }

    #[test]
    fn only_upcoming_open_bookings_are_cancelled() {
        let today = "2026-10-26".to_owned();
        assert!(cancels_booking(BookingStatus::Confirmed, &"2026-10-26".to_owned(), &today));
        assert!(cancels_booking(BookingStatus::PendingPayment, &"2026-11-01".to_owned(), &today));
        assert!(!cancels_booking(BookingStatus::Confirmed, &"2026-10-25".to_owned(), &today));
        assert!(!cancels_booking(BookingStatus::Seated, &"2026-10-26".to_owned(), &today));
        assert!(!cancels_booking(BookingStatus::Completed, &"2026-10-27".to_owned(), &today));
        assert!(!cancels_booking(BookingStatus::Cancelled, &"2026-10-27".to_owned(), &today));
    }

    #[test]
    fn owner_blocker_messages() {
        let m = OwnerBlocker::PendingCounterPayments { restaurant: "Pho Republic".into(), count: 2 }.message();
        assert!(m.contains("2 counter payments") && m.contains("them"));
        let m = OwnerBlocker::OpenCases { restaurant: "Pho Republic".into(), count: 1 }.message();
        assert!(m.contains("1 open dispute ") && m.contains("it's"));
        assert_eq!(anonymised_name(Role::Customer), "Former Feast diner");
    }
}
