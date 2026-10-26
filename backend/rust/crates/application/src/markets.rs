//! Market (country) administration. Each deployment serves one country: its
//! market and cities are seeded from `config/countries/<cc>.yaml` on first
//! start, then admins can tune them here (tax, plan prices, documents…).
//! Payment provider *credentials* never live here — only the provider id,
//! which must be one this server has configured.

use std::collections::HashSet;

use feast_domain::model::*;

use crate::context::Feast;
use crate::error::{AppError, AppResult};

fn invalid(code: &'static str, msg: impl Into<String>) -> AppError {
    AppError::validation(code, msg)
}

impl Feast {
    /// Payment providers this server can use (configured or not).
    pub fn payment_providers(&self, user: &User) -> AppResult<Vec<PaymentProviderInfo>> {
        self.require_role(user, &[Role::Admin])?;
        Ok(self.p.payments.list())
    }

    /// Update this deployment's market. The country is fixed by the deployment;
    /// currency can't change once restaurants exist (amounts are stored in it).
    pub async fn save_market(&self, user: &User, country_code: &str, mut m: Market) -> AppResult<Market> {
        self.require_role(user, &[Role::Admin])?;
        let cc = country_code.to_uppercase();
        if cc != self.settings.country {
            return Err(invalid("WRONG_COUNTRY", format!("This deployment serves {}. Other countries have their own deployment.", self.settings.country)));
        }
        m.country_code = cc.clone();
        m.currency = m.currency.trim().to_uppercase();
        m.payments.public_key = None;
        let existing = self.p.markets.get_market(&cc).await?;
        if let Some(old) = &existing {
            let has_restaurants = self.p.restaurants.list().await?.iter().any(|r| r.detail.country_code == cc);
            if has_restaurants && (old.currency != m.currency || old.currency_decimals != m.currency_decimals) {
                return Err(invalid("CURRENCY_LOCKED", "The currency can't change once restaurants are listed"));
            }
        }
        self.validate_market(&m)?;
        self.p.markets.upsert_market(&m).await?;
        m.payments.public_key = self.p.payments.get(&m.payments.provider).and_then(|g| g.public_key());
        Ok(m)
    }

    fn validate_market(&self, m: &Market) -> AppResult<()> {
        if m.name.trim().is_empty() {
            return Err(invalid("INVALID_MARKET", "Enter the country name"));
        }
        if m.currency.len() != 3 || !m.currency.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(invalid("INVALID_CURRENCY", "Use a 3-letter ISO currency code, e.g. INR or AUD"));
        }
        if m.currency_decimals > 3 {
            return Err(invalid("INVALID_CURRENCY", "Currency decimals must be 0–3"));
        }
        for (label, v) in [("Food tax", m.food_tax_percent), ("Platform tax", m.platform_tax_percent)] {
            if !(0.0..=50.0).contains(&v) {
                return Err(invalid("INVALID_TAX", format!("{label} must be between 0% and 50%")));
            }
        }
        if m.tax_name.trim().is_empty() {
            return Err(invalid("INVALID_TAX", "Enter the tax name, e.g. GST or VAT"));
        }
        let [a, b, c] = m.price_bands;
        if !(a > 0 && a < b && b < c) {
            return Err(invalid("INVALID_PRICE_BANDS", "Price band limits must be increasing"));
        }
        if m.cover_charge_per_pair < 0 || m.tip_presets.iter().any(|t| *t < 0) || m.tip_presets.len() > 6 {
            return Err(invalid("INVALID_AMOUNT", "Amounts can't be negative (max 6 tip presets)"));
        }
        let p = &m.plan_prices;
        if p.monthly <= 0 || p.quarterly <= 0 || p.yearly <= 0 {
            return Err(invalid("INVALID_PLAN", "Every plan needs a price"));
        }
        let gateway = self
            .p
            .payments
            .get(&m.payments.provider)
            .ok_or_else(|| invalid("INVALID_PROVIDER", format!("Payment provider \"{}\" isn't available on this server", m.payments.provider)))?;
        let supported = gateway.info().methods;
        if m.payments.methods.is_empty() || m.payments.methods.iter().any(|x| !supported.contains(x)) {
            return Err(invalid("INVALID_METHODS", format!("Pick payment methods {} supports", gateway.info().name)));
        }
        if !m.phone.dial_code.starts_with('+') || m.phone.dial_code.len() < 2 || regex::Regex::new(&m.phone.pattern).is_err() {
            return Err(invalid("INVALID_PHONE_RULES", "Check the dial code (e.g. +61) and number pattern"));
        }
        let mut ids = HashSet::new();
        for d in &m.documents {
            if d.id.trim().is_empty() || d.name.trim().is_empty() || !ids.insert(d.id.as_str()) {
                return Err(invalid("INVALID_DOCUMENTS", "Every document needs a unique id and a name"));
            }
        }
        if !m.documents.iter().any(|d| d.required) {
            return Err(invalid("INVALID_DOCUMENTS", "Require at least one licence or registration document"));
        }
        Ok(())
    }

    /// Add a city to this deployment's market.
    pub async fn create_city(&self, user: &User, req: CreateCityRequest) -> AppResult<City> {
        self.require_role(user, &[Role::Admin])?;
        let name = req.name.trim();
        if name.chars().count() < 2 {
            return Err(invalid("INVALID_NAME", "Enter the city name"));
        }
        let cc = req.country_code.to_uppercase();
        if cc != self.settings.country {
            return Err(invalid("WRONG_COUNTRY", format!("This deployment serves {}", self.settings.country)));
        }
        self.market(&cc).await?;
        if req.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(invalid("INVALID_TIMEZONE", "Use an IANA time zone, e.g. Australia/Sydney"));
        }
        if req.food_tax_percent.is_some_and(|t| !(0.0..=50.0).contains(&t)) {
            return Err(invalid("INVALID_TAX", "Tax must be between 0% and 50%"));
        }
        if !(-90.0..=90.0).contains(&req.lat) || !(-180.0..=180.0).contains(&req.lng) {
            return Err(invalid("INVALID_LOCATION", "Check the coordinates"));
        }
        let id: String = name.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        if id.is_empty() {
            return Err(invalid("INVALID_NAME", "Use latin letters in the city name"));
        }
        if self.p.markets.get_city(&id).await?.is_some() {
            return Err(AppError::conflict("DUPLICATE", "This city already exists"));
        }
        let areas: Vec<String> = req.areas.iter().map(|a| a.trim().to_owned()).filter(|a| !a.is_empty()).collect();
        let city = City {
            id,
            name: name.to_owned(),
            country_code: cc,
            timezone: req.timezone,
            food_tax_percent: req.food_tax_percent,
            areas,
            lat: req.lat,
            lng: req.lng,
        };
        self.p.markets.upsert_city(&city).await?;
        Ok(city)
    }
}
