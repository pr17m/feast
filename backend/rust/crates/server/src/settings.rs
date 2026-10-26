//! Configuration.
//!
//! Every deployment serves **one country**, chosen at deploy time with
//! `--country <cc>` or `FEAST_COUNTRY=<cc>`. Layers (later wins):
//!
//! 1. `config/default.toml`
//! 2. `config/<env>.{toml,yaml}` (env = `--env` / `FEAST_ENV`, default development)
//! 3. `config/countries/<cc>.toml` (optional per-country deployment settings)
//! 4. `config/local.{toml,yaml}` (git-ignored, for secrets)
//! 5. environment variables `FEAST__SECTION__KEY` (e.g. `FEAST__DATABASE__URL`)
//!
//! The country's market data (currency, tax, payment provider, documents,
//! cities) is in `config/countries/<cc>.yaml` and seeds the database.
//! Every field is documented in `config/default.toml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub app: AppSection,
    pub http: HttpSection,
    pub database: DatabaseSection,
    pub bus: BusSection,
    pub outbox: OutboxSection,
    pub auth: AuthSection,
    pub payments: PaymentsSection,
    pub storage: StorageSection,
    /// Dish photos: stock-photo search for dishes without an owner photo.
    #[serde(default)]
    pub media: MediaSection,
    pub geo: GeoSection,
    pub subscriptions: SubscriptionsSection,
    pub notifications: NotificationsSection,
    pub log: LogSection,
    /// Country → base URL of the deployment serving it (`config/directory.toml`).
    #[serde(default)]
    pub directory: BTreeMap<String, String>,
    /// Where the config was read from (for the country file).
    #[serde(skip)]
    pub config_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppSection {
    /// development | staging | production (production enables strict checks).
    pub env: String,
    /// ISO country this deployment serves (set from --country / FEAST_COUNTRY).
    #[serde(default)]
    pub country: String,
    pub demo_mode: bool,
    pub initial_partner_password: String,
    /// Seed demo data on startup when the database has no restaurants.
    pub seed_on_empty: bool,
    pub seed_history: bool,
    /// Seed/reference data JSON (cities, demo restaurants). Empty = bundled.
    pub seed_file: String,
    /// Password for the seeded partner/admin accounts. Empty = the seed file's
    /// (public!) default — always set it on anything reachable from the internet.
    #[serde(default)]
    pub seed_password: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpSection {
    pub host: String,
    pub port: u16,
    pub cors_origins: Vec<String>,
    pub request_timeout_secs: u64,
    pub max_body_bytes: usize,
    pub max_upload_bytes: usize,
    /// Proxies in front of the API that append to X-Forwarded-For (0 = direct).
    /// Web app proxy only: 1. Cloud load balancer → web app → API: 1 (the web app
    /// passes the LB's header through). GCP external HTTPS LB: 2.
    pub proxy_hops: usize,
    /// Sign-in/OTP/signup requests allowed per client IP per minute (0 = off).
    pub auth_rate_limit_per_minute: u32,
    /// Bearer token for /metrics (empty = open; keep /metrics off the public internet).
    #[serde(default)]
    pub metrics_token: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseSection {
    /// `sqlite://data/feast.db` today; `postgres://…` once that adapter exists.
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BusSection {
    /// memory | redis
    pub kind: String,
    pub memory_capacity: usize,
    pub consumer_group: String,
    pub redis: RedisSection,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedisSection {
    pub url: String,
    pub stream: String,
    pub max_len: usize,
    pub block_ms: usize,
    pub claim_idle_ms: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutboxSection {
    pub poll_interval_ms: u64,
    pub batch_size: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthSection {
    pub jwt_secret: String,
    pub jwt_issuer: String,
    pub token_ttl_secs: i64,
    pub otp: OtpSection,
    pub oauth: OAuthSection,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OtpSection {
    /// fixed (dev: one code for everyone) | log (random codes in the log) | twilio (real SMS via Twilio Verify)
    pub provider: String,
    #[serde(default)]
    pub twilio: TwilioSection,
    pub fixed_code: String,
    pub show_hint: bool,
    pub ttl_secs: u64,
    pub max_attempts: u8,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TwilioSection {
    #[serde(default)]
    pub account_sid: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default)]
    pub service_sid: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OAuthSection {
    pub google: OAuthProviderSection,
    pub apple: OAuthProviderSection,
    pub facebook: OAuthProviderSection,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OAuthProviderSection {
    /// off | demo | live
    pub mode: String,
    #[serde(default)]
    pub client_ids: Vec<String>,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub redirect_uri: String,
    #[serde(default)]
    pub trust_email: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PaymentsSection {
    /// Route every market to this provider (e.g. "mock" in development).
    /// Empty in production: each market uses the provider its country file names.
    #[serde(default)]
    pub force_provider: String,
    /// `[payments.providers.<id>]`: credentials/options per provider (see
    /// crates/infra/src/payments). Only listed providers are enabled.
    #[serde(default)]
    pub providers: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageSection {
    /// local | s3 (AWS S3, GCS interop, R2, MinIO) | azure (Azure Blob Storage)
    pub kind: String,
    /// Directory for `local` (empty = data/<country>/files).
    #[serde(default)]
    pub local_dir: String,
    #[serde(default)]
    pub s3: Option<serde_json::Value>,
    #[serde(default)]
    pub azure: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MediaSection {
    #[serde(default)]
    pub stock: StockSection,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct StockSection {
    /// "" (off: dishes without a photo show generated art) | unsplash
    #[serde(default)]
    pub provider: String,
    /// Unsplash access key: FEAST__MEDIA__STOCK__ACCESS_KEY
    #[serde(default)]
    pub access_key: String,
    /// Unsplash app name for attribution links.
    #[serde(default)]
    pub app_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeoSection {
    /// Request headers carrying the visitor's country, checked in order.
    pub headers: Vec<String>,
    /// Optional MaxMind GeoLite2/GeoIP2 Country database (.mmdb).
    #[serde(default)]
    pub maxmind_db: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubscriptionsSection {
    pub trial_days: i64,
    pub grace_days: i64,
    /// How often the worker checks for expiry reminders.
    pub reminder_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationsSection {
    /// embedded (in-process) | grpc (separate `feast notification` service)
    pub mode: String,
    pub database_url: String,
    pub max_connections: u32,
    /// Where `feast notification` listens.
    pub grpc_listen: String,
    /// Where gateways/workers reach it in grpc mode.
    pub grpc_url: String,
    /// Shared secret for service-to-service calls (empty = no auth; dev only).
    pub service_token: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LogSection {
    /// tracing filter, e.g. "info,feast=debug". RUST_LOG overrides.
    pub level: String,
    /// pretty | json
    pub format: String,
}

const SECRET_KEYS: &[&str] = &[
    "jwt_secret",
    "client_secret",
    "service_token",
    "initial_partner_password",
    "fixed_code",
    "key_secret",
    "auth_token",
    "secret_key",
    "secret_access_key",
    "access_key",
    "sas_token",
    "webhook_secret",
    "metrics_token",
    "seed_password",
];

/// Find the config directory: the given one, else `../config` (running from `rust/`).
pub fn resolve_config_dir(dir: &str) -> PathBuf {
    let p = PathBuf::from(dir);
    if p.join("default.toml").exists() || p.join("default.yaml").exists() {
        return p;
    }
    let up = Path::new("..").join(dir);
    if dir == "config" && up.join("default.toml").exists() {
        return up;
    }
    p
}

impl Settings {
    pub fn load(dir: &str, env: Option<&str>, country: Option<&str>) -> Result<Self> {
        let env = env.map(str::to_owned).or_else(|| std::env::var("FEAST_ENV").ok()).unwrap_or_else(|| "development".into());
        let base = resolve_config_dir(dir);
        if !base.join("default.toml").exists() && !base.join("default.yaml").exists() {
            bail!("config directory '{dir}' has no default.toml/default.yaml (use --config <dir>)");
        }
        let Some(country) = country.map(|c| c.trim().to_lowercase()).filter(|c| !c.is_empty()) else {
            let available = available_countries(&base).join(", ");
            bail!("no country given. Each deployment serves one country: pass --country <code> or set FEAST_COUNTRY (available: {available})");
        };
        if country.len() != 2 || !country.chars().all(|c| c.is_ascii_lowercase()) || !base.join("countries").join(format!("{country}.yaml")).exists() {
            let available = available_countries(&base).join(", ");
            bail!("unknown country '{country}': no config/countries/{country}.yaml (available: {available})");
        }
        let cfg = Config::builder()
            .add_source(File::with_name(&base.join("default").to_string_lossy()).required(true))
            .add_source(File::with_name(&base.join(&env).to_string_lossy()).required(false))
            .add_source(File::from(base.join("countries").join(format!("{country}.toml"))).required(false))
            .add_source(File::with_name(&base.join("local").to_string_lossy()).required(false))
            .add_source(
                Environment::with_prefix("FEAST")
                    .prefix_separator("__")
                    .separator("__")
                    .try_parsing(true)
                    .list_separator(",")
                    .with_list_parse_key("http.cors_origins")
                    .with_list_parse_key("auth.oauth.google.client_ids")
                    .with_list_parse_key("auth.oauth.apple.client_ids")
                    .with_list_parse_key("auth.oauth.facebook.client_ids"),
            )
            .set_override("app.env", env.clone())?
            .set_override("app.country", country.to_uppercase())?
            .build()
            .with_context(|| format!("reading config from '{}'", base.display()))?;
        let mut s: Settings = cfg.try_deserialize().context("invalid configuration")?;
        s.config_dir = base.clone();
        s.directory = load_directory(&base)?;
        // Per-country defaults for local data (each deployment has its own).
        let cc = &country;
        if s.database.url.is_empty() {
            s.database.url = format!("sqlite://data/{cc}/feast.db");
        }
        if s.notifications.database_url.is_empty() {
            s.notifications.database_url = format!("sqlite://data/{cc}/notifications.db");
        }
        if s.is_production() {
            if s.app.demo_mode {
                eprintln!("warning: app.demo_mode is not allowed in production; turning it off");
                s.app.demo_mode = false;
            }
            // Test-mode social login never runs in production: switch it off.
            for (name, p) in [("google", &mut s.auth.oauth.google), ("apple", &mut s.auth.oauth.apple), ("facebook", &mut s.auth.oauth.facebook)] {
                if p.mode == "demo" {
                    eprintln!("warning: auth.oauth.{name}.mode=demo is not allowed in production; turning {name} sign-in off (set mode=live with client_ids to enable it)");
                    p.mode = "off".into();
                }
            }
        }
        if s.storage.local_dir.is_empty() {
            s.storage.local_dir = format!("data/{cc}/files");
        }
        s.validate()?;
        Ok(s)
    }

    pub fn country(&self) -> &str {
        &self.app.country
    }

    pub fn countries_dir(&self) -> PathBuf {
        self.config_dir.join("countries")
    }

    pub fn is_production(&self) -> bool {
        self.app.env == "production"
    }

    /// Fail fast on unsafe or inconsistent settings.
    fn validate(&self) -> Result<()> {
        let mut errors: Vec<String> = vec![];
        if !["memory", "redis"].contains(&self.bus.kind.as_str()) {
            errors.push(format!("bus.kind must be memory|redis, got '{}'", self.bus.kind));
        }
        if !["embedded", "grpc"].contains(&self.notifications.mode.as_str()) {
            errors.push(format!("notifications.mode must be embedded|grpc, got '{}'", self.notifications.mode));
        }
        if !["local", "s3", "azure"].contains(&self.storage.kind.as_str()) {
            errors.push(format!("storage.kind must be local|s3|azure, got '{}'", self.storage.kind));
        }
        if self.storage.kind == "s3" && self.storage.s3.is_none() {
            errors.push("storage.kind=s3 needs a [storage.s3] section".into());
        }
        if self.storage.kind == "azure" && self.storage.azure.is_none() {
            errors.push("storage.kind=azure needs a [storage.azure] section".into());
        }
        if self.subscriptions.trial_days < 0 || self.subscriptions.grace_days < 0 {
            errors.push("subscriptions.trial_days/grace_days can't be negative".into());
        }
        if !self.payments.force_provider.is_empty() && !self.payments.providers.contains_key(&self.payments.force_provider) {
            errors.push(format!("payments.force_provider '{}' needs a [payments.providers.{0}] section", self.payments.force_provider));
        }
        if !["fixed", "log", "twilio"].contains(&self.auth.otp.provider.as_str()) {
            errors.push(format!("auth.otp.provider must be fixed|log|twilio, got '{}'", self.auth.otp.provider));
        }
        for (name, p) in self.oauth_providers() {
            if !["off", "demo", "live"].contains(&p.mode.as_str()) {
                errors.push(format!("auth.oauth.{name}.mode must be off|demo|live"));
            }
        }
        if self.auth.jwt_secret.len() < 32 {
            errors.push("auth.jwt_secret must be at least 32 characters".into());
        }
        if self.is_production() {
            if self.auth.jwt_secret.contains("change-me") {
                errors.push("set a real auth.jwt_secret (FEAST__AUTH__JWT_SECRET)".into());
            }
            if self.auth.otp.provider != "twilio" {
                errors.push("auth.otp.provider must be twilio in production (fixed/log don't send real SMS)".into());
            }
            if !self.payments.force_provider.is_empty() {
                errors.push("payments.force_provider must be empty in production (each market uses its own provider)".into());
            }
            if self.provider_enabled("mock") {
                errors.push("payments.providers.mock (test payments) must not be enabled in production".into());
            }
            if self.storage.kind == "local" {
                tracing_warn("storage.kind=local in production: documents live on this machine's disk; prefer s3");
            }
            if self.http.auth_rate_limit_per_minute == 0 {
                errors.push("http.auth_rate_limit_per_minute must be > 0 in production".into());
            }
            if self.http.cors_origins.iter().any(|o| o == "*") {
                errors.push("http.cors_origins must list your app origins in production (not *)".into());
            }
            if self.notifications.mode == "grpc" && self.notifications.service_token.is_empty() {
                errors.push("notifications.service_token is required in production".into());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            bail!("configuration problems:\n  - {}", errors.join("\n  - "))
        }
    }

    /// A `[payments.providers.<id>]` section exists and isn't `enabled = false`.
    pub fn provider_enabled(&self, id: &str) -> bool {
        match self.payments.providers.get(id).and_then(|v| v.get("enabled")) {
            None => self.payments.providers.contains_key(id),
            Some(serde_json::Value::Bool(b)) => *b,
            Some(serde_json::Value::String(s)) => s.eq_ignore_ascii_case("true"),
            Some(_) => true,
        }
    }

    pub fn oauth_providers(&self) -> [(&'static str, &OAuthProviderSection); 3] {
        [("google", &self.auth.oauth.google), ("apple", &self.auth.oauth.apple), ("facebook", &self.auth.oauth.facebook)]
    }

    /// Effective config as TOML with secrets masked (for `feast config`).
    pub fn redacted_toml(&self) -> String {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        fn mask(v: &mut serde_json::Value) {
            match v {
                serde_json::Value::Object(m) => {
                    for (k, x) in m.iter_mut() {
                        if SECRET_KEYS.contains(&k.as_str()) && x.as_str().is_some_and(|s| !s.is_empty()) {
                            *x = serde_json::Value::String("••••••".into());
                        } else {
                            mask(x);
                        }
                    }
                }
                serde_json::Value::Array(a) => a.iter_mut().for_each(mask),
                _ => {}
            }
        }
        mask(&mut v);
        // Mask credentials inside URLs (redis://:pass@host, postgres://u:p@host).
        let s = serde_json::to_string_pretty(&v).unwrap_or_default();
        mask_url_passwords(&s)
    }
}

fn tracing_warn(msg: &str) {
    eprintln!("warning: {msg}");
}

fn available_countries(base: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(base.join("countries"))
        .map(|d| d.filter_map(|e| e.ok()).filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".yaml")).map(str::to_owned)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// `config/directory.toml`: `[countries] IN = "https://…"` (optional).
fn load_directory(base: &Path) -> Result<BTreeMap<String, String>> {
    let path = base.join("directory.toml");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    #[derive(Deserialize)]
    struct Dir {
        #[serde(default)]
        countries: BTreeMap<String, String>,
    }
    let d: Dir = Config::builder().add_source(File::from(path.clone())).build()?.try_deserialize().with_context(|| path.display().to_string())?;
    Ok(d.countries.into_iter().map(|(k, v)| (k.to_uppercase(), v)).filter(|(_, v)| !v.is_empty()).collect())
}

fn mask_url_passwords(s: &str) -> String {
    s.lines()
        .map(|line| match (line.find("://"), line.find('@')) {
            (Some(a), Some(b)) if b > a => {
                let creds = &line[a + 3..b];
                match creds.find(':') {
                    Some(c) => format!("{}{}:••••••{}", &line[..a + 3], &creds[..c], &line[b..]),
                    None => line.to_owned(),
                }
            }
            _ => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
