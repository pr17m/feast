//! Market (country) rules: price bands, cost limits, plans, phone numbers, tax,
//! money formatting. Mirrors `webapp/src/lib/api/market.ts`.

use regex::Regex;

use crate::model::{City, Market, Paise, PlanId, SubscriptionPlan};

pub fn price_band_for(cost_for_two: Paise, m: &Market) -> i64 {
    let [a, b, c] = m.price_bands;
    if cost_for_two < a {
        1
    } else if cost_for_two < b {
        2
    } else if cost_for_two < c {
        3
    } else {
        4
    }
}

/// Sensible cost-for-two range (scaled from the market's price bands).
pub fn cost_limits(m: &Market) -> (Paise, Paise) {
    (((m.price_bands[0] as f64) / 10.0).round() as Paise, m.price_bands[2] * 30)
}

pub fn tax_on(amount: Paise, percent: f64) -> Paise {
    (amount as f64 * percent / 100.0).round() as Paise
}

/// The city's effective food tax (city override, else the market's).
pub fn food_tax_for(m: &Market, city: Option<&City>) -> f64 {
    city.and_then(|c| c.food_tax_percent).unwrap_or(m.food_tax_percent)
}

pub fn plan_price(m: &Market, id: PlanId) -> Paise {
    match id {
        PlanId::Monthly => m.plan_prices.monthly,
        PlanId::Quarterly => m.plan_prices.quarterly,
        PlanId::Yearly => m.plan_prices.yearly,
    }
}

/// The market's subscription plans with tax and the saving vs monthly.
pub fn plans_for(m: &Market) -> Vec<SubscriptionPlan> {
    let monthly = m.plan_prices.monthly;
    PlanId::ALL
        .into_iter()
        .map(|id| {
            let price = plan_price(m, id);
            let months = id.months();
            let save = if monthly > 0 { ((1.0 - price as f64 / (monthly * months as i64) as f64) * 100.0).round() as i64 } else { 0 };
            SubscriptionPlan {
                id,
                name: id.name().into(),
                months,
                currency: m.currency.clone(),
                price,
                tax_name: m.tax_name.clone(),
                tax_percent: m.platform_tax_percent,
                total: price + tax_on(price, m.platform_tax_percent),
                save_percent: (save > 0).then_some(save),
            }
        })
        .collect()
}

/// E.164 from a national number typed by a person ("0412 345 678" → "+61412345678").
pub fn to_e164(m: &Market, national: &str) -> Option<String> {
    let digits: String = national.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_start_matches('0');
    Regex::new(&m.phone.pattern).ok()?.is_match(digits).then(|| format!("{}{digits}", m.phone.dial_code))
}

/// The market a stored E.164 number belongs to (longest dial code wins).
pub fn market_for_phone<'a>(markets: &'a [Market], e164: &str) -> Option<&'a Market> {
    let mut sorted: Vec<&Market> = markets.iter().collect();
    sorted.sort_by_key(|m| std::cmp::Reverse(m.phone.dial_code.len()));
    sorted
        .into_iter()
        .find(|m| e164.strip_prefix(&m.phone.dial_code).is_some_and(|rest| Regex::new(&m.phone.pattern).map(|r| r.is_match(rest)).unwrap_or(false)))
}

fn symbol(currency: &str) -> &str {
    match currency {
        "INR" => "₹",
        "AUD" | "NZD" | "CAD" | "USD" | "SGD" | "HKD" => "$",
        "GBP" => "£",
        "EUR" => "€",
        "JPY" => "¥",
        other => other,
    }
}

/// Group digits: Indian style (12,34,567) for INR, else thousands (1,234,567).
fn group(int: u64, indian: bool) -> String {
    let s = int.to_string();
    if s.len() <= 3 {
        return s;
    }
    let (head, tail) = s.split_at(s.len() - 3);
    let step = if indian { 2 } else { 3 };
    let mut parts: Vec<&str> = vec![];
    let mut h = head;
    while h.len() > step {
        let (a, b) = h.split_at(h.len() - step);
        parts.insert(0, b);
        h = a;
    }
    if !h.is_empty() {
        parts.insert(0, h);
    }
    format!("{},{tail}", parts.join(","))
}

/// "₹1,800", "$45.50", "AED 120" — like the webapp's `money()`: whole amounts
/// drop the decimals. (Server-side text: notifications, facet labels, errors.)
/// ISO 4217 minor-unit digits (when only the currency code is at hand).
pub fn currency_decimals(currency: &str) -> u32 {
    match currency {
        "JPY" | "KRW" | "VND" | "CLP" | "ISK" | "UGX" | "XOF" | "XAF" | "PYG" | "RWF" => 0,
        "KWD" | "BHD" | "OMR" | "JOD" | "TND" | "LYD" | "IQD" => 3,
        _ => 2,
    }
}

