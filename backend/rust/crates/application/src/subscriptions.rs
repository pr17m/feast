//! Platform billing: restaurants pay Feast to stay listed after their free
//! trial. Prices and tax come from the restaurant's market; payment goes
//! through the market's payment provider (same `PaymentGateway` port as diner
//! payments), so adding a provider never touches this module.

use std::collections::HashMap;

use feast_domain::market::{plan_price, plans_for, tax_on};
use feast_domain::model::*;
use feast_domain::subscription::{self as sub, Renewal};
use feast_domain::DomainEvent;

use crate::context::Feast;
use crate::error::{AppError, AppResult};

/// Max free days an admin can grant at once.
const MAX_COMP_DAYS: i64 = 365;

impl Feast {
    fn rules_dto(&self) -> SubscriptionRules {
        let r = self.settings.subscription_rules;
        SubscriptionRules { trial_days: r.trial_days, grace_days: r.grace_days }
    }

    pub(crate) fn admin_ref(d: &RestaurantDetail) -> AdminRestaurantRef {
        AdminRestaurantRef {
            id: d.id.clone(),
            slug: d.slug.clone(),
            name: d.name.clone(),
            area: d.area.clone(),
            city_id: d.city_id.clone(),
            status: d.status,
            currency: d.currency.clone(),
            country_code: d.country_code.clone(),
        }
    }

    pub(crate) async fn owner_refs(&self, restaurant_id: &str) -> AppResult<Vec<OwnerRef>> {
        Ok(self.p.users.owners_of(restaurant_id).await?.into_iter().map(|u| OwnerRef { id: u.id, name: u.name, email: u.email }).collect())
    }

    pub(crate) async fn subscription_of(&self, restaurant_id: &str) -> AppResult<Subscription> {
        let rec = self.p.subscriptions.get_record(restaurant_id).await?;
        Ok(sub::view(restaurant_id, rec.as_ref(), self.now(), self.settings.subscription_rules))
    }

    /// The owner's billing page: status, plans in the market's currency, invoices.
    pub async fn partner_subscription(&self, user: &User, restaurant_id: &str) -> AppResult<SubscriptionOverview> {
        let r = self.managed_restaurant(user, restaurant_id).await?;
        let m = self.market(&r.detail.country_code).await?;
        let mut payments = self.p.subscriptions.payments_for(restaurant_id).await?;
        payments.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(SubscriptionOverview {
            subscription: self.subscription_of(restaurant_id).await?,
            plans: plans_for(&m),
            payments,
            rules: self.rules_dto(),
            methods: m.payments.methods.clone(),
        })
    }

    /// Start paying for a plan: a payment intent with the market's provider.
    /// The client completes it with `confirm_payment` (as for diner payments).
    pub async fn subscription_checkout(&self, user: &User, restaurant_id: &str, plan: PlanId) -> AppResult<PaymentIntent> {
        let r = self.editable_restaurant(user, restaurant_id).await?;
        if matches!(r.detail.status, RestaurantStatus::Onboarding | RestaurantStatus::PendingApproval) {
            return Err(AppError::conflict("NOT_APPROVED", "You can subscribe once your restaurant is approved. Your free trial starts then."));
        }
        if r.detail.status == RestaurantStatus::Suspended {
            return Err(AppError::conflict("SUSPENDED", "This restaurant is suspended. Contact Feast support."));
        }
        let m = self.market(&r.detail.country_code).await?;
        let price = plan_price(&m, plan);
        if price <= 0 {
            return Err(AppError::validation("INVALID_PLAN", "This plan isn't available"));
        }
        let total = price + tax_on(price, m.platform_tax_percent);
        self.new_intent(user, &m, PaymentPurpose::Subscription, restaurant_id, total, Some(plan)).await
    }

