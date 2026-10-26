use async_trait::async_trait;
use feast_application::ports::RefundRepository;
use feast_application::AppResult;
use feast_domain::refunds::Refund;
use feast_domain::EventEnvelope;
use sqlx::{AnyConnection, Row};

use super::{db_err, from_json, insert_events, now_iso, to_json, SqlStore};

/// Insert a PENDING refund unless its idempotency key is taken (inside the
/// caller's transaction). Returns whether this call inserted it.
pub(super) async fn insert_refund_tx(conn: &mut AnyConnection, r: &Refund) -> AppResult<bool> {
    let res = sqlx::query(
        "INSERT INTO refunds (id, intent_id, provider, provider_refund_id, status, idempotency_key, amount, data, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(&r.id)
    .bind(&r.intent_id)
    .bind(&r.provider)
    .bind(&r.provider_refund_id)
    .bind(r.status.as_str())
    .bind(&r.idempotency_key)
    .bind(r.amount)
    .bind(to_json(r)?)
    .bind(&r.created_at)
    .bind(&r.updated_at)
    .execute(&mut *conn)
    .await
    .map_err(db_err)?;
    Ok(res.rows_affected() == 1)
}

impl SqlStore {
    async fn refunds_where(&self, clause: &str, args: &[&str]) -> AppResult<Vec<Refund>> {
        let sql = format!("SELECT data FROM refunds {clause}");
        let mut q = sqlx::query(&sql);
        for a in args {
            q = q.bind(*a);
        }
        q.fetch_all(self.pool()).await.map_err(db_err)?.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }
}

#[async_trait]
impl RefundRepository for SqlStore {
    async fn insert_refund(&self, r: &Refund, events: &[EventEnvelope]) -> AppResult<(Refund, bool)> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let inserted = insert_refund_tx(&mut tx, r).await?;
        if inserted {
            insert_events(&mut tx, events).await?;
            tx.commit().await.map_err(db_err)?;
            return Ok((r.clone(), true));
        }
        tx.rollback().await.map_err(db_err)?;
        let existing = self
            .refunds_where("WHERE idempotency_key = $1", &[&r.idempotency_key])
            .await?
            .pop()
            .ok_or_else(|| feast_application::AppError::internal(format!("refund with key {} vanished", r.idempotency_key)))?;
        Ok((existing, false))
    }

    async fn update_refund(&self, r: &Refund, events: &[EventEnvelope]) -> AppResult<bool> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let res = sqlx::query("UPDATE refunds SET status = $1, provider_refund_id = $2, data = $3, updated_at = $4 WHERE id = $5 AND status = 'PENDING'")
            .bind(r.status.as_str())
            .bind(&r.provider_refund_id)
            .bind(to_json(r)?)
            .bind(now_iso())
            .bind(&r.id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        let changed = res.rows_affected() == 1;
        if changed {
            insert_events(&mut tx, events).await?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(changed)
    }

    async fn get_refund(&self, id: &str) -> AppResult<Option<Refund>> {
        Ok(self.refunds_where("WHERE id = $1", &[id]).await?.pop())
    }

    async fn find_refund_by_provider_id(&self, provider: &str, provider_refund_id: &str) -> AppResult<Option<Refund>> {
        Ok(self.refunds_where("WHERE provider = $1 AND provider_refund_id = $2", &[provider, provider_refund_id]).await?.pop())
    }

    async fn refunds_for_intent(&self, intent_id: &str) -> AppResult<Vec<Refund>> {
        self.refunds_where("WHERE intent_id = $1 ORDER BY created_at", &[intent_id]).await
    }

    async fn unsent_refunds(&self, limit: u32) -> AppResult<Vec<Refund>> {
        self.refunds_where(&format!("WHERE status = 'PENDING' AND provider_refund_id IS NULL ORDER BY created_at LIMIT {}", limit.max(1)), &[]).await
    }

    async fn list_refunds(&self, limit: u32) -> AppResult<Vec<Refund>> {
        self.refunds_where(&format!("ORDER BY created_at DESC LIMIT {}", limit.max(1)), &[]).await
    }
}