/// Format with the currency's standard decimals.
pub fn format_currency(minor: Paise, currency: &str) -> String {
    format_money(minor, currency, currency_decimals(currency))
}

pub fn format_money(minor: Paise, currency: &str, decimals: u32) -> String {
    let unit = 10_i64.pow(decimals);
    let abs = minor.unsigned_abs();
    let (int, frac) = (abs / unit as u64, abs % unit as u64);
    let sym = symbol(currency);
    let mut body = group(int, currency == "INR");
    if frac != 0 {
        body = format!("{body}.{:0width$}", frac, width = decimals as usize);
    }
    let sign = if minor < 0 { "-" } else { "" };
    if sym.len() > 1 && sym.chars().all(|c| c.is_ascii_uppercase()) {
        format!("{sign}{sym} {body}")
    } else {
        format!("{sign}{sym}{body}")
    }
}

pub fn money(minor: Paise, m: &Market) -> String {
    format_money(minor, &m.currency, m.currency_decimals)
}

/// "₹₹₹" style price band.
pub fn band_symbol(band: i64, currency: &str) -> String {
    let s = symbol(currency);
    if s.chars().count() <= 2 {
        s.repeat(band.clamp(1, 4) as usize)
    } else {
        format!("{s} {}", "•".repeat(band.clamp(1, 4) as usize))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::*;

    pub fn india() -> Market {
        Market {
            country_code: "IN".into(),
            name: "India".into(),
            currency: "INR".into(),
            currency_decimals: 2,
            locale: "en-IN".into(),
            status: MarketStatus::Live,
            tax_name: "GST".into(),
            food_tax_percent: 5.0,
            prices_include_tax: false,
            platform_tax_percent: 18.0,
            price_bands: [90_000, 150_000, 300_000],
            cover_charge_per_pair: 50_000,
            tip_presets: vec![0, 2000, 5000, 10000],
            plan_prices: PlanPrices { monthly: 99_900, quarterly: 269_900, yearly: 999_900 },
            payments: MarketPayments { provider: "mock".into(), methods: vec![PaymentMethod::Upi, PaymentMethod::Card], public_key: None },
            phone: PhoneRules { dial_code: "+91".into(), pattern: r"^[6-9]\d{9}$".into(), example: "98765 43210".into() },
            documents: vec![],
        }
    }

    pub fn australia() -> Market {
        Market {
            country_code: "AU".into(),
            name: "Australia".into(),
            currency: "AUD".into(),
            locale: "en-AU".into(),
            tax_name: "GST".into(),
            food_tax_percent: 10.0,
            prices_include_tax: true,
            platform_tax_percent: 10.0,
            price_bands: [6_000, 12_000, 22_000],
            cover_charge_per_pair: 3_000,
            plan_prices: PlanPrices { monthly: 5_900, quarterly: 15_900, yearly: 59_000 },
            phone: PhoneRules { dial_code: "+61".into(), pattern: r"^4\d{8}$".into(), example: "412 345 678".into() },
            ..india()
        }
    }

    #[test]
    fn plans_bands_phones_money() {
        let (i, a) = (india(), australia());
        let plans = plans_for(&i);
        assert_eq!(plans[0].total, 99_900 + 17_982);
        assert_eq!(plans[1].save_percent, Some(10));
        assert_eq!(plans[2].save_percent, Some(17));
        assert_eq!(price_band_for(140_000, &i), 2);
        assert_eq!(price_band_for(26_000, &a), 4);
        assert_eq!(to_e164(&a, "0412 345 678").as_deref(), Some("+61412345678"));
        assert_eq!(to_e164(&i, "98765 43210").as_deref(), Some("+919876543210"));
        assert!(to_e164(&i, "12345").is_none());
        let markets = vec![i.clone(), a.clone()];
        assert_eq!(market_for_phone(&markets, "+61412345678").map(|m| m.country_code.as_str()), Some("AU"));
        assert!(market_for_phone(&markets, "+15551234567").is_none());
        assert_eq!(format_money(123_456_700, "INR", 2), "₹12,34,567");
        assert_eq!(format_money(4_550, "AUD", 2), "$45.50");
        assert_eq!(format_money(1_234_567_800, "AUD", 2), "$12,345,678");
        assert_eq!(format_money(12_000, "AED", 2), "AED 120");
        assert_eq!(band_symbol(3, "INR"), "₹₹₹");
    }
}
