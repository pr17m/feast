use async_trait::async_trait;
use feast_application::ports::RestaurantRepository;
use feast_application::AppResult;
use std::collections::HashMap;

use feast_domain::model::{IsoDateTime, RatingBase, Restaurant};
use serde::{Deserialize, Serialize};
use sqlx::any::AnyRow;
use sqlx::Row;

use super::{db_err, from_json, now_iso, to_json, SqlStore};

const COLS: &str = "detail, menu, short_code, breakfast, extras";

/// Staff-only restaurant fields kept out of the public `detail` JSON.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Extras {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    support_access_until: Option<IsoDateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rating_base: Option<RatingBase>,
}

fn extras_json(r: &Restaurant) -> AppResult<String> {
    to_json(&Extras { support_access_until: r.support_access_until.clone(), rating_base: r.rating_base })
}

fn row_to_restaurant(r: &AnyRow) -> AppResult<Restaurant> {
    let extras: Extras = r.get::<Option<String>, _>("extras").map(|e| from_json(&e)).transpose()?.unwrap_or_default();
    Ok(Restaurant {
        detail: from_json(&r.get::<String, _>("detail"))?,
        menu: from_json(&r.get::<String, _>("menu"))?,
        short_code: r.get("short_code"),
        breakfast: r.get::<i64, _>("breakfast") != 0,
        support_access_until: extras.support_access_until,
        rating_base: extras.rating_base,
    })
}

impl SqlStore {
    async fn fetch_restaurants(&self, sql: &str, arg: Option<&str>) -> AppResult<Vec<Restaurant>> {
        let mut q = sqlx::query(sql);
        if let Some(a) = arg {
            q = q.bind(a);
        }
        q.fetch_all(self.pool()).await.map_err(db_err)?.iter().map(row_to_restaurant).collect()
    }
}

#[async_trait]
impl RestaurantRepository for SqlStore {
    async fn list(&self) -> AppResult<Vec<Restaurant>> {
        self.fetch_restaurants(&format!("SELECT {COLS} FROM restaurants ORDER BY seq, id"), None).await
    }

    async fn list_by_city(&self, city_id: &str) -> AppResult<Vec<Restaurant>> {
        self.fetch_restaurants(&format!("SELECT {COLS} FROM restaurants WHERE city_id = $1 ORDER BY seq, id"), Some(city_id)).await
    }

    async fn get(&self, id: &str) -> AppResult<Option<Restaurant>> {
        Ok(self.fetch_restaurants(&format!("SELECT {COLS} FROM restaurants WHERE id = $1"), Some(id)).await?.pop())
    }

    async fn get_by_slug(&self, slug: &str) -> AppResult<Option<Restaurant>> {
        Ok(self.fetch_restaurants(&format!("SELECT {COLS} FROM restaurants WHERE slug = $1"), Some(slug)).await?.pop())
    }

    async fn find_by_short_code(&self, code: &str) -> AppResult<Option<Restaurant>> {
        Ok(self.fetch_restaurants(&format!("SELECT {COLS} FROM restaurants WHERE LOWER(short_code) = $1"), Some(&code.to_lowercase())).await?.pop())
    }

    async fn insert(&self, r: &Restaurant) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        sqlx::query(
            "INSERT INTO restaurants (id, slug, city_id, status, country, short_code, breakfast, detail, menu, updated_at, extras, seq) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, (SELECT COALESCE(MAX(seq), 0) + 1 FROM restaurants))",
        )
        .bind(&r.detail.id)
        .bind(&r.detail.slug)
        .bind(&r.detail.city_id)
        .bind(r.detail.status.as_str())
        .bind(&r.detail.country_code)
        .bind(&r.short_code)
        .bind(r.breakfast as i64)
        .bind(to_json(&r.detail)?)
        .bind(to_json(&r.menu)?)
        .bind(now_iso())
        .bind(extras_json(r)?)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }

    async fn update(&self, r: &Restaurant) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        sqlx::query(
            "UPDATE restaurants SET slug = $1, city_id = $2, status = $3, country = $4, short_code = $5, breakfast = $6, \
             detail = $7, menu = $8, updated_at = $9, extras = $10 WHERE id = $11",
        )
        .bind(&r.detail.slug)
        .bind(&r.detail.city_id)
        .bind(r.detail.status.as_str())
        .bind(&r.detail.country_code)
        .bind(&r.short_code)
        .bind(r.breakfast as i64)
        .bind(to_json(&r.detail)?)
        .bind(to_json(&r.menu)?)
        .bind(now_iso())
        .bind(extras_json(r)?)
        .bind(&r.detail.id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }

    async fn city_ids(&self) -> AppResult<HashMap<String, String>> {
        let rows = sqlx::query("SELECT id, city_id FROM restaurants").fetch_all(self.pool()).await.map_err(db_err)?;
        Ok(rows.iter().map(|r| (r.get("id"), r.get("city_id"))).collect())
    }
}
