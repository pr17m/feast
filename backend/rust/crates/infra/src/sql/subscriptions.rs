use std::collections::HashMap;

use async_trait::async_trait;
use feast_application::ports::SubscriptionRepository;
use feast_application::AppResult;
use feast_domain::model::{PlanId, PlatformPayment, PlatformPaymentStatus, SubscriptionRecord};
use feast_domain::EventEnvelope;
use sqlx::any::AnyRow;
use sqlx::{AnyConnection, Row};

use super::{db_err, from_json, insert_events, now_iso, to_json, SqlStore};

fn row_to_record(r: &AnyRow) -> SubscriptionRecord {
    SubscriptionRecord {
        trial_ends_at: r.get("trial_ends_at"),
        paid_through: r.get("paid_through"),
        plan_id: r.get::<Option<String>, _>("plan_id").as_deref().and_then(PlanId::parse),
    }
}

pub(super) async fn put(conn: &mut AnyConnection, restaurant_id: &str, rec: &SubscriptionRecord) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO subscriptions (restaurant_id, trial_ends_at, paid_through, plan_id, updated_at) VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (restaurant_id) DO UPDATE SET trial_ends_at = excluded.trial_ends_at, paid_through = excluded.paid_through, \
         plan_id = excluded.plan_id, updated_at = excluded.updated_at",
    )
    .bind(restaurant_id)
    .bind(&rec.trial_ends_at)
    .bind(&rec.paid_through)
    .bind(rec.plan_id.map(PlanId::as_str))
    .bind(now_iso())
    .execute(&mut *conn)
    .await
    .map_err(db_err)?;
    Ok(())
}

pub(super) async fn insert_payment_tx(conn: &mut AnyConnection, restaurant_id: &str, p: &PlatformPayment) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO platform_payments (id, invoice_number, restaurant_id, status, currency, total, created_at, data) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&p.id)
    .bind(&p.invoice_number)
    .bind(restaurant_id)
    .bind(payment_status(p.status))
    .bind(&p.currency)
    .bind(p.total)
    .bind(&p.created_at)
    .bind(to_json(p)?)
    .execute(&mut *conn)
    .await
    .map_err(db_err)?;
    Ok(())
}

fn payment_status(s: PlatformPaymentStatus) -> &'static str {
    match s {
        PlatformPaymentStatus::Paid => "PAID",
        PlatformPaymentStatus::Failed => "FAILED",
    }
}

#[async_trait]
impl SubscriptionRepository for SqlStore {
    async fn get_record(&self, restaurant_id: &str) -> AppResult<Option<SubscriptionRecord>> {
        let row = sqlx::query("SELECT trial_ends_at, paid_through, plan_id FROM subscriptions WHERE restaurant_id = $1")
            .bind(restaurant_id)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
        Ok(row.as_ref().map(row_to_record))
    }

    async fn all_records(&self) -> AppResult<HashMap<String, SubscriptionRecord>> {
        let rows = sqlx::query("SELECT restaurant_id, trial_ends_at, paid_through, plan_id FROM subscriptions").fetch_all(self.pool()).await.map_err(db_err)?;
        Ok(rows.iter().map(|r| (r.get("restaurant_id"), row_to_record(r))).collect())
    }

    async fn put_record(&self, restaurant_id: &str, rec: &SubscriptionRecord) -> AppResult<()> {
        let mut conn = self.pool().acquire().await.map_err(db_err)?;
        put(&mut conn, restaurant_id, rec).await
    }

    async fn record_payment(&self, restaurant_id: &str, rec: Option<&SubscriptionRecord>, p: &PlatformPayment, events: &[EventEnvelope]) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        insert_payment_tx(&mut tx, restaurant_id, p).await?;
        if let Some(rec) = rec {
            put(&mut tx, restaurant_id, rec).await?;
        }
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)
    }

    async fn payments_for(&self, restaurant_id: &str) -> AppResult<Vec<PlatformPayment>> {
        let rows = sqlx::query("SELECT data FROM platform_payments WHERE restaurant_id = $1 ORDER BY created_at DESC")
            .bind(restaurant_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        rows.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }

    async fn all_payments(&self) -> AppResult<Vec<PlatformPayment>> {
        let rows = sqlx::query("SELECT data FROM platform_payments ORDER BY created_at DESC").fetch_all(self.pool()).await.map_err(db_err)?;
        rows.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }

    async fn next_invoice_seq(&self) -> AppResult<i64> {
        let row = sqlx::query("UPDATE invoice_seq SET value = value + 1 WHERE id = 1 RETURNING value").fetch_one(self.pool()).await.map_err(db_err)?;
        Ok(row.get("value"))
    }
}
