//! State every API instance and worker shares through the database (so it
//! holds behind a load balancer, on SQLite in development and Postgres in
//! production): rate-limit counters, one-time-code challenges and leases.
//! Each operation is a single atomic statement, safe under concurrency.

use async_trait::async_trait;
use feast_application::ports::SharedStateRepository;
use feast_application::AppResult;
use sqlx::Row;

use super::{db_err, SqlStore};

#[async_trait]
impl SharedStateRepository for SqlStore {
    async fn rate_hit(&self, key: &str, window: i64, expires_ms: i64) -> AppResult<i64> {
        // One upsert: the first hit inserts 1, later ones add 1; RETURNING gives
        // the count this hit produced (SQLite ≥ 3.35 and Postgres).
        let row = sqlx::query(
            "INSERT INTO rate_limit_hits (key, win, hits, expires_ms) VALUES ($1, $2, 1, $3) \
             ON CONFLICT (key, win) DO UPDATE SET hits = rate_limit_hits.hits + 1 RETURNING hits",
        )
        .bind(key)
        .bind(window)
        .bind(expires_ms)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?;
        Ok(row.get::<i64, _>("hits"))
    }

    async fn otp_put(&self, subject: &str, code_hash: &str, expires_ms: i64) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO otp_challenges (subject, code_hash, attempts, expires_ms) VALUES ($1, $2, 0, $3) \
             ON CONFLICT (subject) DO UPDATE SET code_hash = excluded.code_hash, attempts = 0, expires_ms = excluded.expires_ms",
        )
        .bind(subject)
        .bind(code_hash)
        .bind(expires_ms)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn otp_check(&self, subject: &str, code_hash: &str, now_ms: i64, max_attempts: i64) -> AppResult<bool> {
        // 1. Spend one attempt on a live challenge (atomic; a used-up or expired
        //    one can't be tried any more).
        let row = sqlx::query("UPDATE otp_challenges SET attempts = attempts + 1 WHERE subject = $1 AND expires_ms > $2 AND attempts < $3 RETURNING code_hash")
            .bind(subject)
            .bind(now_ms)
            .bind(max_attempts)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
        let Some(row) = row else { return Ok(false) };
        if row.get::<String, _>("code_hash") != code_hash {
            return Ok(false);
        }
        // 2. Consume it: only one of two racing correct guesses gets the row.
        let res = sqlx::query("DELETE FROM otp_challenges WHERE subject = $1 AND code_hash = $2")
            .bind(subject)
            .bind(code_hash)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(res.rows_affected() == 1)
    }

    async fn lease(&self, name: &str, holder: &str, now_ms: i64, until_ms: i64) -> AppResult<bool> {
        // Take it if free/expired/ours, in one statement: the conditional
        // upsert only changes the row when the WHERE holds.
        let res = sqlx::query(
            "INSERT INTO leases (name, holder, until_ms) VALUES ($1, $2, $3) \
             ON CONFLICT (name) DO UPDATE SET holder = excluded.holder, until_ms = excluded.until_ms \
             WHERE leases.holder = excluded.holder OR leases.until_ms < $4",
        )
        .bind(name)
        .bind(holder)
        .bind(until_ms)
        .bind(now_ms)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(res.rows_affected() == 1)
    }

    async fn purge_expired(&self, now_ms: i64) -> AppResult<u64> {
        let a = sqlx::query("DELETE FROM rate_limit_hits WHERE expires_ms < $1").bind(now_ms).execute(self.pool()).await.map_err(db_err)?.rows_affected();
        let b = sqlx::query("DELETE FROM otp_challenges WHERE expires_ms < $1").bind(now_ms).execute(self.pool()).await.map_err(db_err)?.rows_affected();
        Ok(a + b)
    }
}
