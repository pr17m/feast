//! Arrival: a diner with a Feast booking taps "I'm here" when they reach the
//! restaurant, so staff know they've arrived. Check-in is open from shortly
//! before the slot until a while after it; repeating it is harmless.

use crate::model::BookingStatus;

/// Check-in opens this long before the slot…
pub const CHECK_IN_BEFORE_MINUTES: i64 = 30;
/// …and stays open until this long after it (bookings auto-complete 3 hours after the slot).
pub const CHECK_IN_AFTER_MINUTES: i64 = 120;

/// Why a diner can't check in ("I'm here") yet, or `None` when they can.
/// `slot_at_ms` is the restaurant's local slot converted to epoch ms.
pub fn check_in_blocker(status: BookingStatus, slot_at_ms: i64, now_ms: i64) -> Option<&'static str> {
    if !matches!(status, BookingStatus::Confirmed | BookingStatus::Seated) {
        return Some("This booking isn't active");
    }
    if now_ms < slot_at_ms - CHECK_IN_BEFORE_MINUTES * 60_000 {
        return Some("Check-in opens 30 minutes before your booking");
    }
    if now_ms > slot_at_ms + CHECK_IN_AFTER_MINUTES * 60_000 {
        return Some("This booking's time has passed");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOT: i64 = 1_800_000_000_000;
    const MIN: i64 = 60_000;

    #[test]
    fn window_is_30_minutes_before_to_2_hours_after() {
        let ok = |now| check_in_blocker(BookingStatus::Confirmed, SLOT, now).is_none();
        assert!(!ok(SLOT - 31 * MIN));
        assert!(ok(SLOT - 30 * MIN));
        assert!(ok(SLOT));
        assert!(ok(SLOT + 120 * MIN));
        assert!(!ok(SLOT + 121 * MIN));
    }

    #[test]
    fn only_live_bookings_check_in() {
        assert!(check_in_blocker(BookingStatus::Seated, SLOT, SLOT).is_none());
        for s in [BookingStatus::PendingPayment, BookingStatus::Cancelled, BookingStatus::Completed, BookingStatus::NoShow] {
            assert_eq!(check_in_blocker(s, SLOT, SLOT), Some("This booking isn't active"), "{s:?}");
        }
    }

    #[test]
    fn check_in_explains_why_not() {
        assert_eq!(check_in_blocker(BookingStatus::Confirmed, SLOT, SLOT - 45 * MIN), Some("Check-in opens 30 minutes before your booking"));
        assert_eq!(check_in_blocker(BookingStatus::Confirmed, SLOT, SLOT + 3 * 60 * MIN), Some("This booking's time has passed"));
    }
}
