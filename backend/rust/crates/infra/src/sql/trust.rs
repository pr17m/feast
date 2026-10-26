//! Grievance cases, reviews and the staff audit trail.

use async_trait::async_trait;
use feast_application::ports::{AuditRepository, CaseRepository, RatingStats, ReviewRepository};
use feast_application::AppResult;
use feast_domain::model::{AuditEntry, Case, CaseKind, Review, ReviewStatus};
use feast_domain::EventEnvelope;
use sqlx::Row;

use super::{db_err, from_json, insert_events, to_json, SqlStore};

fn kind_str(k: CaseKind) -> &'static str {
    match k {
        CaseKind::Complaint => "COMPLAINT",
        CaseKind::Report => "REPORT",
    }
}

fn review_status(s: ReviewStatus) -> &'static str {
    match s {
        ReviewStatus::Published => "PUBLISHED",
        ReviewStatus::Hidden => "HIDDEN",
    }
}

impl SqlStore {
    async fn cases_where(&self, clause: &str, args: &[&str]) -> AppResult<Vec<Case>> {
        let sql = format!("SELECT data FROM cases {clause}");
        let mut q = sqlx::query(&sql);
        for a in args {
            q = q.bind(*a);
        }
        q.fetch_all(self.pool()).await.map_err(db_err)?.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }

    async fn reviews_where(&self, clause: &str, args: &[&str]) -> AppResult<Vec<Review>> {
        let sql = format!("SELECT data FROM reviews {clause}");
        let mut q = sqlx::query(&sql);
        for a in args {
            q = q.bind(*a);
        }
        q.fetch_all(self.pool()).await.map_err(db_err)?.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }
}

#[async_trait]
impl CaseRepository for SqlStore {
    async fn insert_case(&self, c: &Case, events: &[EventEnvelope]) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        sqlx::query(
            "INSERT INTO cases (id, kind, status, restaurant_id, customer_id, booking_id, bill_payment_id, due_at, version, data, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(&c.id)
        .bind(kind_str(c.kind))
        .bind(c.status.as_str())
        .bind(&c.restaurant.id)
        .bind(&c.customer.id)
        .bind(&c.booking_id)
        .bind(&c.bill_payment_id)
        .bind(&c.due_at)
        .bind(c.version)
        .bind(to_json(c)?)
        .bind(&c.created_at)
        .bind(&c.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)
    }

    async fn update_case(&self, c: &Case, events: &[EventEnvelope]) -> AppResult<bool> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let res = sqlx::query("UPDATE cases SET status = $1, due_at = $2, version = $3, data = $4, updated_at = $5 WHERE id = $6 AND version = $7")
            .bind(c.status.as_str())
            .bind(&c.due_at)
            .bind(c.version)
            .bind(to_json(c)?)
            .bind(&c.updated_at)
            .bind(&c.id)
            .bind(c.version - 1)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        if res.rows_affected() != 1 {
            tx.rollback().await.map_err(db_err)?;
            return Ok(false);
        }
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }

    async fn get_case(&self, id: &str) -> AppResult<Option<Case>> {
        Ok(self.cases_where("WHERE id = $1", &[id]).await?.pop())
    }

    async fn cases_for_restaurant(&self, restaurant_id: &str) -> AppResult<Vec<Case>> {
        self.cases_where("WHERE restaurant_id = $1 ORDER BY created_at DESC", &[restaurant_id]).await
    }

    async fn cases_for_customer(&self, user_id: &str) -> AppResult<Vec<Case>> {
        self.cases_where("WHERE customer_id = $1 ORDER BY created_at DESC", &[user_id]).await
    }

    async fn all_cases(&self) -> AppResult<Vec<Case>> {
        self.cases_where("ORDER BY created_at DESC", &[]).await
    }

    async fn open_case_for_visit(&self, kind: CaseKind, booking_id: Option<&str>, bill_payment_id: Option<&str>) -> AppResult<Option<Case>> {
        let (col, id) = match (booking_id, bill_payment_id) {
            (Some(b), _) => ("booking_id", b),
            (None, Some(p)) => ("bill_payment_id", p),
            (None, None) => return Ok(None),
        };
        let clause = format!("WHERE kind = $1 AND {col} = $2 AND status <> 'RESOLVED'");
        Ok(self.cases_where(&clause, &[kind_str(kind), id]).await?.pop())
    }

    async fn overdue_cases(&self, now: &str) -> AppResult<Vec<Case>> {
        self.cases_where("WHERE status IN ('AWAITING_RESTAURANT', 'AWAITING_CUSTOMER') AND due_at <= $1 ORDER BY due_at", &[now]).await
    }
}

