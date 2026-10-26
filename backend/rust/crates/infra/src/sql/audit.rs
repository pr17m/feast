//! The audit trail table (append-only). Event-derived records are written by
//! `insert_events` in the caller's transaction; direct records via `append`.

use async_trait::async_trait;
use feast_application::ports::AuditTrailRepository;
use feast_application::AppResult;
use feast_domain::audit::{cursor_of, parse_cursor, AuditPage, AuditQuery, AuditRecord};
use sqlx::{AnyConnection, Row};

use super::{db_err, from_json, to_json, SqlStore};

/// Insert one record; replays of the same event are ignored (`id` is derived from it).
pub(crate) async fn append_tx(conn: &mut AnyConnection, r: &AuditRecord) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO audit_trail (id, at, restaurant_id, entity_type, entity_id, action, actor_kind, actor_id, subject_user_id, event_id, data) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) ON CONFLICT (id) DO NOTHING",
    )
    .bind(&r.id)
    .bind(&r.at)
    .bind(&r.restaurant_id)
    .bind(&r.entity_type)
    .bind(&r.entity_id)
    .bind(&r.action)
    .bind(r.actor.kind.as_str())
    .bind(&r.actor.user_id)
    .bind(&r.subject_user_id)
    .bind(&r.event_id)
    .bind(to_json(r)?)
    .execute(&mut *conn)
    .await
    .map_err(db_err)?;
    Ok(())
}

#[async_trait]
impl AuditTrailRepository for SqlStore {
    async fn append(&self, r: &AuditRecord) -> AppResult<()> {
        let mut conn = self.pool().acquire().await.map_err(db_err)?;
        append_tx(&mut conn, r).await
    }

    async fn query(&self, q: &AuditQuery) -> AppResult<AuditPage> {
        // Placeholders must appear in increasing order ($1, $2, …) for every
        // dialect, so the WHERE clause and the binds are built together.
        let mut clauses: Vec<String> = vec![];
        let mut binds: Vec<String> = vec![];
        let mut add = |sql: &str, v: &str, binds: &mut Vec<String>| {
            binds.push(v.to_owned());
            clauses.push(sql.replace('?', &format!("${}", binds.len())));
        };
        if let Some(v) = &q.restaurant_id {
            add("restaurant_id = ?", v, &mut binds);
        }
        if let Some(v) = &q.entity_type {
            add("entity_type = ?", v, &mut binds);
        }
        if let Some(v) = &q.entity_id {
            add("entity_id = ?", v, &mut binds);
        }
        if let Some(v) = &q.actor_id {
            add("actor_id = ?", v, &mut binds);
        }
        if let Some(v) = &q.subject_user_id {
            add("subject_user_id = ?", v, &mut binds);
        }
        if let Some(v) = &q.from {
            add("at >= ?", v, &mut binds);
        }
        if let Some(v) = &q.to {
            add("at < ?", v, &mut binds);
        }
        if !q.entity_ids.is_empty() {
            let mut ph = vec![];
            for id in q.entity_ids.iter().take(50) {
                binds.push(id.clone());
                ph.push(format!("${}", binds.len()));
            }
            clauses.push(format!("entity_id IN ({})", ph.join(", ")));
        }
        if let Some((at, id)) = q.cursor.as_deref().and_then(parse_cursor) {
            binds.push(at.clone());
            let a1 = binds.len();
            binds.push(at);
            let a2 = binds.len();
            binds.push(id);
            let i = binds.len();
            clauses.push(format!("(at < ${a1} OR (at = ${a2} AND id < ${i}))"));
        }
        let limit = q.limit.clamp(1, feast_domain::audit::MAX_PAGE);
        let sql = format!(
            "SELECT data FROM audit_trail{} ORDER BY at DESC, id DESC LIMIT {}",
            if clauses.is_empty() { String::new() } else { format!(" WHERE {}", clauses.join(" AND ")) },
            limit + 1
        );
        let mut query = sqlx::query(&sql);
        for b in &binds {
            query = query.bind(b);
        }
        let rows = query.fetch_all(self.pool()).await.map_err(db_err)?;
        let mut items: Vec<AuditRecord> = rows.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect::<AppResult<_>>()?;
        let next_cursor = if items.len() > limit as usize {
            items.truncate(limit as usize);
            items.last().map(cursor_of)
        } else {
            None
        };
        Ok(AuditPage { items, next_cursor })
    }
}
