//! Seeding.
//!
//! - [`ensure_market`]: on every start, creates this deployment's market and
//!   cities from its country file (`config/countries/<cc>.yaml`) if missing.
//! - [`Seeder`]: optional demo data for that country (restaurants, partner and
//!   diner accounts, approved documents, subscriptions, a week of history),
//!   written through the repository ports so it works with any database.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Duration;
use feast_application::ports::{Clock, DemoDataPort, FileStorage, PasswordHasher, ReviewInfo};
use feast_application::{AppError, AppResult};
use feast_domain::market::{plan_price, tax_on};
use feast_domain::model::*;
use feast_domain::slots::demand_hash as hash;
use feast_domain::subscription::{self as sub, add_months};
use serde::Deserialize;

use crate::database::Repositories;

/// The bundled demo dataset (all launch countries; filtered per deployment).
pub const BUNDLED_DEMO: &str = include_str!("../../../../seed/demo.json");

/// A deployment's country: what `config/countries/<cc>.yaml` defines.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryFile {
    pub country: CountryInfo,
    pub market: Market,
    pub cities: Vec<City>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryInfo {
    pub code: String,
    pub default_city: String,
}

impl CountryFile {
    pub fn from_yaml(raw: &str) -> AppResult<Self> {
        let f: Self = serde_yaml::from_str(raw).map_err(|e| AppError::validation("BAD_COUNTRY_FILE", e.to_string()))?;
        f.validate()?;
        Ok(f)
    }

    /// Load `<dir>/<cc>.yaml` (e.g. `config/countries/au.yaml`).
    pub fn load(dir: &std::path::Path, country: &str) -> AppResult<Self> {
        let path = dir.join(format!("{}.yaml", country.to_lowercase()));
        let raw = std::fs::read_to_string(&path).map_err(|e| AppError::validation("COUNTRY_NOT_FOUND", format!("{}: {e}", path.display())))?;
        Self::from_yaml(&raw).map_err(|e| AppError::validation("BAD_COUNTRY_FILE", format!("{}: {}", path.display(), e.message)))
    }

    /// Check the file is internally consistent (run at startup).
    pub fn validate(&self) -> AppResult<()> {
        let cc = &self.market.country_code;
        let bad = |m: String| Err(AppError::validation("BAD_COUNTRY_FILE", m));
        if !self.country.code.eq_ignore_ascii_case(cc) {
            return bad(format!("country.code {} != market.countryCode {cc}", self.country.code));
        }
        if self.cities.is_empty() {
            return bad("at least one city is required".into());
        }
        if let Some(c) = self.cities.iter().find(|c| &c.country_code != cc) {
            return bad(format!("city {} has countryCode {} (expected {cc})", c.id, c.country_code));
        }
        if let Some(c) = self.cities.iter().find(|c| c.timezone.parse::<chrono_tz::Tz>().is_err()) {
            return bad(format!("city {} has an invalid timezone {:?}", c.id, c.timezone));
        }
        if !self.cities.iter().any(|c| c.id == self.country.default_city) {
            return bad(format!("country.defaultCity {} is not one of the cities", self.country.default_city));
        }
        Ok(())
    }
}

