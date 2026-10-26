use std::collections::HashMap;

use async_trait::async_trait;
use feast_application::ports::{BookingRepository, SlotRoom};
use feast_application::{AppError, AppResult};
use feast_domain::model::Booking;
use feast_domain::EventEnvelope;
use sqlx::{AnyConnection, Row};

use super::{db_err, from_json, insert_events, now_iso, to_json, SqlStore};

async fn insert_booking_tx(conn: &mut AnyConnection, b: &Booking, idempotency_key: &str) -> AppResult<()> {
    let res = sqlx::query(
        "INSERT INTO bookings (id, user_id, restaurant_id, code, date, time, guests, status, cover_charge, idempotency_key, data, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(&b.id)
    .bind(&b.user_id)
    .bind(&b.restaurant.id)
    .bind(&b.code)
    .bind(&b.date)
    .bind(&b.time)
    .bind(b.guests)
    .bind(b.status.as_str())
    .bind(b.cover_charge)
    .bind(idempotency_key)
    .bind(to_json(b)?)
    .bind(&b.created_at)
    .bind(now_iso())
    .execute(&mut *conn)
    .await;
    if let Err(sqlx::Error::Database(e)) = &res {
        if e.is_unique_violation() {
            return Err(AppError::conflict("DUPLICATE_REQUEST", "This booking was already submitted"));
        }
    }
    res.map_err(db_err)?;
    Ok(())
}

impl SqlStore {
    async fn bookings_where(&self, clause: &str, args: &[&str]) -> AppResult<Vec<Booking>> {
        let sql = format!("SELECT data FROM bookings {clause}");
        let mut q = sqlx::query(&sql);
        for a in args {
            q = q.bind(*a);
        }
        q.fetch_all(self.pool()).await.map_err(db_err)?.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }
}

/// Bookings that occupy a table: confirmed/seated, or awaiting payment within
/// the hold window (`$n` = the cutoff). Placeholders must appear in increasing
/// order in every query (SQLite numbers `$n` by order of appearance).
fn holds_table(n: u8) -> String {
    format!("(status IN ('CONFIRMED', 'SEATED') OR (status = 'PENDING_PAYMENT' AND created_at >= ${n}))")
}

#[async_trait]
impl BookingRepository for SqlStore {
    async fn insert_if_room(&self, b: &Booking, idempotency_key: &str, events: &[EventEnvelope], room: &SlotRoom) -> AppResult<bool> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        // 1. Lock this slot. The UPDATE takes a row lock on Postgres (and the
        //    database write lock on SQLite) held until commit, so concurrent
        //    bookings for the same slot run one after another, on any instance.
        sqlx::query("INSERT INTO slot_locks (restaurant_id, date, time, version) VALUES ($1, $2, $3, 0) ON CONFLICT DO NOTHING")
            .bind(&b.restaurant.id)
            .bind(&b.date)
            .bind(&b.time)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("UPDATE slot_locks SET version = version + 1 WHERE restaurant_id = $1 AND date = $2 AND time = $3")
            .bind(&b.restaurant.id)
            .bind(&b.date)
            .bind(&b.time)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        // 2. Count under the lock (sees every booking committed before we got it).
        let held: i64 = sqlx::query(&format!("SELECT COUNT(*) AS n FROM bookings WHERE restaurant_id = $1 AND date = $2 AND time = $3 AND {}", holds_table(4)))
            .bind(&b.restaurant.id)
            .bind(&b.date)
            .bind(&b.time)
            .bind(&room.pending_since)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_err)?
            .get("n");
        if held >= room.tables {
            tx.rollback().await.map_err(db_err)?;
            return Ok(false);
        }
        // 3. Insert while still holding the lock.
        insert_booking_tx(&mut tx, b, idempotency_key).await?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }

    async fn insert(&self, b: &Booking, idempotency_key: &str, events: &[EventEnvelope]) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        insert_booking_tx(&mut tx, b, idempotency_key).await?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)
    }

    async fn find_by_idempotency_key(&self, user_id: &str, key: &str) -> AppResult<Option<Booking>> {
        Ok(self.bookings_where("WHERE user_id = $1 AND idempotency_key = $2", &[user_id, key]).await?.pop())
    }

    async fn get(&self, id: &str) -> AppResult<Option<Booking>> {
        Ok(self.bookings_where("WHERE id = $1", &[id]).await?.pop())
    }

    async fn update(&self, b: &Booking, events: &[EventEnvelope]) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        sqlx::query("UPDATE bookings SET status = $1, data = $2, updated_at = $3 WHERE id = $4")
            .bind(b.status.as_str())
            .bind(to_json(b)?)
            .bind(now_iso())
            .bind(&b.id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)
    }

    async fn list_by_user(&self, user_id: &str) -> AppResult<Vec<Booking>> {
        self.bookings_where("WHERE user_id = $1 ORDER BY date, time", &[user_id]).await
    }

    async fn list_by_restaurant_between(&self, restaurant_id: &str, from: &str, to: &str) -> AppResult<Vec<Booking>> {
        self.bookings_where("WHERE restaurant_id = $1 AND date >= $2 AND date <= $3 ORDER BY date, time", &[restaurant_id, from, to]).await
    }

    async fn list_between(&self, from: &str, to: &str) -> AppResult<Vec<Booking>> {
        self.bookings_where("WHERE date >= $1 AND date <= $2 ORDER BY date, time", &[from, to]).await
    }

    async fn tables_held(&self, restaurant_id: &str, date: &str, pending_since: &str) -> AppResult<HashMap<String, i64>> {
        let rows =
            sqlx::query(&format!("SELECT time, COUNT(*) AS n FROM bookings WHERE restaurant_id = $1 AND date = $2 AND {} GROUP BY time", holds_table(3)))
                .bind(restaurant_id)
                .bind(date)
                .bind(pending_since)
                .fetch_all(self.pool())
                .await
                .map_err(db_err)?;
        Ok(rows.iter().map(|r| (r.get::<String, _>("time"), r.get::<i64, _>("n"))).collect())
    }

    async fn list_recent(&self, limit: u32) -> AppResult<Vec<Booking>> {
        self.bookings_where(&format!("ORDER BY date DESC, time DESC LIMIT {}", limit.max(1)), &[]).await
    }
}
