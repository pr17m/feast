//! Restaurant-wide fairness controls on Feast deals. Pure rules; the
//! application layer applies them when a bill is priced and paid.
//!
//! - **Per-bill cap** (`max_discount_per_bill`): no Feast deal takes more than
//!   this off one bill, whatever the offer says.
//! - **Daily deal limit** (`daily_deal_limit`): at most this many discounted
//!   bills per restaurant per local day. Past it, bills are still payable on
//!   Feast, just without the deal, so the restaurant's discount cost is
//!   predictable.
//!
//! - **Book on Feast, pay on Feast**: a Feast deal is only for a diner who
//!   booked a table at that restaurant on Feast for today, and only on a bill
//!   paid through Feast (in the app, or at the counter and confirmed by the
//!   restaurant in Feast) — one discounted bill per booking. Walk-ins without
//!   a Feast booking pay the restaurant as usual. A discount given at the till
//!   outside Feast is the restaurant's own choice, not a Feast deal.
//!
//! Menu prices are the same for every diner (there is no Feast-only price);
//! the deal is always a discount on the bill.

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::market::money;
use crate::model::{BookingStatus, Market, Paise};
use crate::pricing::unit_of;

/// Shown to the diner when today's limit is reached.
pub const DEALS_USED_UP: &str = "Today's Feast deals at this restaurant are used up. You can still pay your bill on Feast.";

/// A walk-in tried to pay a bill they typed in, with no Feast booking.
pub const BOOKING_REQUIRED: &str = "Book a table on Feast to pay here and get the deal.";

/// The booking's one discounted bill is already paid (or being paid).
pub const BOOKING_DEAL_USED: &str = "You've already used your Feast deal for this booking.";

/// Result of holding a deal for a bill (see the billing port's `claim_deal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimOutcome {
    Claimed,
    /// The restaurant's daily deal limit is reached.
    LimitReached,
    /// Another bill already used (or is paying with) this booking's deal.
    BookingDealUsed,
}

/// Does this booking let the diner have a Feast deal on today's bill?
/// Same diner, same restaurant, booked for the restaurant's local `today`, and
/// still on (confirmed, seated, or completed after the meal).
pub fn booking_qualifies(status: BookingStatus, booking_date: &str, today: &str, same_diner: bool, same_restaurant: bool) -> bool {
    same_diner && same_restaurant && booking_date == today && matches!(status, BookingStatus::Confirmed | BookingStatus::Seated | BookingStatus::Completed)
}

/// Highest daily limit an owner can set.
pub const MAX_DAILY_DEAL_LIMIT: i64 = 10_000;

/// The discount after the restaurant's per-bill cap (whole currency units).
/// A missing or non-positive cap means no cap.
pub fn cap_discount(discount: Paise, cap: Option<Paise>, m: &Market) -> Paise {
    match cap {
        Some(c) if c > 0 => {
            let unit = unit_of(m);
            discount.min((c / unit) * unit).max(0)
        }
        _ => discount.max(0),
    }
}

/// Deals still available today: `None` = unlimited.
pub fn deals_left(limit: Option<i64>, used: i64) -> Option<i64> {
    limit.map(|l| (l - used).max(0))
}

/// Can another discounted bill be accepted today?
pub fn deal_available(limit: Option<i64>, used: i64) -> bool {
    deals_left(limit, used).is_none_or(|left| left > 0)
}

/// Diner-facing note when the per-bill cap reduced the deal.
pub fn cap_notice(cap: Paise, m: &Market) -> String {
    format!("This restaurant caps Feast deals at {} per bill", money(cap, m))
}

/// Validate an owner's settings. Returns a message for the first problem.
pub fn validate(max_discount_per_bill: Option<Paise>, daily_deal_limit: Option<i64>, m: &Market) -> Result<(), String> {
    if let Some(cap) = max_discount_per_bill {
        let unit = unit_of(m);
        if cap < unit || cap % unit != 0 {
            return Err(format!("Set the per-bill cap in whole amounts of at least {}", money(unit, m)));
        }
        // Generous ceiling: 20× the top price band threshold (e.g. A$4,400 in Australia).
        let ceiling = m.price_bands[2] * 20;
        if cap > ceiling {
            return Err(format!("The per-bill cap can't be more than {}", money(ceiling, m)));
        }
    }
    if let Some(l) = daily_deal_limit {
        if !(1..=MAX_DAILY_DEAL_LIMIT).contains(&l) {
            return Err(format!("Deals per day must be between 1 and {MAX_DAILY_DEAL_LIMIT} (or no limit)"));
        }
    }
    Ok(())
}

/// Today's deal budget, for the owner's dashboard.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DealBudget {
    /// The restaurant's local date (YYYY-MM-DD).
    pub date: String,
    pub daily_deal_limit: Option<i64>,
    pub max_discount_per_bill: Option<Paise>,
    /// Discounted bills paid today, plus payments in progress.
    pub used: i64,
    /// None = unlimited.
    pub remaining: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn au() -> Market {
        crate::market::tests::australia()
    }

    #[test]
    fn cap_limits_the_discount() {
        let m = au();
        assert_eq!(cap_discount(15_000, Some(10_000), &m), 10_000);
        assert_eq!(cap_discount(8_000, Some(10_000), &m), 8_000);
        assert_eq!(cap_discount(8_000, None, &m), 8_000);
        assert_eq!(cap_discount(8_000, Some(0), &m), 8_000, "0 = no cap");
        // Caps are whole units.
        assert_eq!(cap_discount(15_000, Some(10_050), &m), 10_000);
    }

    #[test]
    fn daily_limit() {
        assert_eq!(deals_left(None, 50), None);
        assert_eq!(deals_left(Some(20), 12), Some(8));
        assert_eq!(deals_left(Some(20), 25), Some(0));
        assert!(deal_available(None, 1_000));
        assert!(deal_available(Some(1), 0));
        assert!(!deal_available(Some(1), 1));
    }

    #[test]
    fn validation() {
        let m = au();
        assert!(validate(Some(10_000), Some(20), &m).is_ok());
        assert!(validate(None, None, &m).is_ok());
        assert!(validate(Some(50), None, &m).is_err(), "below one dollar");
        assert!(validate(Some(10_050), None, &m).is_err(), "not whole dollars");
        assert!(validate(Some(1_000_000), None, &m).is_err(), "absurdly high");
        assert!(validate(None, Some(0), &m).is_err());
        assert!(validate(None, Some(MAX_DAILY_DEAL_LIMIT + 1), &m).is_err());
    }

    #[test]
    fn only_todays_own_booking_at_this_restaurant_qualifies() {
        use BookingStatus::*;
        let today = "2026-10-25";
        for s in [Confirmed, Seated, Completed] {
            assert!(booking_qualifies(s, today, today, true, true), "{s:?}");
        }
        for s in [PendingPayment, Cancelled, NoShow] {
            assert!(!booking_qualifies(s, today, today, true, true), "{s:?}");
        }
        assert!(!booking_qualifies(Confirmed, "2026-10-24", today, true, true), "yesterday's booking");
        assert!(!booking_qualifies(Confirmed, "2026-10-26", today, true, true), "tomorrow's booking");
        assert!(!booking_qualifies(Confirmed, today, today, false, true), "someone else's booking");
        assert!(!booking_qualifies(Confirmed, today, today, true, false), "a booking at another restaurant");
    }

    #[test]
    fn notice_uses_market_money() {
        assert!(cap_notice(10_000, &au()).contains("100"));
    }
}
