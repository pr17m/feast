//! Listing rules: how owner-entered listing data is cleaned, how price bands
//! are derived, and how search filters (facets) are built from live listings.
//! Mirrors `webapp/src/lib/api/listing.ts` (the mock backend) exactly.
//!
//! No cuisine or experience list is hard-coded: every filter option exists
//! because some public restaurant listed it.

use std::collections::{BTreeSet, HashMap};

use crate::market::{band_symbol, money};
use crate::model::{Facet, Market, OfferScope, RestaurantDetail, RestaurantStatus, SearchFacets, SearchQuery, SearchService};

pub const MAX_CUISINES: usize = 6;
pub const MAX_TAGS: usize = 8;
pub const MAX_LABEL_CHARS: usize = 30;
pub const MAX_ABOUT_CHARS: usize = 600;
pub const MAX_HIGHLIGHTS: usize = 6;

/// Trim, collapse whitespace, drop empties and case-insensitive duplicates,
/// reuse the platform's existing spelling ("pizza" → "Pizza"), else capitalise
/// the first letter. Keeps at most `max`.
pub fn clean_labels(input: &[String], known: &[String], max: usize) -> Vec<String> {
    let canon: HashMap<String, &String> = known.iter().map(|k| (k.to_lowercase(), k)).collect();
    let mut out: Vec<String> = vec![];
    for raw in input {
        let v: String = raw.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_LABEL_CHARS).collect();
        if v.is_empty() {
            continue;
        }
        let key = v.to_lowercase();
        if out.iter().any(|o| o.to_lowercase() == key) {
            continue;
        }
        out.push(match canon.get(&key) {
            Some(k) => (*k).clone(),
            None => {
                let mut c = v.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            }
        });
    }
    out.truncate(max);
    out
}

/// Services a diner can use there right now.
pub fn services(d: &RestaurantDetail) -> Vec<SearchService> {
    if d.status != RestaurantStatus::Live {
        return vec![];
    }
    let s = &d.settings;
    [(s.bookings_enabled, SearchService::Bookings), (s.bill_payment_enabled, SearchService::BillPayment)]
        .into_iter()
        .filter_map(|(on, svc)| on.then_some(svc))
        .collect()
}

/// Offer kinds a diner can actually use (the matching service must be on).
pub fn usable_offers(d: &RestaurantDetail) -> Vec<OfferScope> {
    let s = services(d);
    let mut out: Vec<OfferScope> = vec![];
    for o in &d.offers {
        let ok = match o.scope {
            OfferScope::Prebook => s.contains(&SearchService::Bookings),
            OfferScope::Bill | OfferScope::Bank => s.contains(&SearchService::BillPayment),
        };
        if ok && !out.contains(&o.scope) {
            out.push(o.scope);
        }
    }
    out
}

fn offer_label(v: &str) -> String {
    match v {
        "PREBOOK" => "Pre-booking deals",
        "BILL" => "Bill payment offers",
        "BANK" => "Bank offers",
        other => other,
    }
    .to_owned()
}

fn service_label(v: &str) -> String {
    match v {
        "BOOKINGS" => "Table booking",
        "BILL_PAYMENT" => "Pay bill on Feast",
        other => other,
    }
    .to_owned()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
    Text,
    Cuisine,
    Tag,
    Area,
    PriceBand,
    Offer,
    Service,
    PureVeg,
    MinRating,
}

const KEYS: [Key; 9] = [Key::Text, Key::Cuisine, Key::Tag, Key::Area, Key::PriceBand, Key::Offer, Key::Service, Key::PureVeg, Key::MinRating];

