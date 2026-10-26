//! Pricing rules. Pure functions — the single source of truth for money maths.
//! Mirrors `webapp/src/lib/pricing.ts` (the client uses its copy only for previews).
//! Country-specific numbers (tax rate/inclusivity, rounding unit, cover charge)
//! come from the restaurant's [`Market`].

use crate::model::{Market, Offer, Paise};

/// Minor units per major unit (100 for INR/AUD, 1 for JPY, 1000 for KWD).
pub fn unit_of(m: &Market) -> Paise {
    10_i64.pow(m.currency_decimals)
}

/// Discount an offer gives on `amount`: whole currency units, never more than the amount.
pub fn offer_discount(offer: &Offer, amount: Paise, m: &Market) -> Paise {
    if let Some(min) = offer.min_bill {
        if amount < min {
            return 0;
        }
    }
    let mut d: Paise = 0;
    if let Some(p) = offer.percent_off {
        d = round_div(amount * p, 100);
    }
    if let Some(flat) = offer.flat_off {
        d = d.max(flat);
    }
    if let Some(max) = offer.max_discount {
        d = d.min(max);
    }
    let unit = unit_of(m);
    d = (d / unit) * unit;
    d.clamp(0, amount.max(0))
}

/// The offer saving the most on `amount`. Ties go to the first listed.
pub fn best_offer<'a>(offers: impl IntoIterator<Item = &'a Offer>, amount: Paise, m: &Market) -> Option<(&'a Offer, Paise)> {
    let mut best: Option<(&Offer, Paise)> = None;
    for o in offers {
        let d = offer_discount(o, amount, m);
        if d > best.map(|(_, b)| b).unwrap_or(0) {
            best = Some((o, d));
        }
    }
    best
}

/// "5", "14.975" — like JS number formatting of a percentage.
pub fn fmt_pct(p: f64) -> String {
    if p.fract() == 0.0 {
        format!("{}", p as i64)
    } else {
        let s = format!("{p:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

/// The diner types the restaurant's final bill total, taxes included (GST in
/// Australia, where menu prices include it; GST in India, where the restaurant
/// adds it on its bill). Feast never adds tax on top, and a deal applies to
/// that total unless an offer says otherwise. The bill line says so.
pub fn bill_total_label(tax_name: &str) -> String {
    let tax = tax_name.trim();
    if tax.is_empty() {
        "Bill total (incl. taxes)".to_owned()
    } else {
        format!("Bill total (incl. {tax})")
    }
}

pub fn payable_for(bill_amount: Paise, discount: Paise, tip: Paise) -> Paise {
    (bill_amount - discount + tip).max(0)
}

/// Cover charge collected at booking time for fine-dining venues (price band 4),
/// per pair of guests, adjusted against the final bill.
pub fn cover_charge_for(price_band: i64, guests: i64, m: &Market) -> Paise {
    if price_band < 4 || m.cover_charge_per_pair <= 0 {
        return 0;
    }
    m.cover_charge_per_pair * ((guests + 1) / 2)
}

/// Integer division rounding half away from zero (matches JS Math.round for positives).
pub fn round_div(n: i64, d: i64) -> i64 {
    if n >= 0 {
        (n + d / 2) / d
    } else {
        -((-n + d / 2) / d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::tests::{australia, india};
    use crate::model::OfferScope;

    #[test]
    fn typed_bill_total_includes_tax() {
        // Nothing is added on top of what the diner typed, in either country.
        assert_eq!(bill_total_label(&australia().tax_name), "Bill total (incl. GST)");
        assert_eq!(bill_total_label(&india().tax_name), "Bill total (incl. GST)");
        assert_eq!(bill_total_label(""), "Bill total (incl. taxes)");
        assert_eq!(payable_for(10_000, 1_500, 0), 8_500, "the deal applies to the typed total");
    }

    fn pct(p: i64, max: Option<Paise>) -> Offer {
        Offer {
            id: "o".into(),
            scope: OfferScope::Bill,
            title: "t".into(),
            subtitle: None,
            percent_off: Some(p),
            flat_off: None,
            max_discount: max,
            min_bill: None,
            terms: vec![],
            moderation: None,
        }
    }

    #[test]
    fn percent_with_cap_rounds_to_whole_units() {
        let m = india();
        assert_eq!(offer_discount(&pct(15, Some(75_000)), 145_400, &m), 21_800);
        assert_eq!(offer_discount(&pct(15, Some(10_000)), 145_400, &m), 10_000);
    }

    #[test]
    fn min_bill_gates_flat_offers() {
        let m = india();
        let flat = Offer { flat_off: Some(15_000), min_bill: Some(108_000), percent_off: None, ..pct(0, None) };
        assert_eq!(offer_discount(&flat, 100_000, &m), 0);
        assert_eq!(offer_discount(&flat, 200_000, &m), 15_000);
    }

    #[test]
    fn best_offer_picks_biggest() {
        let (a, b) = (pct(10, None), pct(30, None));
        let (o, d) = best_offer([&a, &b], 100_000, &india()).unwrap();
        assert_eq!((o.percent_off, d), (Some(30), 30_000));
    }

    #[test]
    fn cover_charge_only_for_fine_dining() {
        let m = india();
        assert_eq!(cover_charge_for(3, 4, &m), 0);
        assert_eq!(cover_charge_for(4, 2, &m), 50_000);
        assert_eq!(cover_charge_for(4, 3, &m), 100_000);
        assert_eq!(cover_charge_for(4, 2, &australia()), 3_000);
    }
}
