//! Composition root: the only place that knows which adapter implements which
//! port. Change technology here (driven by config), never in business code.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use feast_api::ReadinessCheck;
use feast_application::ports::*;
use feast_application::{AppSettings, Feast, Ports};
use feast_domain::model::AuthProvider;
use feast_infra::auth::{Argon2Hasher, JwtTokenService};
use feast_infra::bus::{self, BusConfig, RedisBusConfig};
use feast_infra::clock::SystemClock;
use feast_infra::database::{self, DatabaseAdmin, DatabaseConfig, Repositories};
use feast_infra::geo::{ChainGeoLocator, HeaderGeoLocator, MaxMindGeoLocator};
use feast_infra::ids::UuidIds;
use feast_infra::media::{UnsplashConfig, UnsplashFinder};
use feast_infra::oauth::{self, OAuthMode, OAuthProviderConfig};
use feast_infra::otp::{FixedOtpService, LogOtpService, TwilioVerifyOtpService};
use feast_infra::payments::PaymentRegistry;
use feast_infra::push::{FcmPushSender, LogPushSender, ServiceAccount};
use feast_infra::seed::{ensure_market, CountryFile, DemoReset, DemoSeed, Seeder};
use feast_infra::storage::{AzureBlobConfig, AzureBlobStorage, LocalDiskStorage, S3Config, S3Storage};
use feast_notification::{GrpcNotificationGateway, NotificationService, SqlNotificationStore};

use crate::settings::Settings;

/// Shared infrastructure every role needs.
pub struct Infra {
    pub settings: Settings,
    pub bus: Arc<dyn MessageBus>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGenerator>,
    pub seed: DemoSeed,
    /// This deployment's country (config/countries/<cc>.yaml).
    pub country: CountryFile,
    pub files: Arc<dyn FileStorage>,
}

pub async fn infra(settings: Settings) -> Result<Infra> {
    let country = CountryFile::load(&settings.countries_dir(), settings.country()).map_err(|e| anyhow::anyhow!("{}", e.message))?;
    // Default zone for "local" times: the country's main city.
    let tz = country.cities.iter().find(|c| c.id == country.country.default_city).map(|c| c.timezone.clone()).unwrap_or_default();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::from_name(&tz).map_err(anyhow::Error::msg)?);
    let bus_cfg = match settings.bus.kind.as_str() {
        "redis" => BusConfig::Redis(RedisBusConfig {
            url: settings.bus.redis.url.clone(),
            stream: settings.bus.redis.stream.clone(),
            max_len: settings.bus.redis.max_len,
            block_ms: settings.bus.redis.block_ms,
            claim_idle_ms: settings.bus.redis.claim_idle_ms,
        }),
        _ => BusConfig::Memory { capacity: settings.bus.memory_capacity },
    };
    let bus = bus::connect(&bus_cfg).await.map_err(|e| anyhow::anyhow!("message bus: {e}"))?;
    let seed = if settings.app.seed_file.is_empty() {
        DemoSeed::bundled()
    } else {
        DemoSeed::from_file(&settings.app.seed_file).map_err(|e| anyhow::anyhow!("{e}"))?
    };
    let mut seed = seed;
    if !settings.app.seed_password.is_empty() {
        seed.demo_password = settings.app.seed_password.clone();
    }
    let files: Arc<dyn FileStorage> = match settings.storage.kind.as_str() {
        "s3" => {
            let cfg: S3Config = serde_json::from_value(settings.storage.s3.clone().unwrap_or_default()).context("storage.s3")?;
            Arc::new(S3Storage::new(cfg).map_err(|e| anyhow::anyhow!("{e}"))?)
        }
        "azure" => {
            let cfg: AzureBlobConfig = serde_json::from_value(settings.storage.azure.clone().unwrap_or_default()).context("storage.azure")?;
            Arc::new(AzureBlobStorage::new(cfg).map_err(|e| anyhow::anyhow!("{e}"))?)
        }
        _ => Arc::new(LocalDiskStorage::new(&settings.storage.local_dir)),
    };
    tracing::info!(country = %country.market.country_code, market = %country.market.name, currency = %country.market.currency, "deployment country");
    Ok(Infra { settings, bus, clock, ids: Arc::new(UuidIds), seed, country, files })
}