/// Filter `all` (public restaurants in a city) by `q` and build facets.
/// `haystack(d)` is the lower-cased text searched by `q.q` (name, area,
/// cuisines, tags, dish names). Facet counts are disjunctive: each group's
/// counts apply every *other* active filter.
pub fn search_listings<'a>(
    all: &[&'a RestaurantDetail],
    q: &SearchQuery,
    market: &Market,
    haystack: impl Fn(&RestaurantDetail) -> String,
) -> (Vec<&'a RestaurantDetail>, SearchFacets) {
    let text = q.q.as_deref().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty());
    let hay: Vec<String> = if text.is_some() { all.iter().map(|d| haystack(d)).collect() } else { vec![] };
    let pass = |i: usize, d: &RestaurantDetail, k: Key| -> bool {
        match k {
            Key::Text => text.as_ref().is_none_or(|t| hay[i].contains(t.as_str())),
            Key::Cuisine => q.cuisine.as_ref().is_none_or(|c| d.cuisines.contains(c)),
            Key::Tag => q.tag.as_ref().is_none_or(|t| d.tags.contains(t)),
            Key::Area => q.area.as_ref().is_none_or(|a| &d.area == a),
            Key::PriceBand => q.price_band.is_none_or(|b| d.price_band == b),
            Key::Offer => q.offer.is_none_or(|o| usable_offers(d).contains(&o)),
            Key::Service => q.service.is_none_or(|s| services(d).contains(&s)),
            Key::PureVeg => q.pure_veg != Some(true) || d.is_pure_veg,
            Key::MinRating => q.min_rating.is_none_or(|m| d.rating >= m),
        }
    };
    let except = |skip: Option<Key>| -> Vec<&'a RestaurantDetail> {
        all.iter().enumerate().filter(|(i, d)| KEYS.iter().all(|&k| Some(k) == skip || pass(*i, d, k))).map(|(_, d)| *d).collect()
    };
    let count = |rows: Vec<&RestaurantDetail>,
                 values: &dyn Fn(&RestaurantDetail) -> Vec<String>,
                 label: &dyn Fn(&str) -> String,
                 selected: Option<String>|
     -> Vec<Facet> {
        let mut n: HashMap<String, usize> = HashMap::new();
        for d in rows {
            for v in values(d).into_iter().collect::<BTreeSet<_>>() {
                *n.entry(v).or_default() += 1;
            }
        }
        if let Some(s) = selected {
            n.entry(s).or_insert(0);
        }
        let mut f: Vec<Facet> = n.into_iter().map(|(value, count)| Facet { label: label(&value), value, count }).collect();
        f.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
        f
    };
    let band_range = |value: &str| -> String {
        let band: i64 = value.parse().unwrap_or(0);
        let costs: Vec<i64> = all.iter().filter(|d| d.price_band == band).map(|d| d.cost_for_two).collect();
        match (costs.iter().min(), costs.iter().max()) {
            (Some(lo), Some(hi)) => {
                let range = if lo == hi { money(*lo, market) } else { format!("{}–{}", money(*lo, market), money(*hi, market)) };
                format!("{} · {range}", band_symbol(band, &market.currency))
            }
            _ => ["Budget", "Mid-range", "Premium", "Luxury"].get((band - 1).clamp(0, 3) as usize).copied().unwrap_or("").to_owned(),
        }
    };

    let mut price_bands = count(except(Some(Key::PriceBand)), &|d| vec![d.price_band.to_string()], &band_range, q.price_band.map(|b| b.to_string()));
    price_bands.sort_by_key(|f| f.value.parse::<i64>().unwrap_or(0));
    let facets = SearchFacets {
        cuisines: count(except(Some(Key::Cuisine)), &|d| d.cuisines.clone(), &|v| v.to_owned(), q.cuisine.clone()),
        tags: count(except(Some(Key::Tag)), &|d| d.tags.clone(), &|v| v.to_owned(), q.tag.clone()),
        areas: count(except(Some(Key::Area)), &|d| vec![d.area.clone()], &|v| v.to_owned(), q.area.clone()),
        price_bands,
        offers: count(
            except(Some(Key::Offer)),
            &|d| usable_offers(d).iter().map(|o| o.as_str().to_owned()).collect(),
            &offer_label,
            q.offer.map(|o| o.as_str().to_owned()),
        ),
        services: count(
            except(Some(Key::Service)),
            &|d| services(d).iter().map(|s| s.as_str().to_owned()).collect(),
            &service_label,
            q.service.map(|s| s.as_str().to_owned()),
        ),
        pure_veg: except(Some(Key::PureVeg)).iter().filter(|d| d.is_pure_veg).count(),
    };
    (except(None), facets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        let known = vec!["Pizza".to_string(), "Live music".to_string()];
        let got = clean_labels(&["  pizza ".into(), "PIZZA".into(), "wood-fired   oven".into(), "".into(), "live MUSIC".into()], &known, 6);
        assert_eq!(got, vec!["Pizza", "Wood-fired oven", "Live music"]);
        assert_eq!(clean_labels(&["a".into(), "b".into(), "c".into()], &[], 2).len(), 2);
    }
}
