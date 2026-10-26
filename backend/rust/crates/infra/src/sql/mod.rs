//! SQL adapter for every core repository port (one store, many traits), on
//! **PostgreSQL** (production: RDS, Azure Database for PostgreSQL, Cloud SQL…)
//! or **SQLite** (development, single-machine installs) — chosen by
//! `database.url`. Queries are portable SQL with `$n` placeholders (which
//! SQLite also accepts); each dialect has its own migrations in
//! `migrations/<dialect>/core`.

mod audit;
mod billing;
mod bookings;
mod compliance;
mod markets;
mod offline;
mod outbox;
mod push;
mod refunds;
mod restaurants;
mod shared;
mod subscriptions;
mod trust;
mod users;

use std::time::Duration;

use feast_application::{AppError, AppResult};
use feast_domain::EventEnvelope;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sqlx::any::AnyPoolOptions;
use sqlx::{AnyConnection, AnyPool, Executor};

/// Embedded migrations per dialect (compiled into the binary).
pub static SQLITE_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../../migrations/sqlite/core");
pub static POSTGRES_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../../migrations/postgres/core");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Sqlite,
    Postgres,
}

impl Dialect {
    pub fn of(url: &str) -> AppResult<Self> {
        match url.split(':').next().unwrap_or_default() {
            "sqlite" => Ok(Self::Sqlite),
            "postgres" | "postgresql" => Ok(Self::Postgres),
            other => Err(AppError::validation("UNSUPPORTED_DATABASE", format!("Unknown database URL scheme '{other}' (use sqlite:// or postgres://)"))),
        }
    }
}

#[derive(Clone)]
pub struct SqlStore {
    pool: AnyPool,
    dialect: Dialect,
}

impl SqlStore {
    /// Connect and run pending migrations.
    pub async fn connect(url: &str, max_connections: u32) -> AppResult<Self> {
        let (pool, dialect) = open_pool(url, max_connections).await?;
        run_migrations(&pool, dialect, &SQLITE_MIGRATOR, &POSTGRES_MIGRATOR).await?;
        Ok(Self { pool, dialect })
    }

    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Delete all rows (demo reset). Keeps the schema.
    pub async fn wipe(&self) -> AppResult<()> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        for t in [
            "refunds",
            "rate_limit_hits",
            "otp_challenges",
            "leases",
            "push_deliveries",
            "outbox",
            "push_tokens",
            "audit_log",
            "audit_trail",
            "offline_payments",
            "reviews",
            "cases",
            "slot_locks",
            "deal_claims",
            "deal_budget_days",
            "restaurant_reviews",
            "compliance_documents",
            "platform_payments",
            "subscriptions",
            "cities",
            "markets",
            "bill_payments",
            "payment_intents",
            "bills",
            "bookings",
            "user_restaurants",
            "user_identities",
            "restaurants",
            "users",
        ] {
            sqlx::query(&format!("DELETE FROM {t}")).execute(&mut *tx).await.map_err(db_err)?;
        }
        sqlx::query("UPDATE invoice_seq SET value = 0").execute(&mut *tx).await.map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }
}

/// Run the dialect's migrations. `ignore_missing` lets the core and
/// notification services share one database (each skips the other's versions).
pub async fn run_migrations(pool: &AnyPool, dialect: Dialect, sqlite: &sqlx::migrate::Migrator, postgres: &sqlx::migrate::Migrator) -> AppResult<()> {
    let base = match dialect {
        Dialect::Sqlite => sqlite,
        Dialect::Postgres => postgres,
    };
    let mut m = sqlx::migrate::Migrator { migrations: base.migrations.clone(), ..sqlx::migrate::Migrator::DEFAULT };
    m.set_ignore_missing(true);
    m.run(pool).await.map_err(|e| AppError::internal(format!("migrations failed: {e}")))
}

/// Open a pool for `sqlite://…` or `postgres://…`. SQLite gets WAL mode,
/// foreign keys and a busy timeout on every connection.
pub async fn open_pool(url: &str, max_connections: u32) -> AppResult<(AnyPool, Dialect)> {
    sqlx::any::install_default_drivers();
    let dialect = Dialect::of(url)?;
    let mut url = url.to_owned();
    let memory = url.contains(":memory:") || url.contains("mode=memory");
    if dialect == Dialect::Sqlite && !memory && !url.contains("mode=") {
        url.push_str(if url.contains('?') { "&mode=rwc" } else { "?mode=rwc" });
    }
    // An in-memory SQLite database exists per connection, so it must use exactly one.
    let max = if memory { 1 } else { max_connections.max(1) };
    let mut opts = AnyPoolOptions::new().max_connections(max).acquire_timeout(Duration::from_secs(10));
    if dialect == Dialect::Sqlite {
        opts = opts.after_connect(|conn, _| {
            Box::pin(async move {
                conn.execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;").await?;
                Ok(())
            })
        });
    }
    let pool = opts.connect(&url).await.map_err(|e| AppError::internal(format!("cannot open database: {e}")))?;
    Ok((pool, dialect))
}

pub(crate) fn db_err(e: sqlx::Error) -> AppError {
    AppError::internal(format!("database: {e}"))
}

pub(crate) fn to_json<T: Serialize>(v: &T) -> AppResult<String> {
    serde_json::to_string(v).map_err(AppError::internal)
}

pub(crate) fn from_json<T: DeserializeOwned>(s: &str) -> AppResult<T> {
    serde_json::from_str(s).map_err(|e| AppError::internal(format!("corrupt row json: {e}")))
}

/// Append events to the outbox — and their audit records to the audit trail —
/// inside the caller's transaction, so neither can disagree with the change.
pub(crate) async fn insert_events(conn: &mut AnyConnection, events: &[EventEnvelope]) -> AppResult<()> {
    for e in events {
        for r in feast_domain::audit::records_for(e) {
            audit::append_tx(conn, &r).await?;
        }
        sqlx::query("INSERT INTO outbox (event_id, event_type, payload, created_at) VALUES ($1, $2, $3, $4) ON CONFLICT (event_id) DO NOTHING")
            .bind(&e.id)
            .bind(e.type_name())
            .bind(e.to_json())
            .bind(&e.occurred_at)
            .execute(&mut *conn)
            .await
            .map_err(db_err)?;
    }
    Ok(())
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[async_trait::async_trait]
impl crate::database::DatabaseAdmin for SqlStore {
    async fn wipe(&self) -> AppResult<()> {
        SqlStore::wipe(self).await
    }

    async fn is_empty(&self) -> AppResult<bool> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM restaurants").fetch_one(&self.pool).await.map_err(db_err)?;
        Ok(sqlx::Row::get::<i64, _>(&row, "n") == 0)
    }

    async fn ping(&self) -> AppResult<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }
}