    #[allow(clippy::too_many_arguments)]
    fn platform_payment(&self, r: &RestaurantDetail, m: &Market, plan: &str, ren: &Renewal, amount: Paise, by: &str, seq: i64) -> PlatformPayment {
        let now = self.now();
        let tax = tax_on(amount, m.platform_tax_percent);
        PlatformPayment {
            id: self.p.ids.new_id("pp"),
            invoice_number: sub::invoice_number(now, seq),
            restaurant: r.reference(),
            plan_id: plan.into(),
            period_start: sub::iso(ren.period_start),
            period_end: sub::iso(ren.period_end),
            currency: m.currency.clone(),
            amount,
            tax,
            tax_name: m.tax_name.clone(),
            total: amount + tax,
            method: None,
            provider: None,
            status: PlatformPaymentStatus::Paid,
            failure_reason: None,
            note: None,
            paid_by: by.to_owned(),
            created_at: sub::iso(now),
        }
    }

    /// Settles a subscription intent the provider confirmed (via the browser's
    /// confirm or a webhook): new paid period + invoice + event, applied once.
    /// Returns false if the intent was already settled.
    pub(crate) async fn subscription_paid(&self, paid_by: &str, intent: &mut PaymentIntent, method: PaymentMethod) -> AppResult<bool> {
        let r = self.p.restaurants.get(&intent.reference_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
        let m = self.market(&r.detail.country_code).await?;
        let plan = intent.plan_id.ok_or_else(|| AppError::validation("INVALID_PLAN", "Missing plan"))?;
        let rec = self.p.subscriptions.get_record(&r.detail.id).await?.unwrap_or_default();
        let ren = sub::renew(&rec, plan, self.now());
        let seq = self.p.subscriptions.next_invoice_seq().await?;
        // Charge what the intent says (the price may have changed since checkout).
        let amount = (intent.amount as f64 / (1.0 + m.platform_tax_percent / 100.0)).round() as Paise;
        let mut pp = self.platform_payment(&r.detail, &m, plan.as_str(), &ren, amount, paid_by, seq);
        pp.tax = intent.amount - amount;
        pp.total = intent.amount;
        pp.currency = intent.currency.clone();
        pp.method = Some(method);
        pp.provider = Some(intent.provider.clone());
        intent.result_ref = Some(pp.id.clone());
        let ev = self.event(Some(&r.detail.id), Some(paid_by), DomainEvent::SubscriptionPaid { payment: pp.clone() });
        self.p.billing.complete_subscription_payment(intent, &r.detail.id, &ren.record, &pp, &[ev]).await
    }

    /// Record a declined subscription payment (shown in the owner's and admin's history).
    pub(crate) async fn subscription_payment_failed(&self, paid_by: &str, intent: &PaymentIntent, method: PaymentMethod, reason: &str) -> AppResult<()> {
        let Some(r) = self.p.restaurants.get(&intent.reference_id).await? else { return Ok(()) };
        let m = self.market(&r.detail.country_code).await?;
        let Some(plan) = intent.plan_id else { return Ok(()) };
        let rec = self.p.subscriptions.get_record(&r.detail.id).await?.unwrap_or_default();
        let ren = sub::renew(&rec, plan, self.now());
        let seq = self.p.subscriptions.next_invoice_seq().await?;
        let amount = (intent.amount as f64 / (1.0 + m.platform_tax_percent / 100.0)).round() as Paise;
        let mut pp = self.platform_payment(&r.detail, &m, plan.as_str(), &ren, amount, paid_by, seq);
        pp.tax = intent.amount - amount;
        pp.total = intent.amount;
        pp.method = Some(method);
        pp.provider = Some(intent.provider.clone());
        pp.status = PlatformPaymentStatus::Failed;
        pp.failure_reason = Some(reason.to_owned());
        self.p.subscriptions.record_payment(&r.detail.id, None, &pp, &[]).await
    }

    // ------------------------------------------------------------ admin

    pub async fn admin_subscriptions(&self, user: &User) -> AppResult<Vec<AdminSubscriptionRow>> {
        self.require_role(user, &[Role::Admin])?;
        let records = self.p.subscriptions.all_records().await?;
        let mut paid: HashMap<String, (Paise, Option<String>)> = HashMap::new();
        for p in self.p.subscriptions.all_payments().await?.into_iter().filter(|p| p.status == PlatformPaymentStatus::Paid) {
            let e = paid.entry(p.restaurant.id.clone()).or_default();
            e.0 += p.total;
            if e.1.as_deref().is_none_or(|t| t < p.created_at.as_str()) {
                e.1 = Some(p.created_at.clone());
            }
        }
        let now = self.now();
        let mut rows = Vec::new();
        for r in self.p.restaurants.list().await? {
            let d = &r.detail;
            let (total_paid, last) = paid.remove(&d.id).unwrap_or_default();
            rows.push(AdminSubscriptionRow {
                restaurant: Self::admin_ref(d),
                owners: self.owner_refs(&d.id).await?,
                subscription: sub::view(&d.id, records.get(&d.id), now, self.settings.subscription_rules),
                total_paid,
                last_payment_at: last,
            });
        }
        rows.sort_by(|a, b| a.restaurant.name.cmp(&b.restaurant.name));
        Ok(rows)
    }

    pub async fn platform_payments(&self, user: &User) -> AppResult<Vec<PlatformPayment>> {
        self.require_role(user, &[Role::Admin])?;
        let mut list = self.p.subscriptions.all_payments().await?;
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(list)
    }

    /// Admin grants free days (goodwill, outages…). Recorded as a zero-value "COMP" invoice.
    pub async fn extend_subscription(&self, user: &User, restaurant_id: &str, days: i64, note: Option<String>) -> AppResult<PaySubscriptionResult> {
        self.require_role(user, &[Role::Admin])?;
        if !(1..=MAX_COMP_DAYS).contains(&days) {
            return Err(AppError::validation("INVALID_DAYS", format!("Enter between 1 and {MAX_COMP_DAYS} days")));
        }
        let r = self.p.restaurants.get(restaurant_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
        let m = self.market(&r.detail.country_code).await?;
        let rec = self.p.subscriptions.get_record(restaurant_id).await?.unwrap_or_default();
        let ren = sub::extend(&rec, days, self.now());
        let seq = self.p.subscriptions.next_invoice_seq().await?;
        let mut pp = self.platform_payment(&r.detail, &m, "COMP", &ren, 0, &user.id, seq);
        pp.note = Some(note.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("{days} free days")));
        self.p.subscriptions.record_payment(restaurant_id, Some(&ren.record), &pp, &[]).await?;
        Ok(PaySubscriptionResult { subscription: self.subscription_of(restaurant_id).await?, payment: pp })
    }

    pub(crate) async fn subscription_metrics(&self) -> AppResult<SubscriptionMetrics> {
        let now = self.now();
        let rules = self.settings.subscription_rules;
        let records = self.p.subscriptions.all_records().await?;
        let restaurants = self.p.restaurants.list().await?;
        let views: Vec<Subscription> = restaurants.iter().map(|r| sub::view(&r.detail.id, records.get(&r.detail.id), now, rules)).collect();
        let count = |s: SubscriptionStatus| views.iter().filter(|v| v.status == s).count();
        let month = &sub::iso(now)[..7];
        let mut revenue: std::collections::BTreeMap<String, SubscriptionRevenue> = Default::default();
        for p in self.p.subscriptions.all_payments().await?.into_iter().filter(|p| p.status == PlatformPaymentStatus::Paid) {
            let e = revenue.entry(p.currency.clone()).or_insert_with(|| SubscriptionRevenue { currency: p.currency.clone(), this_month: 0, total: 0, mrr: 0 });
            e.total += p.amount;
            if p.created_at.starts_with(month) {
                e.this_month += p.amount;
            }
        }
        // MRR: each active paid plan's price per month, in its market's currency.
        let markets: HashMap<String, Market> = self.p.markets.list_markets().await?.into_iter().map(|m| (m.country_code.clone(), m)).collect();
        for (r, v) in restaurants.iter().zip(&views) {
            let (Some(plan), SubscriptionStatus::Active) = (v.plan_id, v.status) else { continue };
            let Some(m) = markets.get(&r.detail.country_code) else { continue };
            let e = revenue.entry(m.currency.clone()).or_insert_with(|| SubscriptionRevenue { currency: m.currency.clone(), this_month: 0, total: 0, mrr: 0 });
            e.mrr += plan_price(m, plan) / plan.months() as Paise;
        }
        Ok(SubscriptionMetrics {
            trial: count(SubscriptionStatus::Trial),
            active: count(SubscriptionStatus::Active),
            past_due: count(SubscriptionStatus::PastDue),
            expired: count(SubscriptionStatus::Expired),
            trials_ending_soon: views.iter().filter(|v| v.status == SubscriptionStatus::Trial && v.days_left.is_some_and(|d| d <= 7)).count(),
            revenue: revenue.into_values().collect(),
        })
    }

    /// Queue due reminders (see `processes::run_reminder_sweeper`). Returns how many.
    pub async fn sweep_reminders(&self) -> AppResult<usize> {
        const SUB_DAYS: [i64; 3] = [7, 3, 1];
        const DOC_DAYS: [i64; 3] = [30, 7, 1];
        let now = self.now();
        let rules = self.settings.subscription_rules;
        let records = self.p.subscriptions.all_records().await?;
        let markets: HashMap<String, Market> = self.p.markets.list_markets().await?.into_iter().map(|m| (m.country_code.clone(), m)).collect();
        let mut docs: HashMap<String, Vec<ComplianceDocument>> = HashMap::new();
        for d in self.p.compliance.all_documents().await? {
            docs.entry(d.restaurant_id.clone()).or_default().push(d);
        }
        let tz = self.tz_by_restaurant().await?;
        let mut events = vec![];
        for r in self.p.restaurants.list().await? {
            let d = &r.detail;
            if !matches!(d.status, RestaurantStatus::Live | RestaurantStatus::Paused) {
                continue;
            }
            let v = sub::view(&d.id, records.get(&d.id), now, rules);
            if let (Some(days), Some(end), SubscriptionStatus::Trial | SubscriptionStatus::Active) = (v.days_left, &v.ends_at, v.status) {
                if SUB_DAYS.contains(&days) {
                    let id = format!("evt_rem_sub_{}_{}_{}", d.id, &end[..10.min(end.len())], days);
                    let ev = DomainEvent::SubscriptionExpiring {
                        restaurant: d.reference(),
                        ends_at: end.clone(),
                        days_left: days,
                        trial: v.status == SubscriptionStatus::Trial,
                    };
                    events.push(feast_domain::EventEnvelope::new(id, self.p.clock.now_iso(), Some(d.id.clone()), None, ev));
                }
            }
            let Some(m) = markets.get(&d.country_code) else { continue };
            let z = tz.get(&d.id).cloned().unwrap_or_default();
            let today = self.p.clock.local_date(&z, 0);
            let Ok(today) = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d") else { continue };
            let (items, _) = self.compliance_items(m, docs.get(&d.id).map(Vec::as_slice).unwrap_or(&[]), &z);
            for it in items.iter().filter(|i| i.state == RequirementState::Approved) {
                let Some(exp) = it.document.as_ref().and_then(|x| x.expires_on.clone()) else { continue };
                let Ok(e) = chrono::NaiveDate::parse_from_str(&exp, "%Y-%m-%d") else { continue };
                let days = (e - today).num_days();
                if DOC_DAYS.contains(&days) {
                    let id = format!("evt_rem_doc_{}_{}_{}_{}", d.id, it.requirement.id, exp, days);
                    let ev = DomainEvent::DocumentExpiring {
                        restaurant: d.reference(),
                        requirement_name: it.requirement.name.clone(),
                        expires_on: exp,
                        days_left: days,
                    };
                    events.push(feast_domain::EventEnvelope::new(id, self.p.clock.now_iso(), Some(d.id.clone()), None, ev));
                }
            }
        }
        // Deterministic ids: the notification service dedupes on event id.
        if !events.is_empty() {
            self.p.outbox.append(&events).await?;
        }
        Ok(events.len())
    }
}
