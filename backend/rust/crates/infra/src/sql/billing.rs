use async_trait::async_trait;
use feast_application::ports::{BillingRepository, DealClaim, Settled, SlotRoom};
use feast_application::{AppError, AppResult};
use feast_domain::deals::ClaimOutcome;
use feast_domain::model::{Bill, BillPayment, Booking, PaymentIntent, PlatformPayment, SubscriptionRecord};
use feast_domain::refunds::Refund;
use feast_domain::EventEnvelope;
use sqlx::{AnyConnection, Row};

use super::{db_err, from_json, insert_events, now_iso, to_json, SqlStore};

fn status_str(i: &PaymentIntent) -> String {
    serde_json::to_value(i.status).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

/// Write the intent unless it already SUCCEEDED (a succeeded payment is final).
/// Returns whether a row changed. Completing a payment uses this as the
/// *claim*: when the browser's confirm and the provider's webhook race, only
/// the first transaction to flip the intent to SUCCEEDED applies the effects.
pub(super) async fn update_intent_tx(conn: &mut AnyConnection, i: &PaymentIntent) -> AppResult<bool> {
    let res = sqlx::query("UPDATE payment_intents SET status = $1, data = $2, updated_at = $3 WHERE id = $4 AND status <> 'SUCCEEDED'")
        .bind(status_str(i))
        .bind(to_json(i)?)
        .bind(now_iso())
        .bind(&i.id)
        .execute(&mut *conn)
        .await
        .map_err(db_err)?;
    Ok(res.rows_affected() == 1)
}

pub(super) async fn insert_intent_tx(conn: &mut AnyConnection, i: &PaymentIntent) -> AppResult<()> {
    let purpose = serde_json::to_value(i.purpose).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
    sqlx::query(
        "INSERT INTO payment_intents (id, purpose, reference_id, amount, status, provider, provider_ref, data, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(&i.id)
    .bind(purpose)
    .bind(&i.reference_id)
    .bind(i.amount)
    .bind(status_str(i))
    .bind(&i.provider)
    .bind(&i.provider_ref)
    .bind(to_json(i)?)
    .bind(now_iso())
    .bind(now_iso())
    .execute(&mut *conn)
    .await
    .map_err(db_err)?;
    Ok(())
}

/// Free a held deal (HELD or PENDING_OFFLINE) if `intent_id` still holds it.
pub(super) async fn release_deal_tx(conn: &mut AnyConnection, bill_id: &str, intent_id: &str) -> AppResult<bool> {
    let res = sqlx::query("UPDATE deal_claims SET status = 'RELEASED' WHERE bill_id = $1 AND intent_id = $2 AND status IN ('HELD', 'PENDING_OFFLINE')")
        .bind(bill_id)
        .bind(intent_id)
        .execute(&mut *conn)
        .await
        .map_err(db_err)?;
    Ok(res.rows_affected() == 1)
}

/// Record a paid bill inside the caller's transaction: insert the bill payment
/// (a bill can be paid once: `ALREADY_PAID`) and turn its deal USED. The
/// caller has already claimed the intent.
pub(super) async fn complete_bill_tx(conn: &mut AnyConnection, i: &PaymentIntent, bp: &BillPayment) -> AppResult<()> {
    let res = sqlx::query(
        "INSERT INTO bill_payments (id, user_id, restaurant_id, bill_id, intent_id, amount, paid_at, data) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&bp.id)
    .bind(&bp.user_id)
    .bind(&bp.restaurant.id)
    .bind(&bp.bill.id)
    .bind(&i.id)
    .bind(bp.payment.amount)
    .bind(&bp.payment.paid_at)
    .bind(to_json(bp)?)
    .execute(&mut *conn)
    .await;
    if let Err(sqlx::Error::Database(e)) = &res {
        if e.is_unique_violation() {
            return Err(AppError::conflict("ALREADY_PAID", "This bill is already paid"));
        }
    }
    res.map_err(db_err)?;
    // The bill's deal (if it had one) now counts for good.
    sqlx::query("UPDATE deal_claims SET status = 'USED', intent_id = $1 WHERE bill_id = $2")
        .bind(&i.id)
        .bind(&bp.bill.id)
        .execute(&mut *conn)
        .await
        .map_err(db_err)?;
    Ok(())
}

impl SqlStore {
    async fn payments_where(&self, clause: &str, arg: Option<&str>) -> AppResult<Vec<BillPayment>> {
        let sql = format!("SELECT data FROM bill_payments {clause}");
        let mut q = sqlx::query(&sql);
        if let Some(a) = arg {
            q = q.bind(a);
        }
        q.fetch_all(self.pool()).await.map_err(db_err)?.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }
}

#[async_trait]
impl BillingRepository for SqlStore {
    async fn insert_bill(&self, b: &Bill) -> AppResult<()> {
        sqlx::query("INSERT INTO bills (id, restaurant_id, data, created_at) VALUES ($1, $2, $3, $4)")
            .bind(&b.id)
            .bind(&b.restaurant_id)
            .bind(to_json(b)?)
            .bind(now_iso())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn get_bill(&self, id: &str) -> AppResult<Option<Bill>> {
        let row = sqlx::query("SELECT data FROM bills WHERE id = $1").bind(id).fetch_optional(self.pool()).await.map_err(db_err)?;
        row.map(|r| from_json(&r.get::<String, _>("data"))).transpose()
    }

    async fn is_bill_paid(&self, bill_id: &str) -> AppResult<bool> {
        Ok(sqlx::query("SELECT 1 FROM bill_payments WHERE bill_id = $1").bind(bill_id).fetch_optional(self.pool()).await.map_err(db_err)?.is_some())
    }

    async fn insert_intent(&self, i: &PaymentIntent, events: &[EventEnvelope]) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        insert_intent_tx(&mut tx, i).await?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)
    }

    async fn get_intent(&self, id: &str) -> AppResult<Option<PaymentIntent>> {
        let row = sqlx::query("SELECT data FROM payment_intents WHERE id = $1").bind(id).fetch_optional(self.pool()).await.map_err(db_err)?;
        row.map(|r| from_json(&r.get::<String, _>("data"))).transpose()
    }

    async fn find_intent_by_provider_ref(&self, provider: &str, provider_ref: &str) -> AppResult<Option<PaymentIntent>> {
        let row = sqlx::query("SELECT data FROM payment_intents WHERE provider = $1 AND provider_ref = $2")
            .bind(provider)
            .bind(provider_ref)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
        row.map(|r| from_json(&r.get::<String, _>("data"))).transpose()
    }

    async fn update_intent(&self, i: &PaymentIntent, events: &[EventEnvelope]) -> AppResult<bool> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let changed = update_intent_tx(&mut tx, i).await?;
        if changed {
            insert_events(&mut tx, events).await?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(changed)
    }

    async fn complete_bill_payment(&self, i: &PaymentIntent, bp: &BillPayment, events: &[EventEnvelope]) -> AppResult<Settled> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        if !update_intent_tx(&mut tx, i).await? {
            return Ok(Settled::AlreadySettled); // already completed (webhook/confirm race)
        }
        match complete_bill_tx(&mut tx, i, bp).await {
            Ok(()) => {}
            // Paid meanwhile by another payment (another card payment, or at the
            // counter): this money can't count too. Nothing is written.
            Err(e) if e.code == "ALREADY_PAID" => {
                tx.rollback().await.map_err(db_err)?;
                return Ok(Settled::NotApplied);
            }
            Err(e) => return Err(e),
        }
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(Settled::Applied)
    }

    async fn record_unapplied_payment(&self, i: &PaymentIntent, refund: &Refund, events: &[EventEnvelope]) -> AppResult<bool> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        if !update_intent_tx(&mut tx, i).await? {
            return Ok(false);
        }
        super::refunds::insert_refund_tx(&mut tx, refund).await?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }

    async fn claim_deal(&self, c: &DealClaim, limit: Option<i64>, held_since: &str, events: &[EventEnvelope]) -> AppResult<ClaimOutcome> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        // 1. Lock the restaurant's day (row lock on Postgres, write lock on SQLite)
        //    so concurrent claims are counted one after another, on any instance.
        sqlx::query("INSERT INTO deal_budget_days (restaurant_id, date, version) VALUES ($1, $2, 0) ON CONFLICT DO NOTHING")
            .bind(&c.restaurant_id)
            .bind(&c.date)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("UPDATE deal_budget_days SET version = version + 1 WHERE restaurant_id = $1 AND date = $2")
            .bind(&c.restaurant_id)
            .bind(&c.date)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        // 2. Count the other bills' live claims under the lock.
        if let Some(limit) = limit {
            let used: i64 = sqlx::query(
                "SELECT COUNT(*) AS n FROM deal_claims WHERE restaurant_id = $1 AND date = $2 AND bill_id <> $3 \
                 AND (status IN ('USED', 'PENDING_OFFLINE') OR (status = 'HELD' AND claimed_at >= $4))",
            )
            .bind(&c.restaurant_id)
            .bind(&c.date)
            .bind(&c.bill_id)
            .bind(held_since)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_err)?
            .get("n");
            if used >= limit {
                tx.rollback().await.map_err(db_err)?;
                return Ok(ClaimOutcome::LimitReached);
            }
        }
        // 2b. One discounted bill per booking (a booking is for one restaurant
        //     and day, so the lock above covers it too).
        if let Some(booking_id) = &c.booking_id {
            let taken: i64 = sqlx::query(
                "SELECT COUNT(*) AS n FROM deal_claims WHERE booking_id = $1 AND bill_id <> $2 \
                 AND (status IN ('USED', 'PENDING_OFFLINE') OR (status = 'HELD' AND claimed_at >= $3))",
            )
            .bind(booking_id)
            .bind(&c.bill_id)
            .bind(held_since)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_err)?
            .get("n");
            if taken > 0 {
                tx.rollback().await.map_err(db_err)?;
                return Ok(ClaimOutcome::BookingDealUsed);
            }
        }
        // 3. Hold (or refresh the hold) for this bill; a USED claim stays USED.
        sqlx::query(
            "INSERT INTO deal_claims (bill_id, restaurant_id, date, intent_id, status, claimed_at, booking_id) VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (bill_id) DO UPDATE SET intent_id = excluded.intent_id, claimed_at = excluded.claimed_at, \
             status = CASE WHEN deal_claims.status = 'USED' THEN 'USED' ELSE excluded.status END",
        )
        .bind(&c.bill_id)
        .bind(&c.restaurant_id)
        .bind(&c.date)
        .bind(&c.intent_id)
        .bind(if c.pending_offline { "PENDING_OFFLINE" } else { "HELD" })
        .bind(&c.claimed_at)
        .bind(&c.booking_id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(ClaimOutcome::Claimed)
    }

    async fn release_deal(&self, bill_id: &str, intent_id: &str, events: &[EventEnvelope]) -> AppResult<bool> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let freed = release_deal_tx(&mut tx, bill_id, intent_id).await?;
        if freed {
            insert_events(&mut tx, events).await?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(freed)
    }

    async fn booking_deal_used(&self, booking_id: &str) -> AppResult<bool> {
        let n: i64 = sqlx::query("SELECT COUNT(*) AS n FROM deal_claims WHERE booking_id = $1 AND status = 'USED'")
            .bind(booking_id)
            .fetch_one(self.pool())
            .await
            .map_err(db_err)?
            .get("n");
        Ok(n > 0)
    }

    async fn deals_used(&self, restaurant_id: &str, date: &str, held_since: &str) -> AppResult<i64> {
        Ok(sqlx::query(
            "SELECT COUNT(*) AS n FROM deal_claims WHERE restaurant_id = $1 AND date = $2 \
             AND (status IN ('USED', 'PENDING_OFFLINE') OR (status = 'HELD' AND claimed_at >= $3))",
        )
        .bind(restaurant_id)
        .bind(date)
        .bind(held_since)
        .fetch_one(self.pool())
        .await
        .map_err(db_err)?
        .get("n"))
    }

    async fn complete_booking_payment(&self, i: &PaymentIntent, booking: &Booking, room: &SlotRoom, events: &[EventEnvelope]) -> AppResult<Settled> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        if !update_intent_tx(&mut tx, i).await? {
            return Ok(Settled::AlreadySettled);
        }
        // Same slot lock as booking, so the table count below can't race a new booking.
        sqlx::query("INSERT INTO slot_locks (restaurant_id, date, time, version) VALUES ($1, $2, $3, 0) ON CONFLICT DO NOTHING")
            .bind(&booking.restaurant.id)
            .bind(&booking.date)
            .bind(&booking.time)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("UPDATE slot_locks SET version = version + 1 WHERE restaurant_id = $1 AND date = $2 AND time = $3")
            .bind(&booking.restaurant.id)
            .bind(&booking.date)
            .bind(&booking.time)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        let row = sqlx::query("SELECT status, created_at FROM bookings WHERE id = $1").bind(&booking.id).fetch_optional(&mut *tx).await.map_err(db_err)?;
        let Some(row) = row else {
            tx.rollback().await.map_err(db_err)?;
            return Ok(Settled::NotApplied);
        };
        let status: String = row.get("status");
        let created_at: String = row.get("created_at");
        // Its table hold lapsed: confirm only if a table is still free (the
        // count doesn't include this booking, whose hold no longer counts).
        let hold_lapsed = created_at.as_str() < room.pending_since.as_str();
        let table_free = if hold_lapsed {
            let held: i64 = sqlx::query(&format!(
                "SELECT COUNT(*) AS n FROM bookings WHERE restaurant_id = $1 AND date = $2 AND time = $3 AND id <> $4 AND {}",
                super::bookings::holds_table(5)
            ))
            .bind(&booking.restaurant.id)
            .bind(&booking.date)
            .bind(&booking.time)
            .bind(&booking.id)
            .bind(&room.pending_since)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_err)?
            .get("n");
            held < room.tables
        } else {
            true
        };
        if !feast_domain::refunds::booking_payment_applies(&status, hold_lapsed, table_free) {
            tx.rollback().await.map_err(db_err)?;
            return Ok(Settled::NotApplied);
        }
        let res = sqlx::query("UPDATE bookings SET status = $1, data = $2, updated_at = $3 WHERE id = $4 AND status = 'PENDING_PAYMENT'")
            .bind(booking.status.as_str())
            .bind(to_json(booking)?)
            .bind(now_iso())
            .bind(&booking.id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        if res.rows_affected() != 1 {
            tx.rollback().await.map_err(db_err)?;
            return Ok(Settled::NotApplied);
        }
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(Settled::Applied)
    }

    async fn complete_subscription_payment(
        &self,
        i: &PaymentIntent,
        restaurant_id: &str,
        rec: &SubscriptionRecord,
        payment: &PlatformPayment,
        events: &[EventEnvelope],
    ) -> AppResult<bool> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        if !update_intent_tx(&mut tx, i).await? {
            return Ok(false);
        }
        super::subscriptions::insert_payment_tx(&mut tx, restaurant_id, payment).await?;
        super::subscriptions::put(&mut tx, restaurant_id, rec).await?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }

    async fn get_bill_payment(&self, id: &str) -> AppResult<Option<BillPayment>> {
        Ok(self.payments_where("WHERE id = $1", Some(id)).await?.pop())
    }

    async fn payments_by_user(&self, user_id: &str) -> AppResult<Vec<BillPayment>> {
        self.payments_where("WHERE user_id = $1 ORDER BY paid_at DESC", Some(user_id)).await
    }

    async fn payments_by_restaurant(&self, restaurant_id: &str) -> AppResult<Vec<BillPayment>> {
        self.payments_where("WHERE restaurant_id = $1 ORDER BY paid_at DESC", Some(restaurant_id)).await
    }

    async fn list_recent_payments(&self, limit: u32) -> AppResult<Vec<BillPayment>> {
        self.payments_where(&format!("ORDER BY paid_at DESC LIMIT {}", limit.max(1)), None).await
    }

    async fn payments_since(&self, since: &str) -> AppResult<Vec<BillPayment>> {
        self.payments_where("WHERE paid_at >= $1 ORDER BY paid_at", Some(since)).await
    }
}
