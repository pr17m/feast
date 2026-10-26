//! Push tokens and the per-user push switch.

use async_trait::async_trait;
use feast_application::ports::{DuePush, PushTokenRepository};
use feast_application::AppResult;
use feast_domain::model::Notification;
use feast_domain::push::{PushApp, PushPlatform, PushToken};
use sqlx::Row;

use super::{db_err, SqlStore};

#[async_trait]
impl PushTokenRepository for SqlStore {
    async fn upsert_token(&self, t: &PushToken) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO push_tokens (token, user_id, platform, app, created_at, last_seen_at) VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (token) DO UPDATE SET user_id = excluded.user_id, platform = excluded.platform, app = excluded.app, last_seen_at = excluded.last_seen_at",
        )
        .bind(&t.token)
        .bind(&t.user_id)
        .bind(t.platform.as_str())
        .bind(t.app.as_str())
        .bind(&t.created_at)
        .bind(&t.last_seen_at)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn delete_token(&self, user_id: &str, token: &str) -> AppResult<bool> {
        let r =
            sqlx::query("DELETE FROM push_tokens WHERE token = $1 AND user_id = $2").bind(token).bind(user_id).execute(self.pool()).await.map_err(db_err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn forget_token(&self, token: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM push_tokens WHERE token = $1").bind(token).execute(self.pool()).await.map_err(db_err)?;
        Ok(())
    }

    async fn tokens_for(&self, user_id: &str) -> AppResult<Vec<PushToken>> {
        let rows = sqlx::query("SELECT token, user_id, platform, app, created_at, last_seen_at FROM push_tokens WHERE user_id = $1 ORDER BY last_seen_at DESC")
            .bind(user_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                Some(PushToken {
                    token: r.get("token"),
                    user_id: r.get("user_id"),
                    platform: PushPlatform::parse(r.get::<String, _>("platform").as_str())?,
                    app: PushApp::parse(r.get::<String, _>("app").as_str())?,
                    created_at: r.get("created_at"),
                    last_seen_at: r.get("last_seen_at"),
                })
            })
            .collect())
    }

    async fn push_enabled(&self, user_id: &str) -> AppResult<bool> {
        let row = sqlx::query("SELECT push_enabled FROM users WHERE id = $1").bind(user_id).fetch_optional(self.pool()).await.map_err(db_err)?;
        Ok(row.map(|r| r.get::<i64, _>("push_enabled") != 0).unwrap_or(false))
    }

    async fn set_push_enabled(&self, user_id: &str, enabled: bool) -> AppResult<()> {
        sqlx::query("UPDATE users SET push_enabled = $1 WHERE id = $2").bind(i64::from(enabled)).bind(user_id).execute(self.pool()).await.map_err(db_err)?;
        Ok(())
    }

    async fn claim_push(&self, n: &Notification, token: &str, lease_until_ms: i64) -> AppResult<bool> {
        let res = sqlx::query(
            "INSERT INTO push_deliveries (notification_id, token, status, attempts, next_at_ms, lease_until_ms, data, claimed_at) \
             VALUES ($1, $2, 'SENDING', 1, 0, $3, $4, $5) ON CONFLICT (notification_id, token) DO NOTHING",
        )
        .bind(&n.id)
        .bind(token)
        .bind(lease_until_ms)
        .bind(super::to_json(n)?)
        .bind(super::now_iso())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(res.rows_affected() == 1)
    }

    async fn finish_push(&self, notification_id: &str, token: &str, status: &str, next_at_ms: i64, error: Option<&str>) -> AppResult<()> {
        sqlx::query("UPDATE push_deliveries SET status = $1, next_at_ms = $2, lease_until_ms = 0, error = $3 WHERE notification_id = $4 AND token = $5")
            .bind(status)
            .bind(next_at_ms)
            .bind(error)
            .bind(notification_id)
            .bind(token)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn claim_due_pushes(&self, now_ms: i64, lease_until_ms: i64, limit: u32) -> AppResult<Vec<DuePush>> {
        let candidates = sqlx::query(&format!(
            "SELECT notification_id, token FROM push_deliveries \
             WHERE (status = 'RETRY' AND next_at_ms <= $1) OR (status = 'SENDING' AND lease_until_ms < $1) ORDER BY next_at_ms LIMIT {}",
            limit.max(1)
        ))
        .bind(now_ms)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        let mut due = Vec::new();
        for c in candidates {
            let (nid, token): (String, String) = (c.get("notification_id"), c.get("token"));
            // Conditional claim: of several sweepers, one gets each row.
            let row = sqlx::query(
                "UPDATE push_deliveries SET status = 'SENDING', attempts = attempts + 1, lease_until_ms = $1 \
                 WHERE notification_id = $2 AND token = $3 AND ((status = 'RETRY' AND next_at_ms <= $4) OR (status = 'SENDING' AND lease_until_ms < $4)) \
                 RETURNING attempts, data",
            )
            .bind(lease_until_ms)
            .bind(&nid)
            .bind(&token)
            .bind(now_ms)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
            if let Some(r) = row {
                due.push(DuePush { notification: super::from_json(&r.get::<String, _>("data"))?, token, attempts: r.get::<i64, _>("attempts") });
            }
        }
        Ok(due)
    }
}