#[async_trait]
impl ReviewRepository for SqlStore {
    async fn insert_review(&self, r: &Review, visit_key: &str, events: &[EventEnvelope]) -> AppResult<bool> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        let res = sqlx::query(
            "INSERT INTO reviews (id, restaurant_id, user_id, visit_key, status, rating, flagged, data, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT (visit_key) DO NOTHING",
        )
        .bind(&r.id)
        .bind(&r.restaurant.id)
        .bind(r.user_id.as_deref().unwrap_or_default())
        .bind(visit_key)
        .bind(review_status(r.status))
        .bind(r.rating)
        .bind(r.flag.is_some() as i64)
        .bind(to_json(r)?)
        .bind(&r.created_at)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        if res.rows_affected() != 1 {
            tx.rollback().await.map_err(db_err)?;
            return Ok(false);
        }
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }

    async fn update_review(&self, r: &Review, events: &[EventEnvelope]) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        sqlx::query("UPDATE reviews SET status = $1, rating = $2, flagged = $3, data = $4 WHERE id = $5")
            .bind(review_status(r.status))
            .bind(r.rating)
            .bind(r.flag.is_some() as i64)
            .bind(to_json(r)?)
            .bind(&r.id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        insert_events(&mut tx, events).await?;
        tx.commit().await.map_err(db_err)
    }

    async fn get_review(&self, id: &str) -> AppResult<Option<Review>> {
        Ok(self.reviews_where("WHERE id = $1", &[id]).await?.pop())
    }

    async fn reviews_for_restaurant(&self, restaurant_id: &str, published_only: bool, limit: u32) -> AppResult<Vec<Review>> {
        let filter = if published_only { " AND status = 'PUBLISHED'" } else { "" };
        self.reviews_where(&format!("WHERE restaurant_id = $1{filter} ORDER BY created_at DESC LIMIT {}", limit.clamp(1, 500)), &[restaurant_id]).await
    }

    async fn reviews_by_user(&self, user_id: &str) -> AppResult<Vec<Review>> {
        self.reviews_where("WHERE user_id = $1 ORDER BY created_at DESC", &[user_id]).await
    }

    async fn reviews_since(&self, since: &str) -> AppResult<Vec<Review>> {
        self.reviews_where("WHERE created_at >= $1 ORDER BY created_at", &[since]).await
    }

    async fn reviews_needing_attention(&self) -> AppResult<Vec<Review>> {
        self.reviews_where("WHERE flagged = 1 OR status = 'HIDDEN' ORDER BY created_at DESC LIMIT 200", &[]).await
    }

    async fn rating_stats(&self, restaurant_id: &str) -> AppResult<RatingStats> {
        let rows = sqlx::query("SELECT rating, COUNT(*) AS n FROM reviews WHERE restaurant_id = $1 AND status = 'PUBLISHED' GROUP BY rating")
            .bind(restaurant_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        let mut s = RatingStats::default();
        for r in rows {
            let (stars, n): (i64, i64) = (r.get("rating"), r.get("n"));
            if (1..=5).contains(&stars) {
                s.distribution[(stars - 1) as usize] = n;
                s.sum += stars * n;
                s.count += n;
            }
        }
        Ok(s)
    }
}

#[async_trait]
impl AuditRepository for SqlStore {
    async fn record(&self, e: &AuditEntry) -> AppResult<()> {
        sqlx::query("INSERT INTO audit_log (id, restaurant_id, data, at) VALUES ($1, $2, $3, $4)")
            .bind(&e.id)
            .bind(&e.restaurant_id)
            .bind(to_json(e)?)
            .bind(&e.at)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn audit_for(&self, restaurant_id: &str, limit: u32) -> AppResult<Vec<AuditEntry>> {
        let sql = format!("SELECT data FROM audit_log WHERE restaurant_id = $1 ORDER BY at DESC LIMIT {}", limit.clamp(1, 500));
        sqlx::query(&sql).bind(restaurant_id).fetch_all(self.pool()).await.map_err(db_err)?.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }
}
