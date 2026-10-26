//! The application service: one struct holding the ports, with use cases split
//! across modules by bounded context (auth, discovery, booking, restaurant QR, billing,
//! partner, admin, notifications). Delivery layers (HTTP, gRPC, workers) call
//! these methods; they never touch repositories directly.

use std::sync::Arc;

use std::collections::{HashMap, HashSet};

use feast_domain::model::{City, ComplianceDocument, ComplianceItem, Market, MarketStatus, Restaurant, RestaurantDetail, Role, User};
use feast_domain::{DomainEvent, EventEnvelope};

use crate::error::{AppError, AppResult};
use crate::ports::*;

/// Everything the use cases depend on, as trait objects (dependency inversion).
#[derive(Clone)]
pub struct Ports {
    pub users: Arc<dyn UserRepository>,
    pub restaurants: Arc<dyn RestaurantRepository>,
    pub bookings: Arc<dyn BookingRepository>,
    pub billing: Arc<dyn BillingRepository>,
    pub outbox: Arc<dyn OutboxRepository>,
    pub markets: Arc<dyn MarketRepository>,
    pub subscriptions: Arc<dyn SubscriptionRepository>,
    pub compliance: Arc<dyn ComplianceRepository>,
    pub cases: Arc<dyn CaseRepository>,
    pub reviews: Arc<dyn ReviewRepository>,
    pub audit: Arc<dyn AuditRepository>,
    /// Append-only audit trail of every state change.
    pub trail: Arc<dyn AuditTrailRepository>,
    /// Bills paid at the counter, awaiting the restaurant's confirmation.
    pub offline: Arc<dyn OfflinePaymentRepository>,
    pub files: Arc<dyn FileStorage>,
    /// Photos for dishes the owner hasn't photographed (stock search; no-op when unconfigured).
    pub dish_images: Arc<dyn DishImageFinder>,
    pub geo: Arc<dyn GeoLocator>,
    pub demo: Option<Arc<dyn DemoDataPort>>,
    pub notifications: Arc<dyn NotificationGateway>,
    /// Payment providers; each market names the one it uses.
    pub payments: Arc<dyn PaymentProviders>,
    pub otp: Arc<dyn OtpService>,
    /// Enabled social login providers (may be empty).
    pub identity: Vec<Arc<dyn IdentityProvider>>,
    pub tokens: Arc<dyn TokenService>,
    pub passwords: Arc<dyn PasswordHasher>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGenerator>,
}

/// Behaviour switches that aren't infrastructure.
#[derive(Debug, Clone)]
pub struct AppSettings {
    /// Expose demo-only endpoints (data reset).
    pub demo_mode: bool,
    /// The country this deployment serves (ISO code), e.g. "IN".
    pub country: String,
    /// Where other countries are served: country code → base URL.
    pub directory: std::collections::BTreeMap<String, String>,
    pub subscription_rules: feast_domain::subscription::Rules,
    /// Max upload size for documents (bytes).
    pub max_upload_bytes: usize,
    /// Initial password for partner/admin accounts created by an admin (demo).
    /// Production should replace this with an invite/reset-link flow.
    pub initial_partner_password: String,
}

#[derive(Clone)]
pub struct Feast {
    pub(crate) p: Ports,
    pub(crate) settings: AppSettings,
}

impl Feast {
    pub fn new(ports: Ports, settings: AppSettings) -> Self {
        Self { p: ports, settings }
    }

