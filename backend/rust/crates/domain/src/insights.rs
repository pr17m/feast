//! Insights: the numbers behind the owner scorecard ("did Feast pay for
//! itself?"), the diner's savings page and the admin marketplace health page.
//!
//! The application layer gathers bookings, bill payments, reviews and
//! cases for a period; everything here is pure arithmetic on them so it can be
//! unit-tested and ported rule-for-rule. Money is always in minor units of one
//! currency — callers never add amounts across currencies.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::model::{Booking, BookingStatus, IsoDate, Paise, RestaurantRef, SubscriptionMetrics};
use crate::trust::{Case, CaseRole, CaseStatus, FEAST_DECISION_HOURS};

/// Periods offered by the insights screens (days). 0 = all time (diner only).
pub const OWNER_PERIODS: [i64; 3] = [7, 30, 90];
pub const DINER_PERIODS: [i64; 4] = [30, 90, 365, 0];

pub const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

// ------------------------------------------------------------------ contract

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InsightsPeriod {
    /// Length in days; 0 = all time.
    pub days: i64,
    /// First local date included (absent for all time).
    pub from: Option<IsoDate>,
    /// Last local date included (today).
    pub to: IsoDate,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BookingFunnel {
    /// Bookings for slots in the period (not cancelled), up to today.
    pub booked: usize,
    /// Of those, diners who came (seated or completed).
    pub honoured: usize,
    pub no_show: usize,
    pub cancelled: usize,
    /// Confirmed bookings still to come (today or later).
    pub upcoming: usize,
}

impl BookingFunnel {
    /// No-shows as a share of bookings whose outcome is known.
    pub fn no_show_rate(&self) -> Option<f64> {
        rate(self.no_show, self.honoured + self.no_show)
    }

    pub fn add(&mut self, b: &Booking, today: &str) {
        match b.status {
            BookingStatus::PendingPayment => {}
            BookingStatus::Cancelled => self.cancelled += 1,
            BookingStatus::NoShow => {
                self.booked += 1;
                self.no_show += 1;
            }
            BookingStatus::Seated | BookingStatus::Completed => {
                self.booked += 1;
                self.honoured += 1;
            }
            BookingStatus::Confirmed => {
                if b.date.as_str() >= today {
                    self.upcoming += 1;
                } else {
                    self.booked += 1;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WeekdayStat {
    /// "Mon" … "Sun".
    pub day: String,
    pub bookings: usize,
    pub covers: i64,
    pub revenue: Paise,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HourStat {
    /// Local hour of the booked slot, 0–23.
    pub hour: u32,
    pub bookings: usize,
    pub covers: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonthRating {
    /// "2026-10".
    pub month: String,
    pub reviews: usize,
    pub average: f64,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DealUsage {
    /// Bills paid on Feast that carried a Feast deal.
    pub deals_used: usize,
    /// Discount the restaurant gave on those bills.
    pub discounts_given: Paise,
    pub daily_deal_limit: Option<i64>,
    /// Deals available over the period under the daily limit.
    pub deals_available: Option<i64>,
    /// deals_used ÷ deals_available.
    pub budget_used: Option<f64>,
    pub max_discount_per_bill: Option<Paise>,
}

/// The owner's "did Feast pay for itself?" scorecard.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OwnerInsights {
    pub restaurant: RestaurantRef,
    pub currency: String,
    pub period: InsightsPeriod,
    /// Distinct diners who visited through Feast (a kept booking or a bill paid on Feast).
    pub feast_diners: usize,
    /// First visit ever through Feast falls in the period.
    pub new_diners: usize,
    /// Visited through Feast before, or more than once in the period.
    pub repeat_diners: usize,
    pub visits: usize,
    /// Guests on kept bookings.
    pub covers: i64,
    pub bookings: BookingFunnel,
    pub no_show_rate: Option<f64>,
    /// Bills paid on Feast (what diners paid, after the deal and tip).
    pub revenue: Paise,
    pub bills_paid: usize,
    pub average_bill: Option<Paise>,
    /// Bills Feast diners paid at the counter and the restaurant confirmed
    /// (settled offline, not through Feast).
    #[serde(default)]
    pub offline_revenue: Paise,
    #[serde(default)]
    pub offline_bills: usize,
    /// Cover charges diners paid upfront to hold tables.
    pub cover_charges: Paise,
    pub deals: DealUsage,
    /// Subscription cost for the period (each paid invoice spread over the days it covers).
    pub feast_cost: Paise,
    /// Revenue through Feast per unit of Feast cost.
    pub return_on_cost: Option<f64>,
    pub by_weekday: Vec<WeekdayStat>,
    pub by_hour: Vec<HourStat>,
    /// Rating shown to diners now.
    pub rating: f64,
    pub rating_count: i64,
    /// Feast reviews by month, oldest first (last 6 months).
    pub rating_trend: Vec<MonthRating>,
    pub complaints_opened: usize,
    /// Complaints and reports not yet resolved (any age).
    pub open_cases: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FavouriteRestaurant {
    pub restaurant: RestaurantRef,
    pub visits: usize,
    pub spent: Paise,
    pub saved: Paise,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NameCount {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonthMoney {
    pub month: String,
    pub spent: Paise,
    pub saved: Paise,
}

/// A diner's own savings and dining history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DinerInsights {
    pub currency: String,
    pub period: InsightsPeriod,
    /// Discounts on bills paid on Feast.
    pub total_saved: Paise,
    /// Paid through Feast: bills plus upfront cover charges.
    pub total_spent: Paise,
    pub bills_paid: usize,
    pub visits: usize,
    pub restaurants_visited: usize,
    pub bookings: BookingFunnel,
    pub reviews_written: usize,
    pub favourite_restaurants: Vec<FavouriteRestaurant>,
    pub favourite_cuisines: Vec<NameCount>,
    /// Oldest first.
    pub by_month: Vec<MonthMoney>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MarketMoney {
    pub currency: String,
    /// Bills paid on Feast.
    pub gmv: Paise,
    pub bills_paid: usize,
    pub average_bill: Option<Paise>,
    /// Discounts restaurants funded on those bills.
    pub discounts: Paise,
    /// Upfront cover charges.
    pub cover_charges: Paise,
    /// Feast's own revenue in the period: subscription invoices paid.
    pub subscription_revenue: Paise,
    pub mrr: Paise,
    /// Bills paid at restaurants' counters and confirmed by them: settled
    /// offline, not through Feast (not in `gmv`).
    #[serde(default)]
    pub offline_settled: Paise,
    #[serde(default)]
    pub offline_bills: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CityHealth {
    pub city_id: String,
    pub city_name: String,
    pub currency: String,
    pub live_restaurants: usize,
    pub bookings: usize,
    pub gmv: Paise,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DisputeStats {
    /// Complaints and reports opened in the period.
    pub opened: usize,
    pub per_100_visits: Option<f64>,
    /// Escalated cases Feast decided in the period.
    pub decided_by_feast: usize,
    /// Share of those decided within the 72-hour promise.
    pub decided_on_time: Option<f64>,
    /// Open now, waiting on Feast.
    pub with_feast_now: usize,
    pub open_now: usize,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewHealth {
    pub posted: usize,
    pub average: Option<f64>,
    pub hidden: usize,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopRestaurant {
    pub restaurant: RestaurantRef,
    pub city_name: String,
    pub currency: String,
    pub bookings: usize,
    pub gmv: Paise,
    pub rating: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestaurantPipeline {
    pub live: usize,
    pub pending_approval: usize,
    pub onboarding: usize,
    pub paused_or_suspended: usize,
}

/// Marketplace health for Feast admins.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdminInsights {
    /// Dates are UTC for the marketplace as a whole.
    pub period: InsightsPeriod,
    pub restaurants: RestaurantPipeline,
    pub subscriptions: SubscriptionMetrics,
    pub money: Vec<MarketMoney>,
    pub bookings: BookingFunnel,
    pub no_show_rate: Option<f64>,
    pub visits: usize,
    /// Distinct diners with a visit in the period.
    pub active_diners: usize,
    /// Diner accounts created in the period.
    pub new_diners: usize,
    pub disputes: DisputeStats,
    pub reviews: ReviewHealth,
    pub cities: Vec<CityHealth>,
    pub top_restaurants: Vec<TopRestaurant>,
}

// ------------------------------------------------------------------ rules

/// `part ÷ whole`, or `None` when there's nothing to divide by.
pub fn rate(part: usize, whole: usize) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

pub fn average(total: Paise, count: usize) -> Option<Paise> {
    (count > 0).then(|| (total as f64 / count as f64).round() as Paise)
}

/// Monday = 0 … Sunday = 6.
pub fn weekday_index(date: &str) -> Option<usize> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok().map(|d| d.weekday().num_days_from_monday() as usize)
}

/// Hour of an `HH:MM` slot.
pub fn slot_hour(hhmm: &str) -> Option<u32> {
    hhmm.split(':').next()?.parse().ok().filter(|h| *h < 24)
}

fn parse(iso: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(iso).ok().map(|d| d.with_timezone(&Utc))
}

/// The share of a paid invoice that falls in `[from, to)`: each invoice is
/// spread evenly over the time it covers.
pub fn prorated(amount: Paise, period_start: &str, period_end: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> Paise {
    let (Some(s), Some(e)) = (parse(period_start), parse(period_end)) else { return 0 };
    if e <= s || to <= from {
        return 0;
    }
    let overlap = (e.min(to) - s.max(from)).num_seconds();
    if overlap <= 0 {
        return 0;
    }
    (amount as f64 * overlap as f64 / (e - s).num_seconds() as f64).round() as Paise
}

/// A visit: one diner at one restaurant on one local date.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Visit {
    pub user_id: String,
    pub restaurant_id: String,
    pub date: String,
}

/// Distinct visits.
pub fn unique_visits(visits: impl IntoIterator<Item = Visit>) -> Vec<Visit> {
    let set: std::collections::BTreeSet<Visit> = visits.into_iter().collect();
    set.into_iter().collect()
}

/// New vs repeat diners in `[from, to]`, given every visit ever (any order).
/// New: their first visit falls in the period and they came once in it.
/// Repeat: visited before the period, or more than once within it.
pub fn new_and_repeat(all: &[Visit], from: &str, to: &str) -> (usize, usize) {
    let mut first: HashMap<&str, &str> = HashMap::new();
    let mut in_period: HashMap<&str, usize> = HashMap::new();
    for v in all {
        let f = first.entry(v.user_id.as_str()).or_insert(v.date.as_str());
        if v.date.as_str() < *f {
            *f = v.date.as_str();
        }
        if v.date.as_str() >= from && v.date.as_str() <= to {
            *in_period.entry(v.user_id.as_str()).or_default() += 1;
        }
    }
    let mut new = 0;
    let mut repeat = 0;
    for (user, n) in in_period {
        if first.get(user).is_some_and(|f| *f >= from) && n == 1 {
            new += 1;
        } else {
            repeat += 1;
        }
    }
    (new, repeat)
}

/// Whether an escalated case was decided by Feast within its promise
/// (`None` if Feast hasn't decided it).
pub fn decided_on_time(c: &Case) -> Option<bool> {
    let o = c.outcome.as_ref()?;
    if o.decided_by != CaseRole::Feast {
        return None;
    }
    let escalated = parse(c.escalated_at.as_deref()?)?;
    let decided = parse(&o.at)?;
    Some(decided - escalated <= chrono::Duration::hours(FEAST_DECISION_HOURS))
}

pub fn is_open(c: &Case) -> bool {
    c.status != CaseStatus::Resolved
}

/// Bookings and covers by weekday (Mon first), with revenue by local payment date.
pub fn by_weekday<'a>(bookings: impl IntoIterator<Item = &'a Booking>, revenue_by_date: &[(String, Paise)]) -> Vec<WeekdayStat> {
    let mut out: Vec<WeekdayStat> = WEEKDAYS.iter().map(|d| WeekdayStat { day: (*d).to_owned(), bookings: 0, covers: 0, revenue: 0 }).collect();
    for b in bookings {
        if let Some(i) = weekday_index(&b.date) {
            out[i].bookings += 1;
            out[i].covers += b.guests;
        }
    }
    for (date, amount) in revenue_by_date {
        if let Some(i) = weekday_index(date) {
            out[i].revenue += amount;
        }
    }
    out
}

/// Bookings and covers by slot hour, only the hours that have any.
pub fn by_hour<'a>(bookings: impl IntoIterator<Item = &'a Booking>) -> Vec<HourStat> {
    let mut map: BTreeMap<u32, (usize, i64)> = BTreeMap::new();
    for b in bookings {
        if let Some(h) = slot_hour(&b.time) {
            let e = map.entry(h).or_default();
            e.0 += 1;
            e.1 += b.guests;
        }
    }
    map.into_iter().map(|(hour, (bookings, covers))| HourStat { hour, bookings, covers }).collect()
}

/// Top `n` by count, ties by name.
pub fn top_counts(items: impl IntoIterator<Item = String>, n: usize) -> Vec<NameCount> {
    let mut map: HashMap<String, usize> = HashMap::new();
    for i in items {
        *map.entry(i).or_default() += 1;
    }
    let mut v: Vec<NameCount> = map.into_iter().map(|(name, count)| NameCount { name, count }).collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    v.truncate(n);
    v
}

/// Average rating per month for the last `months` months ending at `to_month` ("YYYY-MM").
pub fn rating_trend(ratings: &[(String, i64)], to_month: &str, months: usize) -> Vec<MonthRating> {
    let mut keys = vec![];
    let mut cur = to_month.to_owned();
    for _ in 0..months {
        keys.push(cur.clone());
        cur = previous_month(&cur);
    }
    keys.reverse();
    keys.into_iter()
        .map(|m| {
            let rs: Vec<i64> = ratings.iter().filter(|(month, _)| month.starts_with(&m)).map(|(_, r)| *r).collect();
            let average = if rs.is_empty() { 0.0 } else { (rs.iter().sum::<i64>() as f64 / rs.len() as f64 * 10.0).round() / 10.0 };
            MonthRating { month: m, reviews: rs.len(), average }
        })
        .collect()
}

pub fn previous_month(ym: &str) -> String {
    let (y, m) = ym.split_once('-').and_then(|(y, m)| Some((y.parse::<i32>().ok()?, m.parse::<u32>().ok()?))).unwrap_or((1970, 1));
    if m <= 1 {
        format!("{:04}-12", y - 1)
    } else {
        format!("{:04}-{:02}", y, m - 1)
    }
}

/// Deal budget usage over a period of `days`.
pub fn deal_usage(deals_used: usize, discounts_given: Paise, daily_limit: Option<i64>, max_per_bill: Option<Paise>, days: i64) -> DealUsage {
    let deals_available = daily_limit.map(|l| l.max(0) * days.max(1));
    DealUsage {
        deals_used,
        discounts_given,
        daily_deal_limit: daily_limit,
        deals_available,
        budget_used: deals_available.and_then(|a| rate(deals_used, a as usize)),
        max_discount_per_bill: max_per_bill,
    }
}

/// Distinct ids.
pub fn distinct<'a>(ids: impl IntoIterator<Item = &'a str>) -> usize {
    ids.into_iter().collect::<HashSet<_>>().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(u: &str, d: &str) -> Visit {
        Visit { user_id: u.into(), restaurant_id: "r".into(), date: d.into() }
    }

    #[test]
    fn rates_and_averages() {
        assert_eq!(rate(1, 4), Some(0.25));
        assert_eq!(rate(1, 0), None);
        assert_eq!(average(1000, 3), Some(333));
        assert_eq!(average(1000, 0), None);
    }

    #[test]
    fn weekdays_and_hours() {
        assert_eq!(weekday_index("2026-10-26"), Some(0)); // Monday
        assert_eq!(weekday_index("2026-10-25"), Some(6)); // Sunday
        assert_eq!(weekday_index("nope"), None);
        assert_eq!(slot_hour("19:30"), Some(19));
        assert_eq!(slot_hour("25:00"), None);
    }

    #[test]
    fn invoices_spread_over_the_days_they_cover() {
        let from = parse("2026-10-01T00:00:00Z").unwrap();
        let to = parse("2026-10-11T00:00:00Z").unwrap();
        // A 30-day A$45 invoice: 10 of its days fall in the period.
        assert_eq!(prorated(4500, "2026-10-01T00:00:00Z", "2026-10-31T00:00:00Z", from, to), 1500);
        // Entirely before the period.
        assert_eq!(prorated(4500, "2026-08-01T00:00:00Z", "2026-08-31T00:00:00Z", from, to), 0);
        // Bad dates count as nothing rather than failing.
        assert_eq!(prorated(4500, "x", "y", from, to), 0);
    }

    #[test]
    fn new_versus_repeat_diners() {
        let all = vec![v("a", "2026-09-01"), v("a", "2026-10-05"), v("b", "2026-10-03"), v("c", "2026-10-02"), v("c", "2026-10-09"), v("d", "2026-08-01")];
        // a came before the period, c twice in it: repeat. b is new. d didn't come in the period.
        assert_eq!(new_and_repeat(&all, "2026-10-01", "2026-10-10"), (1, 2));
    }

    #[test]
    fn rating_trend_fills_empty_months() {
        let r = vec![("2026-10-02T00:00:00Z".to_owned(), 5), ("2026-10-09T00:00:00Z".to_owned(), 4), ("2026-08-01T00:00:00Z".to_owned(), 3)];
        let t = rating_trend(&r, "2026-10", 3);
        assert_eq!(t.iter().map(|m| m.month.as_str()).collect::<Vec<_>>(), ["2026-08", "2026-09", "2026-10"]);
        assert_eq!((t[0].reviews, t[1].reviews, t[2].reviews), (1, 0, 2));
        assert_eq!(t[2].average, 4.5);
        assert_eq!(previous_month("2026-01"), "2025-12");
    }

    #[test]
    fn deal_budget_usage() {
        let u = deal_usage(30, 50_000, Some(5), Some(10_000), 7);
        assert_eq!(u.deals_available, Some(35));
        assert!((u.budget_used.unwrap() - 30.0 / 35.0).abs() < 1e-9);
        assert_eq!(deal_usage(3, 0, None, None, 7).budget_used, None);
    }

    #[test]
    fn top_counts_breaks_ties_by_name() {
        let t = top_counts(["b", "a", "b", "c", "a"].map(String::from), 2);
        assert_eq!(t.iter().map(|n| (n.name.as_str(), n.count)).collect::<Vec<_>>(), [("a", 2), ("b", 2)]);
    }
}