/// The notification service in-process (owns its database).
pub async fn notification_service(inf: &Infra) -> Result<Arc<NotificationService>> {
    let n = &inf.settings.notifications;
    ensure_sqlite_dir(&n.database_url)?;
    let store = SqlNotificationStore::connect(&n.database_url, n.max_connections).await.map_err(|e| anyhow::anyhow!("notification database: {e}"))?;
    Ok(Arc::new(NotificationService::new(Arc::new(store), inf.bus.clone(), inf.ids.clone(), inf.clock.clone())))
}

/// What the core uses to reach notifications: in-process, or over gRPC.
#[allow(clippy::large_enum_variant)] // one per process
pub enum Notifications {
    Embedded(Arc<NotificationService>),
    Grpc(GrpcNotificationGateway),
}

impl Notifications {
    pub fn gateway(&self) -> Arc<dyn NotificationGateway> {
        match self {
            Self::Embedded(s) => s.clone(),
            Self::Grpc(c) => Arc::new(c.clone()),
        }
    }
}

pub async fn notifications(inf: &Infra, caller: &str) -> Result<Notifications> {
    let n = &inf.settings.notifications;
    Ok(match n.mode.as_str() {
        "grpc" => Notifications::Grpc(
            GrpcNotificationGateway::new(&n.grpc_url, caller, Some(&n.service_token), Duration::from_millis(n.timeout_ms))
                .map_err(|e| anyhow::anyhow!("{e}"))?,
        ),
        _ => Notifications::Embedded(notification_service(inf).await?),
    })
}

/// Repositories + every port → the application service.
pub struct Core {
    pub feast: Arc<Feast>,
    pub repos: Repositories,
    pub notifications: Notifications,
}

