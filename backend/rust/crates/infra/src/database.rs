//! Choose the persistence adapter from a database URL.
//!
//! `postgres://user:pass@host/db` or `sqlite://data/feast.db` (or
//! `sqlite::memory:`) → [`crate::sql::SqlStore`]. Another database (e.g.
//! DynamoDB) = implement the repository traits in a new module + a match arm here.
//! Business code never changes — it only sees the traits.

use std::sync::Arc;

use async_trait::async_trait;
use feast_application::ports::*;
use feast_application::{AppError, AppResult};

/// Every repository port, backed by one database.
#[derive(Clone)]
pub struct Repositories {
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
    pub trail: Arc<dyn AuditTrailRepository>,
    pub offline: Arc<dyn OfflinePaymentRepository>,
    /// Phones registered for push notifications.
    pub push_tokens: Arc<dyn PushTokenRepository>,
    pub refunds: Arc<dyn RefundRepository>,
    /// Rate limits, code challenges and leases shared across instances.
    pub shared: Arc<dyn SharedStateRepository>,
    /// Admin-level operations that aren't business ports.
    pub admin: Arc<dyn DatabaseAdmin>,
}

/// Maintenance operations on the database (not used by business code).
#[async_trait]
pub trait DatabaseAdmin: Send + Sync {
    /// Delete all data, keep the schema (demo reset).
    async fn wipe(&self) -> AppResult<()>;
    /// True when there are no restaurants yet (first boot → seed).
    async fn is_empty(&self) -> AppResult<bool>;
    /// Cheap connectivity check for health probes.
    async fn ping(&self) -> AppResult<()>;
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

/// Open the database named by `cfg.url`, run migrations, return the ports.
pub async fn connect(cfg: &DatabaseConfig) -> AppResult<Repositories> {
    let scheme = cfg.url.split(':').next().unwrap_or_default();
    match scheme {
        #[cfg(feature = "sql")]
        "sqlite" | "postgres" | "postgresql" => {
            let store = Arc::new(crate::sql::SqlStore::connect(&cfg.url, cfg.max_connections).await?);
            Ok(Repositories {
                users: store.clone(),
                restaurants: store.clone(),
                bookings: store.clone(),
                billing: store.clone(),
                outbox: store.clone(),
                markets: store.clone(),
                subscriptions: store.clone(),
                compliance: store.clone(),
                cases: store.clone(),
                reviews: store.clone(),
                audit: store.clone(),
                trail: store.clone(),
                offline: store.clone(),
                push_tokens: store.clone(),
                refunds: store.clone(),
                shared: store.clone(),
                admin: store,
            })
        }
        "mysql" => Err(AppError::validation(
            "UNSUPPORTED_DATABASE",
            format!(
                "No {scheme} adapter yet: implement the repository ports in crates/infra/src/{scheme} and register it in database.rs (see docs/SCALING.md)"
            ),
        )),
        other => Err(AppError::validation("UNSUPPORTED_DATABASE", format!("Unknown database URL scheme '{other}'"))),
    }
}
