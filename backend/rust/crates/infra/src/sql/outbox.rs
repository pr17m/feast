use async_trait::async_trait;
use feast_application::ports::OutboxRepository;
use feast_application::AppResult;
use feast_domain::EventEnvelope;
use sqlx::Row;

use super::{db_err, insert_events, now_iso, SqlStore};

#[async_trait]
impl OutboxRepository for SqlStore {
    async fn append(&self, events: &[EventEnvelope]) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)
    }

    async fn fetch_pending(&self, limit: u32) -> AppResult<Vec<(i64, EventEnvelope)>> {
        let rows = sqlx::query("SELECT seq, payload FROM outbox WHERE published_at IS NULL ORDER BY seq LIMIT $1")
            .bind(limit.max(1) as i64)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let seq: i64 = r.get("seq");
            match EventEnvelope::from_json(&r.get::<String, _>("payload")) {
                Ok(env) => out.push((seq, env)),
                Err(e) => {
                    // Unreadable payload (e.g. schema drift): park it so it can't block the relay.
                    tracing::error!(seq, error = %e, "unparseable outbox row; marking failed");
                    sqlx::query("UPDATE outbox SET published_at = $1, last_error = $2 WHERE seq = $3")
                        .bind(format!("FAILED {}", now_iso()))
                        .bind(e.to_string())
                        .bind(seq)
                        .execute(self.pool())
                        .await
                        .map_err(db_err)?;
                }
            }
        }
        Ok(out)
    }

    async fn mark_published(&self, seqs: &[i64]) -> AppResult<()> {
        if seqs.is_empty() {
            return Ok(());
        }
        let placeholders = (2..=seqs.len() + 1).map(|i| format!("${i}")).collect::<Vec<_>>().join(", ");
        let sql = format!("UPDATE outbox SET published_at = $1, attempts = attempts + 1 WHERE seq IN ({placeholders})");
        let mut q = sqlx::query(&sql).bind(now_iso());
        for s in seqs {
            q = q.bind(s);
        }
        q.execute(self.pool()).await.map_err(db_err)?;
        Ok(())
    }

    async fn record_failure(&self, seq: i64, error: &str) -> AppResult<()> {
        sqlx::query("UPDATE outbox SET attempts = attempts + 1, last_error = $1 WHERE seq = $2")
            .bind(error)
            .bind(seq)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }
}