pub async fn core(inf: &Infra, caller: &str) -> Result<Core> {
    let s = &inf.settings;
    ensure_sqlite_dir(&s.database.url)?;
    let repos = database::connect(&DatabaseConfig { url: s.database.url.clone(), max_connections: s.database.max_connections })
        .await
        .map_err(|e| anyhow::anyhow!("database: {e}"))?;
    let passwords: Arc<dyn PasswordHasher> = Arc::new(Argon2Hasher::default());
    let notifications = notifications(inf, caller).await?;

    let otp: Arc<dyn OtpService> = match s.auth.otp.provider.as_str() {
        "log" => Arc::new(LogOtpService::new(Duration::from_secs(s.auth.otp.ttl_secs), s.auth.otp.max_attempts, repos.shared.clone())),
        "twilio" => {
            let t = &s.auth.otp.twilio;
            Arc::new(TwilioVerifyOtpService::new(&t.account_sid, &t.auth_token, &t.service_sid, None).map_err(|e| anyhow::anyhow!("{e}"))?)
        }
        _ => Arc::new(FixedOtpService::new(&s.auth.otp.fixed_code, s.auth.otp.show_hint)),
    };
    // Market and cities exist from the first start (from the country file).
    ensure_market(&repos, &inf.country, false).await.map_err(|e| anyhow::anyhow!("loading market: {e}"))?;
    let payments = PaymentRegistry::from_config(&s.payments.providers).map_err(|e| anyhow::anyhow!("{e}"))?.force(Some(s.payments.force_provider.clone()));
    let market = repos.markets.get_market(&inf.country.market.country_code).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(m) = &market {
        if payments.get(&m.payments.provider).is_none() {
            let msg = format!(
                "{} uses payment provider '{}', which isn't enabled: add [payments.providers.{}] with its credentials (or set payments.force_provider=mock in development)",
                m.name, m.payments.provider, m.payments.provider
            );
            if s.is_production() {
                bail!(msg);
            }
            tracing::warn!("{msg}");
        }
    }
    let geo: Arc<dyn GeoLocator> = {
        let mut chain: Vec<Box<dyn GeoLocator>> = vec![Box::new(HeaderGeoLocator::new(s.geo.headers.clone()))];
        if !s.geo.maxmind_db.is_empty() {
            chain.push(Box::new(MaxMindGeoLocator::open(std::path::Path::new(&s.geo.maxmind_db)).map_err(|e| anyhow::anyhow!("{e}"))?));
        }
        Arc::new(ChainGeoLocator(chain))
    };
    let dish_images: Arc<dyn DishImageFinder> = match s.media.stock.provider.as_str() {
        "" | "none" => Arc::new(NoDishImages),
        "unsplash" if s.media.stock.access_key.trim().is_empty() => {
            tracing::warn!("media.stock.provider=unsplash but no access key (FEAST__MEDIA__STOCK__ACCESS_KEY): stock dish photos are off");
            Arc::new(NoDishImages)
        }
        "unsplash" => {
            let app_name = if s.media.stock.app_name.is_empty() { "feast".to_owned() } else { s.media.stock.app_name.clone() };
            let cfg = UnsplashConfig { access_key: s.media.stock.access_key.clone(), app_name, api_url: "https://api.unsplash.com".into(), width: 640 };
            Arc::new(UnsplashFinder::new(cfg).map_err(|e| anyhow::anyhow!("{e}"))?)
        }
        other => bail!("media.stock.provider '{other}' is not supported (use \"unsplash\" or leave empty)"),
    };
    let push: Arc<dyn PushSender> = match s.push.provider.as_str() {
        "fcm" => {
            let account = ServiceAccount::load(&s.push.fcm.credentials_json, &s.push.fcm.credentials_file).map_err(|e| anyhow::anyhow!("{e}"))?;
            tracing::info!(project = %account.project_id, "push notifications via Firebase Cloud Messaging");
            Arc::new(FcmPushSender::new(account, None).map_err(|e| anyhow::anyhow!("{e}"))?)
        }
        "log" => Arc::new(LogPushSender),
        _ => Arc::new(NoPush),
    };
    let identity = oauth::build(&oauth_configs(s)).map_err(|e| anyhow::anyhow!("{e}"))?;
    let demo: Option<Arc<dyn DemoDataPort>> =
        s.app.demo_mode.then(|| Arc::new(DemoReset { seeder: seeder(inf, &repos, passwords.clone(), s.app.seed_history) }) as Arc<dyn DemoDataPort>);

    let ports = Ports {
        users: repos.users.clone(),
        restaurants: repos.restaurants.clone(),
        bookings: repos.bookings.clone(),
        billing: repos.billing.clone(),
        outbox: repos.outbox.clone(),
        markets: repos.markets.clone(),
        subscriptions: repos.subscriptions.clone(),
        compliance: repos.compliance.clone(),
        cases: repos.cases.clone(),
        reviews: repos.reviews.clone(),
        audit: repos.audit.clone(),
        trail: repos.trail.clone(),
        offline: repos.offline.clone(),
        files: inf.files.clone(),
        dish_images,
        geo,
        push_tokens: repos.push_tokens.clone(),
        refunds: repos.refunds.clone(),
        shared: repos.shared.clone(),
        push,
        demo,
        notifications: notifications.gateway(),
        payments: Arc::new(payments),
        otp,
        identity,
        tokens: Arc::new(JwtTokenService::new(&s.auth.jwt_secret, &s.auth.jwt_issuer, s.auth.token_ttl_secs)),
        passwords,
        clock: inf.clock.clone(),
        ids: inf.ids.clone(),
    };
    let settings = AppSettings {
        demo_mode: s.app.demo_mode,
        country: inf.country.market.country_code.clone(),
        directory: s.directory.clone(),
        subscription_rules: feast_domain::subscription::Rules { trial_days: s.subscriptions.trial_days, grace_days: s.subscriptions.grace_days },
        max_upload_bytes: s.http.max_upload_bytes,
        initial_partner_password: s.app.initial_partner_password.clone(),
        review_numbers: s.auth.otp.review_numbers.iter().map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()).collect(),
        review_code: s.auth.otp.review_code.trim().to_owned(),
    };
    Ok(Core { feast: Arc::new(Feast::new(ports, settings)), repos, notifications })
}

