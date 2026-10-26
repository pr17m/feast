//! Notification persistence. One trait; the SQL adapter runs on PostgreSQL or
//! SQLite (by URL). Another store (DynamoDB, Cosmos DB…) = implement the trait.

use async_trait::async_trait;
use feast_application::{AppError, AppResult};
use feast_domain::model::{Notification, NotificationKind};
use sqlx::any::AnyRow;
use sqlx::{AnyPool, Row};

pub static SQLITE_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../../migrations/sqlite/notification");
pub static POSTGRES_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../../migrations/postgres/notification");

#[async_trait]
pub trait NotificationStore: Send + Sync {
    /// Insert unless (event_id, user_id) exists. Returns true when inserted.
    async fn insert_if_absent(&self, n: &Notification, event_type: &str) -> AppResult<bool>;
    async fn list(&self, user_id: &str, unread_only: bool, limit: u32) -> AppResult<Vec<Notification>>;
    /// Mark read; empty `ids` = all. Returns rows changed.
    async fn mark_read(&self, user_id: &str, ids: &[String], at: &str) -> AppResult<i64>;
    async fn unread_count(&self, user_id: &str) -> AppResult<i64>;
    async fn ping(&self) -> AppResult<()>;
}

pub struct SqlNotificationStore {
    pool: AnyPool,
}

fn db_err(e: sqlx::Error) -> AppError {
    AppError::internal(format!("notification db: {e}"))
}

impl SqlNotificationStore {
    pub async fn connect(url: &str, max_connections: u32) -> AppResult<Self> {
        let (pool, dialect) = feast_infra::sql::open_pool(url, max_connections).await?;
        feast_infra::sql::run_migrations(&pool, dialect, &SQLITE_MIGRATOR, &POSTGRES_MIGRATOR).await?;
        Ok(Self { pool })
    }
}

fn row_to_notification(r: &AnyRow) -> Notification {
    Notification {
        id: r.get("id"),
        user_id: r.get("user_id"),
        restaurant_id: r.get("restaurant_id"),
        kind: NotificationKind::parse(r.get::<String, _>("kind").as_str()).unwrap_or(NotificationKind::BookingUpdated),
        title: r.get("title"),
        body: r.get("body"),
        link: r.get("link"),
        event_id: r.get("event_id"),
        created_at: r.get("created_at"),
        read_at: r.get("read_at"),
    }
}

#[async_trait]
impl NotificationStore for SqlNotificationStore {
    async fn insert_if_absent(&self, n: &Notification, event_type: &str) -> AppResult<bool> {
        let res = sqlx::query(
            "INSERT INTO notifications (id, user_id, restaurant_id, kind, title, body, link, event_id, event_type, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) ON CONFLICT (event_id, user_id) DO NOTHING",
        )
        .bind(&n.id)
        .bind(&n.user_id)
        .bind(&n.restaurant_id)
        .bind(n.kind.as_str())
        .bind(&n.title)
        .bind(&n.body)
        .bind(&n.link)
        .bind(&n.event_id)
        .bind(event_type)
        .bind(&n.created_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(res.rows_affected() == 1)
    }

    async fn list(&self, user_id: &str, unread_only: bool, limit: u32) -> AppResult<Vec<Notification>> {
        let sql = format!(
            "SELECT * FROM notifications WHERE user_id = $1 {} ORDER BY created_at DESC, id DESC LIMIT $2",
            if unread_only { "AND read_at IS NULL" } else { "" }
        );
        let rows = sqlx::query(&sql).bind(user_id).bind(i64::from(limit.clamp(1, 200))).fetch_all(&self.pool).await.map_err(db_err)?;
        Ok(rows.iter().map(row_to_notification).collect())
    }

    async fn mark_read(&self, user_id: &str, ids: &[String], at: &str) -> AppResult<i64> {
        let res = if ids.is_empty() {
            sqlx::query("UPDATE notifications SET read_at = $1 WHERE user_id = $2 AND read_at IS NULL").bind(at).bind(user_id).execute(&self.pool).await
        } else {
            let placeholders = (3..=ids.len() + 2).map(|i| format!("${i}")).collect::<Vec<_>>().join(", ");
            let sql = format!("UPDATE notifications SET read_at = $1 WHERE user_id = $2 AND read_at IS NULL AND id IN ({placeholders})");
            let mut q = sqlx::query(&sql).bind(at).bind(user_id);
            for id in ids {
                q = q.bind(id);
            }
            q.execute(&self.pool).await
        }
        .map_err(db_err)?;
        Ok(res.rows_affected() as i64)
    }

    async fn unread_count(&self, user_id: &str) -> AppResult<i64> {
        let r = sqlx::query("SELECT COUNT(*) AS n FROM notifications WHERE user_id = $1 AND read_at IS NULL")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(r.get("n"))
    }

    async fn ping(&self) -> AppResult<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await.map_err(db_err).map(|_| ())
    }
}
