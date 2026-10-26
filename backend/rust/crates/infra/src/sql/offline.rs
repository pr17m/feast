//! Counter (offline) payments.

use async_trait::async_trait;
use feast_application::ports::OfflinePaymentRepository;
use feast_application::{AppError, AppResult};
use feast_domain::model::{BillPayment, OfflinePayment, OfflinePaymentStatus, PaymentIntent};
use feast_domain::EventEnvelope;
use sqlx::{AnyConnection, Row};

use super::billing::{complete_bill_tx, insert_intent_tx, release_deal_tx, update_intent_tx};
use super::{db_err, from_json, insert_events, to_json, SqlStore};

/// Flip a pending payment to its decided state; false if it was decided already.
async fn decide_tx(conn: &mut AnyConnection, p: &OfflinePayment) -> AppResult<bool> {
    let res = sqlx::query("UPDATE offline_payments SET status = $1, data = $2 WHERE id = $3 AND status = 'PENDING_APPROVAL'")
        .bind(p.status.as_str())
        .bind(to_json(p)?)
        .bind(&p.id)
        .execute(&mut *conn)
        .await
        .map_err(db_err)?;
    Ok(res.rows_affected() == 1)
}

impl SqlStore {
    async fn offline_where(&self, sql_tail: &str, args: &[&str]) -> AppResult<Vec<OfflinePayment>> {
        let sql = format!("SELECT data FROM offline_payments {sql_tail}");
        let mut q = sqlx::query(&sql);
        for a in args {
            q = q.bind(*a);
        }
        q.fetch_all(self.pool()).await.map_err(db_err)?.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }
}

#[async_trait]
impl OfflinePaymentRepository for SqlStore {
    async fn insert_offline(&self, p: &OfflinePayment, intent: &PaymentIntent, events: &[EventEnvelope]) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        insert_intent_tx(&mut tx, intent).await?;
        let res = sqlx::query(
            "INSERT INTO offline_payments (id, bill_id, intent_id, restaurant_id, user_id, status, submitted_at, expires_at, data) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(&p.id)
        .bind(&p.bill_id)
        .bind(&p.intent_id)
        .bind(&p.restaurant.id)
        .bind(&p.user_id)
        .bind(p.status.as_str())
        .bind(&p.submitted_at)
        .bind(&p.expires_at)
        .bind(to_json(p)?)
        .execute(&mut *tx)
        .await;
        if let Err(sqlx::Error::Database(e)) = &res {
            if e.is_unique_violation() {
                return Err(AppError::conflict("OFFLINE_PENDING", "This bill already has a counter payment waiting for the restaurant"));
            }
        }
        res.map_err(db_err)?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)
    }

    async fn get_offline(&self, id: &str) -> AppResult<Option<OfflinePayment>> {
        Ok(self.offline_where("WHERE id = $1", &[id]).await?.pop())
    }

    async fn latest_for_bill(&self, bill_id: &str) -> AppResult<Option<OfflinePayment>> {
        Ok(self.offline_where("WHERE bill_id = $1 ORDER BY submitted_at DESC, id DESC LIMIT 1", &[bill_id]).await?.pop())
    }

    async fn offline_for_restaurant(&self, restaurant_id: &str, status: Option<OfflinePaymentStatus>, limit: u32) -> AppResult<Vec<OfflinePayment>> {
        let limit = limit.clamp(1, 500);
        match status {
            Some(s) => {
                self.offline_where(&format!("WHERE restaurant_id = $1 AND status = $2 ORDER BY submitted_at DESC LIMIT {limit}"), &[restaurant_id, s.as_str()])
                    .await
            }
            None => self.offline_where(&format!("WHERE restaurant_id = $1 ORDER BY submitted_at DESC LIMIT {limit}"), &[restaurant_id]).await,
        }
    }

    async fn expired_pending(&self, now: &str, limit: u32) -> AppResult<Vec<OfflinePayment>> {
        self.offline_where(&format!("WHERE status = 'PENDING_APPROVAL' AND expires_at <= $1 ORDER BY expires_at LIMIT {}", limit.clamp(1, 500)), &[now]).await
    }

    async fn pending_for_user(&self, user_id: &str) -> AppResult<Vec<OfflinePayment>> {
        self.offline_where("WHERE user_id = $1 AND status = 'PENDING_APPROVAL' ORDER BY submitted_at DESC LIMIT 50", &[user_id]).await
    }

    async fn approve_offline(&self, p: &OfflinePayment, intent: &PaymentIntent, bp: &BillPayment, events: &[EventEnvelope]) -> AppResult<bool> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        if !decide_tx(&mut tx, p).await? {
            return Ok(false);
        }
        if !update_intent_tx(&mut tx, intent).await? {
            return Ok(false);
        }
        // ALREADY_PAID (bill paid online meanwhile) rolls everything back.
        complete_bill_tx(&mut tx, intent, bp).await?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }

    async fn close_offline(&self, p: &OfflinePayment, intent: &PaymentIntent, events: &[EventEnvelope]) -> AppResult<bool> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        if !decide_tx(&mut tx, p).await? {
            return Ok(false);
        }
        update_intent_tx(&mut tx, intent).await?;
        release_deal_tx(&mut tx, &p.bill_id, &intent.id).await?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }
}
