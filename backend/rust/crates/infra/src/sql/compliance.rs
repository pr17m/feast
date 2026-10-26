use async_trait::async_trait;
use feast_application::ports::{ComplianceRepository, ReviewInfo};
use feast_application::AppResult;
use feast_domain::model::ComplianceDocument;
use sqlx::any::AnyRow;
use sqlx::Row;

use super::{db_err, from_json, now_iso, to_json, SqlStore};

/// `storage_key` isn't serialized into the JSON (it's internal), so it's a column.
fn row_to_doc(r: &AnyRow) -> AppResult<ComplianceDocument> {
    let mut d: ComplianceDocument = from_json(&r.get::<String, _>("data"))?;
    d.storage_key = r.get("storage_key");
    Ok(d)
}

const COLS: &str = "data, storage_key";

#[async_trait]
impl ComplianceRepository for SqlStore {
    async fn insert_document(&self, d: &ComplianceDocument) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO compliance_documents (id, restaurant_id, requirement_id, status, storage_key, expires_on, uploaded_at, data) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&d.id)
        .bind(&d.restaurant_id)
        .bind(&d.requirement_id)
        .bind(d.status.as_str())
        .bind(&d.storage_key)
        .bind(&d.expires_on)
        .bind(&d.uploaded_at)
        .bind(to_json(d)?)
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get_document(&self, id: &str) -> AppResult<Option<ComplianceDocument>> {
        let row = sqlx::query(&format!("SELECT {COLS} FROM compliance_documents WHERE id = $1")).bind(id).fetch_optional(self.pool()).await.map_err(db_err)?;
        row.as_ref().map(row_to_doc).transpose()
    }

    async fn update_document(&self, d: &ComplianceDocument) -> AppResult<()> {
        sqlx::query("UPDATE compliance_documents SET status = $1, expires_on = $2, data = $3 WHERE id = $4")
            .bind(d.status.as_str())
            .bind(&d.expires_on)
            .bind(to_json(d)?)
            .bind(&d.id)
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn documents_for(&self, restaurant_id: &str) -> AppResult<Vec<ComplianceDocument>> {
        let rows = sqlx::query(&format!("SELECT {COLS} FROM compliance_documents WHERE restaurant_id = $1 ORDER BY uploaded_at"))
            .bind(restaurant_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        rows.iter().map(row_to_doc).collect()
    }

    async fn all_documents(&self) -> AppResult<Vec<ComplianceDocument>> {
        let rows = sqlx::query(&format!("SELECT {COLS} FROM compliance_documents ORDER BY uploaded_at")).fetch_all(self.pool()).await.map_err(db_err)?;
        rows.iter().map(row_to_doc).collect()
    }

    async fn review_info(&self, restaurant_id: &str) -> AppResult<ReviewInfo> {
        let row = sqlx::query("SELECT notes, submitted_at FROM restaurant_reviews WHERE restaurant_id = $1")
            .bind(restaurant_id)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
        Ok(row.map(|r| ReviewInfo { notes: r.get("notes"), submitted_at: r.get("submitted_at") }).unwrap_or_default())
    }

    async fn set_review_info(&self, restaurant_id: &str, info: &ReviewInfo) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO restaurant_reviews (restaurant_id, notes, submitted_at, updated_at) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (restaurant_id) DO UPDATE SET notes = excluded.notes, submitted_at = excluded.submitted_at, updated_at = excluded.updated_at",
        )
        .bind(restaurant_id)
        .bind(&info.notes)
        .bind(&info.submitted_at)
        .bind(now_iso())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }
}
