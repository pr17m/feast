//! Payment providers (adapters for the `PaymentGateway` port).
//!
//! Each market (country) names the provider it uses — e.g. `razorpay` for
//! India, `stripe` for Australia — in `config/countries/<cc>.yaml`. Credentials
//! come from `[payments.providers.<id>]` (env vars in production).
//!
//! **Adding a provider** (e.g. Adyen): write `adyen.rs` implementing
//! `PaymentGateway`, then add one line to [`FACTORIES`]. Nothing else in the
//! codebase changes: use cases only see the port (Open/Closed principle).

pub mod mock;
pub mod razorpay;
pub mod stripe;

use std::collections::BTreeMap;
use std::sync::Arc;

use feast_application::ports::{PaymentGateway, PaymentProviders};
use feast_application::{AppError, AppResult};
use feast_domain::model::PaymentProviderInfo;
use serde::de::DeserializeOwned;

pub use mock::MockPaymentGateway;
pub use razorpay::RazorpayGateway;
pub use stripe::StripeGateway;

/// Builds a provider from its config table.
pub type Factory = fn(&serde_json::Value) -> AppResult<Arc<dyn PaymentGateway>>;

/// Every provider this build knows: `id → factory`.
pub const FACTORIES: &[(&str, Factory)] = &[
    ("mock", |v| Ok(Arc::new(MockPaymentGateway::new(parse::<mock::MockConfig>("mock", v)?)))),
    ("razorpay", |v| Ok(Arc::new(RazorpayGateway::new(parse("razorpay", v)?)?))),
    ("stripe", |v| Ok(Arc::new(StripeGateway::new(parse("stripe", v)?)?))),
];

fn parse<T: DeserializeOwned>(id: &str, v: &serde_json::Value) -> AppResult<T> {
    serde_json::from_value(v.clone()).map_err(|e| AppError::internal(format!("payments.providers.{id}: {e}")))
}

/// The providers configured on this server.
#[derive(Default, Clone)]
pub struct PaymentRegistry {
    gateways: BTreeMap<String, Arc<dyn PaymentGateway>>,
    /// Route every market to this provider (development/test).
    forced: Option<String>,
}

impl PaymentRegistry {
    /// Build from `[payments.providers.*]` tables. Unknown ids are an error.
    pub fn from_config(providers: &BTreeMap<String, serde_json::Value>) -> AppResult<Self> {
        let mut gateways = BTreeMap::new();
        for (id, cfg) in providers {
            let enabled = match cfg.get("enabled") {
                Some(serde_json::Value::Bool(b)) => *b,
                Some(serde_json::Value::String(s)) => s.eq_ignore_ascii_case("true"),
                _ => true,
            };
            if !enabled {
                continue;
            }
            let factory = FACTORIES
                .iter()
                .find(|(k, _)| k == id)
                .map(|(_, f)| f)
                .ok_or_else(|| AppError::internal(format!("unknown payment provider '{id}' (known: {})", known())))?;
            gateways.insert(id.clone(), factory(cfg)?);
        }
        Ok(Self { gateways, forced: None })
    }

    /// Send every market to `id` (e.g. test payments in development).
    pub fn force(mut self, id: Option<String>) -> Self {
        self.forced = id.filter(|s| !s.is_empty());
        self
    }

    /// Register a gateway directly (tests, custom builds).
    pub fn with(mut self, id: &str, gateway: Arc<dyn PaymentGateway>) -> Self {
        self.gateways.insert(id.to_owned(), gateway);
        self
    }

    pub fn ids(&self) -> Vec<String> {
        self.gateways.keys().cloned().collect()
    }
}

fn known() -> String {
    FACTORIES.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
}

impl PaymentProviders for PaymentRegistry {
    fn get(&self, id: &str) -> Option<Arc<dyn PaymentGateway>> {
        self.gateways.get(self.forced.as_deref().unwrap_or(id)).cloned()
    }
    fn list(&self) -> Vec<PaymentProviderInfo> {
        self.gateways.values().map(|g| g.info()).collect()
    }
}

/// Shared HTTP client for provider APIs.
pub(crate) fn http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("feast-backend")
        .build()
        .map_err(|e| AppError::internal(format!("http client: {e}")))
}

pub(crate) fn provider_err(provider: &str, e: impl std::fmt::Display) -> AppError {
    tracing::error!(provider, error = %e, "payment provider call failed");
    AppError::new(feast_application::ErrorKind::Unavailable, "PAYMENTS_UNAVAILABLE", "The payment provider isn't responding. Please try again.")
}

/// Constant-time comparison for signatures.
/// A webhook signing secret that is actually set. Deploy tooling writes "-"
/// for unset optional secrets (some secret stores reject empty values); a
/// guessable secret would let anyone forge webhooks, so those count as unset.
pub(crate) fn webhook_secret(s: &str) -> Option<&str> {
    let s = s.trim();
    (s.len() >= 8 && s != "-").then_some(s)
}

pub(crate) fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