    pub fn ports(&self) -> &Ports {
        &self.p
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    // ------------------------------------------------------------ helpers

    pub(crate) fn event(&self, restaurant_id: Option<&str>, user_id: Option<&str>, event: DomainEvent) -> EventEnvelope {
        EventEnvelope::new(self.p.ids.new_id("evt"), self.p.clock.now_iso(), restaurant_id.map(str::to_owned), user_id.map(str::to_owned), event)
            .by(crate::actor::current())
    }

    pub(crate) fn require_role(&self, user: &User, roles: &[Role]) -> AppResult<()> {
        if roles.contains(&user.role) {
            Ok(())
        } else {
            Err(AppError::forbidden("You don't have access to this"))
        }
    }

    /// Load a restaurant the caller manages (OWNER of it, or ADMIN).
    pub(crate) async fn managed_restaurant(&self, user: &User, restaurant_id: &str) -> AppResult<Restaurant> {
        self.require_role(user, &[Role::Owner, Role::Admin])?;
        let r = self.p.restaurants.get(restaurant_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
        if !user.can_manage(restaurant_id) {
            return Err(AppError::forbidden("You don't manage this restaurant"));
        }
        Ok(r)
    }

    /// A restaurant customers may see, by slug or id (see [`Feast::is_listed`]).
    pub(crate) async fn public_restaurant(&self, slug_or_id: &str) -> AppResult<Restaurant> {
        let r = match self.p.restaurants.get_by_slug(slug_or_id).await? {
            Some(r) => Some(r),
            None => self.p.restaurants.get(slug_or_id).await?,
        };
        match r {
            Some(r) if self.is_listed(&r.detail).await? => Ok(r.customer_view()),
            _ => Err(AppError::not_found("Restaurant")),
        }
    }

    /// Visible to diners only when: approved (LIVE/PAUSED), every required
    /// document approved and unexpired, subscription not lapsed, market live.
    pub(crate) async fn is_listed(&self, d: &RestaurantDetail) -> AppResult<bool> {
        if !d.status.is_public() {
            return Ok(false);
        }
        let Some(market) = self.p.markets.get_market(&d.country_code).await? else { return Ok(false) };
        if market.status != MarketStatus::Live {
            return Ok(false);
        }
        let sub = self.p.subscriptions.get_record(&d.id).await?;
        if !feast_domain::subscription::view(&d.id, sub.as_ref(), self.now(), self.settings.subscription_rules).listed {
            return Ok(false);
        }
        let docs = self.p.compliance.documents_for(&d.id).await?;
        let tz = self.city_tz(&d.city_id).await?;
        Ok(self.compliance_items(&market, &docs, &tz).1)
    }

    /// Ids of the listed restaurants among `rs` (batched: one query per table).
    pub(crate) async fn listed_ids(&self, rs: &[Restaurant]) -> AppResult<HashSet<String>> {
        let markets: HashMap<String, Market> = self.p.markets.list_markets().await?.into_iter().map(|m| (m.country_code.clone(), m)).collect();
        let cities: HashMap<String, City> = self.p.markets.list_cities().await?.into_iter().map(|c| (c.id.clone(), c)).collect();
        let subs = self.p.subscriptions.all_records().await?;
        let mut docs: HashMap<String, Vec<ComplianceDocument>> = HashMap::new();
        for d in self.p.compliance.all_documents().await? {
            docs.entry(d.restaurant_id.clone()).or_default().push(d);
        }
        let now = self.now();
        let mut out = HashSet::new();
        for r in rs {
            let d = &r.detail;
            let Some(m) = markets.get(&d.country_code).filter(|m| m.status == MarketStatus::Live) else { continue };
            if !d.status.is_public() || !feast_domain::subscription::view(&d.id, subs.get(&d.id), now, self.settings.subscription_rules).listed {
                continue;
            }
            let tz = cities.get(&d.city_id).map(|c| c.timezone.clone()).unwrap_or_default();
            if self.compliance_items(m, docs.get(&d.id).map(Vec::as_slice).unwrap_or(&[]), &tz).1 {
                out.insert(d.id.clone());
            }
        }
        Ok(out)
    }

    /// Compliance items for a restaurant, and whether they're complete.
    pub(crate) fn compliance_items(&self, m: &Market, docs: &[ComplianceDocument], tz: &str) -> (Vec<ComplianceItem>, bool) {
        let today = self.p.clock.local_date(tz, 0);
        let soon = self.p.clock.local_date(tz, 30);
        let items = feast_domain::compliance::items(&m.documents, docs, &today, &soon);
        let complete = feast_domain::compliance::complete(&items);
        (items, complete)
    }

    pub(crate) fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp_millis(self.p.clock.now_ms()).unwrap_or_else(chrono::Utc::now)
    }

    pub(crate) async fn market(&self, country_code: &str) -> AppResult<Market> {
        self.p.markets.get_market(country_code).await?.ok_or_else(|| AppError::not_found("Market"))
    }

    /// The restaurant's market and its city's effective food tax.
    pub(crate) async fn pricing_for(&self, d: &RestaurantDetail) -> AppResult<(Market, f64)> {
        let m = self.market(&d.country_code).await?;
        let city = self.p.markets.get_city(&d.city_id).await?;
        let tax = feast_domain::market::food_tax_for(&m, city.as_ref());
        Ok((m, tax))
    }

    /// IANA time zone of a city ("" → the clock's default).
    pub(crate) async fn city_tz(&self, city_id: &str) -> AppResult<String> {
        Ok(self.p.markets.get_city(city_id).await?.map(|c| c.timezone).unwrap_or_default())
    }

    /// Time zone per restaurant id (for lists spanning restaurants).
    pub(crate) async fn tz_by_restaurant(&self) -> AppResult<HashMap<String, String>> {
        let cities: HashMap<String, String> = self.p.markets.list_cities().await?.into_iter().map(|c| (c.id, c.timezone)).collect();
        Ok(self.p.restaurants.city_ids().await?.into_iter().map(|(rid, cid)| (rid, cities.get(&cid).cloned().unwrap_or_default())).collect())
    }

    /// An E.164 phone number valid for one of the configured markets.
    pub(crate) async fn is_valid_phone(&self, e164: &str) -> AppResult<bool> {
        Ok(valid_phone(&self.p.markets.list_markets().await?, e164))
    }

    pub(crate) fn initial_password(&self) -> String {
        self.settings.initial_partner_password.clone()
    }
}

/// An E.164 phone number valid for one of `markets` (e.g. "+919876543210").
pub(crate) fn valid_phone(markets: &[Market], e164: &str) -> bool {
    feast_domain::market::market_for_phone(markets, e164).is_some()
}