/// Create the market and cities from the country file when they don't exist
/// yet; with `overwrite`, replace them (`feast markets sync`). Returns whether
/// anything was written.
pub async fn ensure_market(repos: &Repositories, file: &CountryFile, overwrite: bool) -> AppResult<bool> {
    let existing = repos.markets.get_market(&file.market.country_code).await?;
    let mut wrote = false;
    if existing.is_none() || overwrite {
        if let Some(old) = &existing {
            let has_data = repos.restaurants.list().await?.iter().any(|r| r.detail.country_code == old.country_code);
            if has_data && (old.currency != file.market.currency || old.currency_decimals != file.market.currency_decimals) {
                return Err(AppError::validation("CURRENCY_LOCKED", "The country file changes the currency, but restaurants already exist"));
            }
        }
        repos.markets.upsert_market(&file.market).await?;
        wrote = true;
    }
    for c in &file.cities {
        if overwrite || repos.markets.get_city(&c.id).await?.is_none() {
            repos.markets.upsert_city(c).await?;
            wrote = true;
        }
    }
    if wrote {
        tracing::info!(country = %file.market.country_code, cities = file.cities.len(), overwrite, "market loaded from country file");
    }
    Ok(wrote)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoSeed {
    pub users: Vec<User>,
    pub demo_otp: String,
    pub demo_password: String,
    pub restaurants: Vec<Restaurant>,
}

impl DemoSeed {
    pub fn bundled() -> Self {
        serde_json::from_str(BUNDLED_DEMO).expect("bundled seed/demo.json is valid")
    }

    pub fn from_file(path: &str) -> AppResult<Self> {
        let raw = std::fs::read_to_string(path).map_err(|e| AppError::validation("SEED_NOT_FOUND", format!("{path}: {e}")))?;
        serde_json::from_str(&raw).map_err(|e| AppError::validation("BAD_SEED", format!("{path}: {e}")))
    }

    /// Only what belongs to one country: its restaurants, their owners, admins,
    /// and diners with that country's phone numbers.
    pub fn for_country(&self, m: &Market) -> DemoSeed {
        let restaurants: Vec<Restaurant> = self.restaurants.iter().filter(|r| r.detail.country_code == m.country_code).cloned().collect();
        let ids: HashSet<&str> = restaurants.iter().map(|r| r.detail.id.as_str()).collect();
        let users = self
            .users
            .iter()
            .filter_map(|u| match u.role {
                Role::Admin => Some(u.clone()),
                Role::Owner => {
                    let mine: Vec<String> = u.restaurant_ids.iter().filter(|r| ids.contains(r.as_str())).cloned().collect();
                    (!mine.is_empty()).then(|| User { restaurant_ids: mine, ..u.clone() })
                }
                Role::Customer => u.phone.as_deref().is_some_and(|p| p.starts_with(&m.phone.dial_code)).then(|| u.clone()),
            })
            .collect();
        DemoSeed { users, restaurants, demo_otp: self.demo_otp.clone(), demo_password: self.demo_password.clone() }
    }
}

/// Demo subscription states: (restaurant, plan, paid periods, period ends in N days).
const SUB_SEED: &[(&str, Option<PlanId>, u32, i64)] = &[
    ("r_olive_ember", Some(PlanId::Monthly), 4, 18),
    ("r_saffron_courtyard", Some(PlanId::Quarterly), 2, 50),
    ("r_bamboo_lane", Some(PlanId::Yearly), 1, 200),
    ("r_coastal_tide", Some(PlanId::Monthly), 3, 6),
    ("r_daily_grind", None, 0, 12),
    ("r_ember_oak", Some(PlanId::Monthly), 2, 25),
    ("r_sugar_bloom", None, 0, 20),
    ("r_hops_republic", None, 0, 5),
    ("r_annapoorna", Some(PlanId::Quarterly), 1, 80),
    ("r_bombay_monsoon", Some(PlanId::Yearly), 1, 300),
    ("r_salt_water", Some(PlanId::Monthly), 2, -3),
    ("r_noodle_theory", Some(PlanId::Monthly), 5, 10),
    ("r_colaba_corner", None, 0, -12),
    ("r_smokehouse_54", None, 0, 27),
    ("r_harbour_vine", Some(PlanId::Monthly), 2, 21),
    ("r_pho_republic", None, 0, 9),
    ("r_laneway_grind", Some(PlanId::Yearly), 1, 240),
    ("r_fitzroy_fire", None, 0, 4),
    ("r_spice_route", Some(PlanId::Quarterly), 1, 40),
    ("r_swan_grill", Some(PlanId::Monthly), 3, 14),
];

pub struct Seeder {
    pub repos: Repositories,
    pub passwords: Arc<dyn PasswordHasher>,
    pub clock: Arc<dyn Clock>,
    pub files: Arc<dyn FileStorage>,
    pub country: CountryFile,
    /// The full dataset; filtered to `country` when seeding.
    pub seed: DemoSeed,
    /// Generate a week of past bookings and payments so dashboards aren't empty.
    pub with_history: bool,
}

impl Seeder {
    pub async fn run(&self) -> AppResult<()> {
        ensure_market(&self.repos, &self.country, false).await?;
        let m = &self.country.market;
        let seed = self.seed.for_country(m);
        for r in &seed.restaurants {
            self.repos.restaurants.insert(r).await?;
        }
        // Staff sign in with a password; diners use OTP / social login.
        let staff_hash = self.passwords.hash(&seed.demo_password)?;
        for u in &seed.users {
            let hash = matches!(u.role, Role::Owner | Role::Admin).then_some(staff_hash.as_str());
            self.repos.users.insert(u, hash).await?;
        }
        self.documents(&seed).await?;
        self.subscriptions(&seed).await?;
        if self.with_history {
            self.history(&seed).await?;
        }
        tracing::info!(country = %m.country_code, restaurants = seed.restaurants.len(), users = seed.users.len(), history = self.with_history, "demo data seeded");
        Ok(())
    }

    /// Copy the demo photos (cover, gallery, dishes) onto demo restaurants that
    /// are already in the database, leaving everything else untouched.
    /// Returns how many restaurants were updated.
    pub async fn refresh_photos(&self) -> AppResult<usize> {
        let seed = self.seed.for_country(&self.country.market);
        let mut updated = 0;
        for s in &seed.restaurants {
            let Some(mut r) = self.repos.restaurants.get(&s.detail.id).await? else { continue };
            r.detail.art.image = s.detail.art.image.clone();
            r.detail.art.image_credit = s.detail.art.image_credit.clone();
            for p in &mut r.detail.photos {
                if let Some(sp) = s.detail.photos.iter().find(|x| x.id == p.id) {
                    p.url = sp.url.clone();
                    p.credit = sp.credit.clone();
                }
            }
            for item in r.menu.iter_mut().flat_map(|c| c.items.iter_mut()) {
                if let Some(si) = s.menu.iter().flat_map(|c| c.items.iter()).find(|x| x.id == item.id) {
                    // Never overwrite a photo the owner uploaded, or bring back one they removed.
                    if feast_domain::media::source_of(item) != feast_domain::media::ImageSource::Owner && !item.stock_declined {
                        item.image = si.image.clone();
                        item.image_source = si.image_source;
                        item.image_credit = si.image_credit.clone();
                    }
                }
            }
            self.repos.restaurants.update(&r).await?;
            updated += 1;
        }
        tracing::info!(updated, "demo photos refreshed");
        Ok(updated)
    }

    fn owner_of(seed: &DemoSeed, rid: &str) -> String {
        seed.users.iter().find(|u| u.role == Role::Owner && u.restaurant_ids.iter().any(|x| x == rid)).map(|u| u.id.clone()).unwrap_or_else(|| "u_admin".into())
    }

    /// Every required document approved (with a placeholder file), so demo restaurants are listed.
    async fn documents(&self, seed: &DemoSeed) -> AppResult<()> {
        let m = &self.country.market;
        let today = chrono::NaiveDate::parse_from_str(&self.clock.local_date("", 0), "%Y-%m-%d").unwrap_or_default();
        for r in &seed.restaurants {
            let d = &r.detail;
            let owner = Self::owner_of(seed, &d.id);
            self.repos.compliance.set_review_info(&d.id, &ReviewInfo { notes: None, submitted_at: Some("2026-01-10T09:00:00.000Z".into()) }).await?;
            for (i, req) in m.documents.iter().filter(|q| q.required).enumerate() {
                let id = format!("doc_seed_{}_{}", d.id, req.id);
                let key = format!("documents/{}/{id}", d.id);
                let pdf = placeholder_pdf(&format!("{} - {} (demo document)", req.name, d.name));
                let size = pdf.len() as i64;
                self.files.put(&key, pdf, "application/pdf").await?;
                let expiring_soon = d.id == "r_smokehouse_54" && req.has_expiry && i == 0;
                let expires = today + Duration::days(if expiring_soon { 20 } else { 300 + 30 * i as i64 });
                self.repos
                    .compliance
                    .insert_document(&ComplianceDocument {
                        id,
                        restaurant_id: d.id.clone(),
                        requirement_id: req.id.clone(),
                        file_name: format!("{}-{}.pdf", req.id, d.slug),
                        file_type: "application/pdf".into(),
                        file_size: size,
                        storage_key: key,
                        number: req
                            .number_label
                            .as_ref()
                            .map(|_| format!("{}{:08}", req.id[..3.min(req.id.len())].to_uppercase(), hash(&format!("{}{}", d.id, req.id)) % 100_000_000)),
                        expires_on: req.has_expiry.then(|| expires.format("%Y-%m-%d").to_string()),
                        status: DocumentStatus::Approved,
                        review_note: None,
                        uploaded_at: "2026-01-08T09:00:00.000Z".into(),
                        uploaded_by: owner.clone(),
                        reviewed_at: Some("2026-01-12T09:00:00.000Z".into()),
                        reviewed_by: Some("u_admin".into()),
                    })
                    .await?;
            }
        }
        Ok(())
    }

    /// Trials and paid plans in various states (active, ending soon, past due, expired).
    async fn subscriptions(&self, seed: &DemoSeed) -> AppResult<()> {
        let m = &self.country.market;
        let now = chrono::DateTime::from_timestamp_millis(self.clock.now_ms()).unwrap_or_else(chrono::Utc::now);
        for r in &seed.restaurants {
            let d = &r.detail;
            let (plan, periods, ends_in) = SUB_SEED.iter().find(|s| s.0 == d.id).map(|s| (s.1, s.2, s.3)).unwrap_or((None, 0, 30));
            let end = now + Duration::days(ends_in);
            let Some(plan) = plan.filter(|_| periods > 0) else {
                self.repos.subscriptions.put_record(&d.id, &SubscriptionRecord { trial_ends_at: Some(sub::iso(end)), ..Default::default() }).await?;
                continue;
            };
            // Walk back from the end of the paid period to when the trial ended.
            let mut starts = vec![];
            let mut cursor = end;
            for _ in 0..periods {
                cursor = add_months(cursor, -(plan.months() as i32));
                starts.insert(0, cursor);
            }
            let rec = SubscriptionRecord { trial_ends_at: Some(sub::iso(starts[0])), paid_through: Some(sub::iso(end)), plan_id: Some(plan) };
            self.repos.subscriptions.put_record(&d.id, &rec).await?;
            let price = plan_price(m, plan);
            let tax = tax_on(price, m.platform_tax_percent);
            for (i, start) in starts.iter().enumerate() {
                let period_end = starts.get(i + 1).copied().unwrap_or(end);
                let paid_at = *start - Duration::days(2);
                let seq = self.repos.subscriptions.next_invoice_seq().await?;
                let p = PlatformPayment {
                    id: format!("pp_seed_{}_{i}", d.id),
                    invoice_number: sub::invoice_number(paid_at, seq),
                    restaurant: d.reference(),
                    plan_id: plan.as_str().into(),
                    period_start: sub::iso(*start),
                    period_end: sub::iso(period_end),
                    currency: m.currency.clone(),
                    amount: price,
                    tax,
                    tax_name: m.tax_name.clone(),
                    total: price + tax,
                    method: m.payments.methods.first().copied(),
                    provider: Some(m.payments.provider.clone()),
                    status: PlatformPaymentStatus::Paid,
                    failure_reason: None,
                    note: None,
                    paid_by: Self::owner_of(seed, &d.id),
                    created_at: sub::iso(paid_at),
                };
                self.repos.subscriptions.record_payment(&d.id, None, &p, &[]).await?;
            }
        }
        Ok(())
    }

    async fn history(&self, seed: &DemoSeed) -> AppResult<()> {
        let m = &self.country.market;
        let customers: Vec<&User> = seed.users.iter().filter(|u| u.role == Role::Customer).collect();
        if customers.is_empty() {
            return Ok(());
        }
        let unit = 10_i64.pow(m.currency_decimals);
        let methods = if m.payments.methods.is_empty() { vec![PaymentMethod::Card] } else { m.payments.methods.clone() };
        let now_ms = self.clock.now_ms();
        for r in &seed.restaurants {
            let d = &r.detail;
            let tz = self.country.cities.iter().find(|c| c.id == d.city_id).map(|c| c.timezone.clone()).unwrap_or_default();
            let hour_now = self.clock.local_minutes(&tz) / 60;
            let pct = d.offers.iter().find(|o| o.scope == OfferScope::Prebook).and_then(|o| o.percent_off).unwrap_or(0);
            for day in -7i64..=0 {
                let date = self.clock.local_date(&tz, day);
                let n = 1 + hash(&format!("{}{}", d.id, date)) % 4;
                for k in 0..n {
                    let h = hash(&format!("{}:{}:{}", d.id, date, k));
                    let u = customers[(h as usize) % customers.len()];
                    let hour = if day == 0 { (hour_now + 1 + k).clamp(19, 22) } else { 12 + h % 10 };
                    let time = format!("{hour:02}:{}", if h % 2 == 1 { "30" } else { "00" });
                    let status = if day == 0 {
                        BookingStatus::Confirmed
                    } else if h % 9 == 0 {
                        BookingStatus::NoShow
                    } else if h % 7 == 0 {
                        BookingStatus::Cancelled
                    } else {
                        BookingStatus::Completed
                    };
                    let id = format!("bk_seed_{}_{}_{}", d.id, date, k);
                    let booking = Booking {
                        id: id.clone(),
                        user_id: u.id.clone(),
                        code: format!("FST-{:0>3}{k}", to_base36(h % 46656).to_uppercase()),
                        restaurant: d.reference(),
                        date: date.clone(),
                        time: time.clone(),
                        guests: 2 + h % 5,
                        deal: (pct > 0).then(|| Offer {
                            id: format!("{}_slot_{pct}", d.id),
                            scope: OfferScope::Prebook,
                            title: format!("Flat {pct}% off"),
                            subtitle: None,
                            percent_off: Some(pct),
                            flat_off: None,
                            max_discount: None,
                            min_bill: None,
                            terms: vec![],
                            moderation: None,
                        }),
                        special_request: None,
                        contact: Contact { name: u.name.clone(), phone: u.phone.clone().unwrap_or_default() },
                        status,
                        cover_charge: 0,
                        currency: m.currency.clone(),
                        payment: None,
                        created_at: crate::clock::iso(ms_to_utc(now_ms + (day - 2) * 86_400_000)),
                        checked_in_at: None,
                    };
                    self.repos.bookings.insert(&booking, &id, &[]).await?;

                    if status == BookingStatus::Completed {
                        let amount = ((d.cost_for_two as f64 * (0.8 + (h % 9) as f64 / 10.0)) / unit as f64).round() as i64 * unit;
                        let discount = ((amount * pct) as f64 / 100.0 / unit as f64).floor() as i64 * unit;
                        let bill = Bill {
                            id: format!("bill_seed_{id}"),
                            restaurant_id: d.id.clone(),
                            bill_amount: amount,
                            lines: vec![
                                BillLine { label: "Bill amount".into(), amount, kind: BillLineKind::ItemTotal },
                                BillLine { label: format!("Flat {pct}% off"), amount: -discount, kind: BillLineKind::Discount },
                            ],
                            applied_offer: None,
                            savings: discount,
                            payable: amount - discount,
                            deal_notice: None,
                            booking_id: Some(id.clone()),
                            currency: m.currency.clone(),
                        };
                        self.repos.billing.insert_bill(&bill).await?;
                        let paid_ms = self.clock.local_to_ms(&tz, &date, &time).unwrap_or(now_ms) + 75 * 60_000;
                        let mut intent = PaymentIntent {
                            id: format!("pi_seed_{id}"),
                            purpose: PaymentPurpose::Bill,
                            reference_id: bill.id.clone(),
                            amount: bill.payable,
                            currency: m.currency.clone(),
                            provider: m.payments.provider.clone(),
                            checkout: None,
                            provider_ref: None,
                            plan_id: None,
                            user_id: Some(u.id.clone()),
                            status: PaymentStatus::Created,
                            result_ref: None,
                            failure_reason: None,
                        };
                        self.repos.billing.insert_intent(&intent, &[]).await?;
                        // Completing flips it to SUCCEEDED (the same claim real payments use).
                        intent.status = PaymentStatus::Succeeded;
                        intent.result_ref = Some(format!("bp_seed_{id}"));
                        let bp = BillPayment {
                            id: format!("bp_seed_{id}"),
                            user_id: u.id.clone(),
                            restaurant: d.reference(),
                            payment: PaymentRef {
                                settlement: Default::default(),
                                intent_id: intent.id.clone(),
                                method: methods[(h as usize) % methods.len()],
                                amount: bill.payable,
                                currency: m.currency.clone(),
                                provider: m.payments.provider.clone(),
                                paid_at: crate::clock::iso(ms_to_utc(paid_ms)),
                            },
                            bill,
                        };
                        self.repos.billing.complete_bill_payment(&intent, &bp, &[]).await?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// A one-page PDF with a line of text (demo documents).
pub fn placeholder_pdf(text: &str) -> Vec<u8> {
    let safe: String = text.chars().filter(|c| c.is_ascii() && !matches!(c, '(' | ')' | '\\')).collect();
    let content = format!("BT /F1 14 Tf 60 760 Td ({safe}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = vec![];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
    }
    let xref = out.len();
    out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
    for off in offsets {
        out.push_str(&format!("{off:010} 00000 n \n"));
    }
    out.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1));
    out.into_bytes()
}

fn ms_to_utc(ms: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(ms).unwrap_or_else(chrono::Utc::now)
}

fn to_base36(mut n: i64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = vec![];
    while n > 0 {
        out.push(D[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `DemoDataPort`: wipe everything and seed again.
pub struct DemoReset {
    pub seeder: Seeder,
}

#[async_trait]
impl DemoDataPort for DemoReset {
    async fn reset(&self) -> AppResult<()> {
        self.seeder.repos.admin.wipe().await?;
        self.seeder.run().await
    }
}
