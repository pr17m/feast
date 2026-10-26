//! Table-slot generation and per-slot deals. Pure: the caller supplies "now" and
//! how many tables are already booked per slot.

use crate::model::{MealPeriod, Offer, OfferScope, Restaurant, Slot};

const PERIODS: [(MealPeriod, i64, i64); 3] =
    [(MealPeriod::Breakfast, 8 * 60, 11 * 60), (MealPeriod::Lunch, 12 * 60, 15 * 60 + 30), (MealPeriod::Dinner, 19 * 60, 23 * 60)];

pub struct SlotContext<'a> {
    pub date: &'a str,
    pub guests: i64,
    /// Today's date in the city's timezone.
    pub today: &'a str,
    /// Minutes since local midnight, now.
    pub now_minutes: i64,
}

/// Deterministic 32-bit FNV-1a variant, identical to the web mock's `hash()` so
/// demand patterns match between mock and real backend.
pub fn demand_hash(s: &str) -> i64 {
    if s.is_empty() {
        return 2_166_136_261; // JS never coerces to int32 for an empty string
    }
    let mut h: i32 = 2_166_136_261u32 as i32;
    for c in s.encode_utf16() {
        h ^= c as i32;
        h = h.wrapping_mul(16_777_619);
    }
    (h as i64).abs()
}

pub fn parse_hhmm(s: &str) -> i64 {
    let mut it = s.split(':');
    let h: i64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let m: i64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    h * 60 + m
}

pub fn fmt_hhmm(m: i64) -> String {
    format!("{:02}:{:02}", m / 60, m % 60)
}

pub fn slot_id(restaurant_id: &str, date: &str, time: &str) -> String {
    format!("{restaurant_id}:{date}:{time}")
}

/// All slots for a day. `booked(time)` returns tables already held in that slot.
/// How long an unpaid booking (cover charge pending) holds its table.
pub const PAYMENT_HOLD_MINUTES: i64 = 20;

/// Tables Feast may sell for one slot: the restaurant's tables per slot minus
/// what walk-ins and other channels take (deterministic per slot).
pub fn feast_tables(r: &Restaurant, date: &str, time: &str) -> i64 {
    let d = &r.detail;
    let tables = d.settings.tables_per_slot.max(0);
    let h = demand_hash(&format!("{}:{}:{}", d.id, date, time));
    let other_demand = if h % 7 == 0 { tables } else { h % 3 };
    (tables - other_demand).max(0)
}

pub fn compute_slots(r: &Restaurant, ctx: &SlotContext, booked: impl Fn(&str) -> i64) -> Vec<Slot> {
    let d = &r.detail;
    if !d.is_open_for(crate::model::Service::Bookings) {
        return vec![];
    }
    let (open_s, close_s) = d.hours.first().map(|h| (h.open.as_str(), h.close.as_str())).unwrap_or(("12:00", "23:00"));
    let open = parse_hhmm(open_s);
    let mut close = parse_hhmm(close_s);
    if close <= open {
        close += 24 * 60; // closes after midnight
    }
    let pct = d.prebook_percent();
    let is_today = ctx.date == ctx.today;
    let mut slots = Vec::new();

    for (period, from, to) in PERIODS {
        if period == MealPeriod::Breakfast && !r.breakfast {
            continue;
        }
        let mut m = from.max(open);
        while m <= to.min(close - 60) {
            let time = fmt_hhmm(m);
            // Walk-ins and other channels also take tables (see `feast_tables`).
            let tables_left = (feast_tables(r, ctx.date, &time) - booked(&time)).max(0);
            let seats_left = tables_left * 4;
            let peak = (13 * 60..=14 * 60).contains(&m) || (20 * 60..=21 * 60 + 30).contains(&m);
            let off_peak = m <= from + 30 || m >= to - 30;
            let deal_pct = if pct == 0 {
                0
            } else if off_peak {
                (pct + 10).min(50)
            } else if peak {
                (pct - 10).max(0)
            } else {
                pct
            };
            let deal = (deal_pct > 0).then(|| Offer {
                id: format!("{}_slot_{deal_pct}", d.id),
                scope: OfferScope::Prebook,
                title: format!("Flat {deal_pct}% off"),
                subtitle: Some(
                    if off_peak {
                        "Off-peak deal"
                    } else if peak {
                        "Peak hour"
                    } else {
                        "on total bill"
                    }
                    .into(),
                ),
                percent_off: Some(deal_pct),
                flat_off: None,
                max_discount: None,
                min_bill: None,
                terms: vec!["Valid on total bill for the booked slot".into(), "Arrive within 15 minutes of slot time".into(), "Not valid on alcohol".into()],
                moderation: None,
            });
            let past = is_today && m < ctx.now_minutes + 30;
            let needed_tables = (ctx.guests + 7) / 8;
            slots.push(Slot {
                id: slot_id(&d.id, ctx.date, &time),
                time,
                period,
                seats_left,
                available: !past && seats_left >= ctx.guests.min(8) && tables_left >= needed_tables,
                deal,
            });
            m += 30;
        }
    }
    slots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_js_reference() {
        // Reference values computed with the web mock's hash() in Node.
        assert_eq!(demand_hash(""), 2_166_136_261);
        assert_eq!(demand_hash("a"), 468_965_076);
        assert_eq!(demand_hash("r_olive_ember:2026-10-25:19:00"), 593_360_117);
        assert_eq!(demand_hash("feast"), 2_038_236_160);
    }

    #[test]
    fn hhmm_roundtrip() {
        assert_eq!(parse_hhmm("19:30"), 1170);
        assert_eq!(fmt_hhmm(1170), "19:30");
    }
}
