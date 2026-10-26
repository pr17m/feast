//! Subscription rules (platform billing for restaurants). Mirrors
//! `webapp/src/lib/api/subscription.ts`.
//!
//! - Free trial of `trial_days` from when a restaurant first goes LIVE.
//! - Paying a plan extends the paid period from max(now, current end).
//! - After the period ends: `grace_days` of PAST_DUE (still listed), then
//!   EXPIRED (hidden from diners until paid).

use chrono::{DateTime, Datelike, Duration, Months, SecondsFormat, Utc};

use crate::model::{IsoDateTime, PlanId, Subscription, SubscriptionRecord, SubscriptionStatus};

#[derive(Debug, Clone, Copy)]
pub struct Rules {
    pub trial_days: i64,
    pub grace_days: i64,
}

impl Default for Rules {
    fn default() -> Self {
        Self { trial_days: 30, grace_days: 7 }
    }
}

pub fn parse(iso: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(iso).ok().map(|d| d.with_timezone(&Utc))
}

pub fn iso(t: DateTime<Utc>) -> IsoDateTime {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Calendar months, clamping to month end (31 Jan + 1 month → 28/29 Feb), like the webapp.
pub fn add_months(t: DateTime<Utc>, months: i32) -> DateTime<Utc> {
    let r = if months >= 0 { t.checked_add_months(Months::new(months as u32)) } else { t.checked_sub_months(Months::new((-months) as u32)) };
    r.unwrap_or(t)
}

/// The end of whatever the restaurant currently has (trial or paid).
pub fn period_end(rec: &SubscriptionRecord) -> Option<DateTime<Utc>> {
    [rec.trial_ends_at.as_deref(), rec.paid_through.as_deref()].into_iter().flatten().filter_map(parse).max()
}

pub fn view(restaurant_id: &str, rec: Option<&SubscriptionRecord>, now: DateTime<Utc>, rules: Rules) -> Subscription {
    let Some((rec, end)) = rec.and_then(|r| period_end(r).map(|e| (r, e))) else {
        return Subscription {
            restaurant_id: restaurant_id.into(),
            status: SubscriptionStatus::NotStarted,
            plan_id: None,
            trial_ends_at: None,
            paid_through: None,
            ends_at: None,
            hidden_after: None,
            days_left: None,
            listed: false,
        };
    };
    let hidden_after = end + Duration::days(rules.grace_days);
    let paid = rec.paid_through.as_deref().and_then(parse).is_some_and(|p| p > now);
    let status = if now < end {
        if paid {
            SubscriptionStatus::Active
        } else {
            SubscriptionStatus::Trial
        }
    } else if now < hidden_after {
        SubscriptionStatus::PastDue
    } else {
        SubscriptionStatus::Expired
    };
    let ms = (end - now).num_milliseconds() as f64;
    Subscription {
        restaurant_id: restaurant_id.into(),
        status,
        plan_id: rec.plan_id,
        trial_ends_at: rec.trial_ends_at.clone(),
        paid_through: rec.paid_through.clone(),
        ends_at: Some(iso(end)),
        hidden_after: Some(iso(hidden_after)),
        days_left: Some((ms / 86_400_000.0).ceil() as i64),
        listed: status != SubscriptionStatus::Expired,
    }
}

/// Start the free trial the first time a restaurant goes live.
pub fn start_trial(rec: Option<SubscriptionRecord>, now: DateTime<Utc>, rules: Rules) -> SubscriptionRecord {
    match rec {
        Some(r) if r.trial_ends_at.is_some() => r,
        other => SubscriptionRecord { trial_ends_at: Some(iso(now + Duration::days(rules.trial_days))), ..other.unwrap_or_default() },
    }
}

pub struct Renewal {
    pub record: SubscriptionRecord,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

fn start_from(rec: &SubscriptionRecord, now: DateTime<Utc>) -> DateTime<Utc> {
    period_end(rec).filter(|e| *e > now).unwrap_or(now)
}

/// New paid period for a plan, starting at max(now, current end).
pub fn renew(rec: &SubscriptionRecord, plan: PlanId, now: DateTime<Utc>) -> Renewal {
    let start = start_from(rec, now);
    let end = add_months(start, plan.months() as i32);
    Renewal { record: SubscriptionRecord { paid_through: Some(iso(end)), plan_id: Some(plan), ..rec.clone() }, period_start: start, period_end: end }
}

/// Admin-granted free days: extends the trial if never paid, else the paid period.
pub fn extend(rec: &SubscriptionRecord, days: i64, now: DateTime<Utc>) -> Renewal {
    let start = start_from(rec, now);
    let end = start + Duration::days(days);
    let record = if rec.paid_through.is_some() {
        SubscriptionRecord { paid_through: Some(iso(end)), ..rec.clone() }
    } else {
        SubscriptionRecord { trial_ends_at: Some(iso(end)), ..rec.clone() }
    };
    Renewal { record, period_start: start, period_end: end }
}

/// Invoice numbers like FST-202610-00042.
pub fn invoice_number(at: DateTime<Utc>, seq: i64) -> String {
    format!("FST-{}{:02}-{:05}", at.year(), at.month(), seq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn lifecycle() {
        let r = Rules::default();
        let t0 = Utc.with_ymd_and_hms(2026, 1, 31, 10, 0, 0).unwrap();
        assert_eq!(view("r", None, t0, r).status, SubscriptionStatus::NotStarted);
        let rec = start_trial(None, t0, r);
        let v = view("r", Some(&rec), t0, r);
        assert_eq!((v.status, v.days_left, v.listed), (SubscriptionStatus::Trial, Some(30), true));
        // Paying during the trial starts after the trial.
        let paid = renew(&rec, PlanId::Monthly, t0 + Duration::days(10));
        assert_eq!(paid.period_start, t0 + Duration::days(30));
        assert_eq!(view("r", Some(&paid.record), t0 + Duration::days(10), r).status, SubscriptionStatus::Active);
        // Past the end: grace, then expired.
        let end = paid.period_end;
        assert_eq!(view("r", Some(&paid.record), end + Duration::days(3), r).status, SubscriptionStatus::PastDue);
        let exp = view("r", Some(&paid.record), end + Duration::days(8), r);
        assert_eq!((exp.status, exp.listed), (SubscriptionStatus::Expired, false));
        // Renewing when expired starts now.
        let back = renew(&paid.record, PlanId::Yearly, end + Duration::days(8));
        assert_eq!(back.period_start, end + Duration::days(8));
        // Month-end clamping.
        assert_eq!(add_months(t0, 1), Utc.with_ymd_and_hms(2026, 2, 28, 10, 0, 0).unwrap());
        // Extending a never-paid trial moves the trial end.
        let ext = extend(&rec, 14, t0);
        assert!(ext.record.paid_through.is_none());
        assert_eq!(ext.period_end, t0 + Duration::days(44));
        assert_eq!(invoice_number(t0, 42), "FST-202601-00042");
    }
}