pub fn seeder(inf: &Infra, repos: &Repositories, passwords: Arc<dyn PasswordHasher>, with_history: bool) -> Seeder {
    Seeder {
        repos: repos.clone(),
        passwords,
        clock: inf.clock.clone(),
        files: inf.files.clone(),
        country: inf.country.clone(),
        seed: inf.seed.clone(),
        with_history,
    }
}

/// Seed demo data when the database is empty (first boot) and config allows it.
pub async fn seed_if_empty(inf: &Infra, core: &Core) -> Result<()> {
    if inf.settings.app.seed_on_empty && core.repos.admin.is_empty().await.map_err(|e| anyhow::anyhow!("{e}"))? {
        tracing::info!("empty database: loading demo data");
        seeder(inf, &core.repos, Arc::new(Argon2Hasher::default()), inf.settings.app.seed_history)
            .run()
            .await
            .map_err(|e| anyhow::anyhow!("seeding failed: {e}"))?;
    }
    Ok(())
}

fn oauth_configs(s: &Settings) -> Vec<OAuthProviderConfig> {
    [(AuthProvider::Google, &s.auth.oauth.google), (AuthProvider::Apple, &s.auth.oauth.apple), (AuthProvider::Facebook, &s.auth.oauth.facebook)]
        .into_iter()
        .map(|(provider, p)| OAuthProviderConfig {
            provider,
            mode: match p.mode.as_str() {
                "live" => OAuthMode::Live,
                "demo" => OAuthMode::Demo,
                _ => OAuthMode::Off,
            },
            client_ids: p.client_ids.iter().filter(|c| !c.is_empty()).cloned().collect(),
            client_secret: (!p.client_secret.is_empty()).then(|| p.client_secret.clone()),
            redirect_uri: (!p.redirect_uri.is_empty()).then(|| p.redirect_uri.clone()),
            trust_email: p.trust_email,
            endpoint_override: None,
        })
        .collect()
}

/// `sqlite://data/feast.db` → make sure `data/` exists.
fn ensure_sqlite_dir(url: &str) -> Result<()> {
    if let Some(path) = url.strip_prefix("sqlite://").or_else(|| url.strip_prefix("sqlite:")) {
        let path = path.split('?').next().unwrap_or_default();
        if path.is_empty() || path.contains(":memory:") {
            return Ok(());
        }
        if let Some(dir) = std::path::Path::new(path).parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
    }
    Ok(())
}

// ------------------------------------------------------------ readiness checks

pub struct DbCheck(pub Arc<dyn DatabaseAdmin>);
#[async_trait]
impl ReadinessCheck for DbCheck {
    fn name(&self) -> &'static str {
        "database"
    }
    async fn check(&self) -> Result<(), String> {
        self.0.ping().await.map_err(|e| e.message)
    }
}

pub struct BusCheck(pub Arc<dyn MessageBus>);
#[async_trait]
impl ReadinessCheck for BusCheck {
    fn name(&self) -> &'static str {
        "bus"
    }
    async fn check(&self) -> Result<(), String> {
        self.0.health().await.map_err(|e| e.message)
    }
}

pub struct StorageCheck(pub Arc<dyn FileStorage>);
#[async_trait]
impl ReadinessCheck for StorageCheck {
    fn name(&self) -> &'static str {
        "storage"
    }
    async fn check(&self) -> Result<(), String> {
        self.0.health().await.map_err(|e| e.message)
    }
}

pub struct NotificationCheck(pub Arc<dyn NotificationGateway>);
#[async_trait]
impl ReadinessCheck for NotificationCheck {
    fn name(&self) -> &'static str {
        "notifications"
    }
    async fn check(&self) -> Result<(), String> {
        self.0.unread_count("_healthcheck").await.map(|_| ()).map_err(|e| e.message)
    }
}
