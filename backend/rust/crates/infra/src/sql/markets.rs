use async_trait::async_trait;
use feast_application::ports::MarketRepository;
use feast_application::AppResult;
use feast_domain::model::{City, Market, MarketStatus};
use sqlx::Row;

use super::{db_err, from_json, now_iso, to_json, SqlStore};

fn status(s: MarketStatus) -> &'static str {
    match s {
        MarketStatus::Live => "LIVE",
        MarketStatus::ComingSoon => "COMING_SOON",
    }
}

#[async_trait]
impl MarketRepository for SqlStore {
    async fn list_markets(&self) -> AppResult<Vec<Market>> {
        let rows = sqlx::query("SELECT data FROM markets ORDER BY country_code").fetch_all(self.pool()).await.map_err(db_err)?;
        rows.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }

    async fn get_market(&self, country_code: &str) -> AppResult<Option<Market>> {
        let row = sqlx::query("SELECT data FROM markets WHERE country_code = $1")
            .bind(country_code.to_uppercase())
            .fetch_optional(self.pool())
            .await
            .map_err(db_err)?;
        row.map(|r| from_json(&r.get::<String, _>("data"))).transpose()
    }

    async fn upsert_market(&self, m: &Market) -> AppResult<()> {
        let mut m = m.clone();
        m.payments.public_key = None; // never persisted
        sqlx::query(
            "INSERT INTO markets (country_code, status, data, updated_at) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (country_code) DO UPDATE SET status = excluded.status, data = excluded.data, updated_at = excluded.updated_at",
        )
        .bind(&m.country_code)
        .bind(status(m.status))
        .bind(to_json(&m)?)
        .bind(now_iso())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn list_cities(&self) -> AppResult<Vec<City>> {
        let rows = sqlx::query("SELECT data FROM cities ORDER BY seq, id").fetch_all(self.pool()).await.map_err(db_err)?;
        rows.iter().map(|r| from_json(&r.get::<String, _>("data"))).collect()
    }

    async fn get_city(&self, id: &str) -> AppResult<Option<City>> {
        let row = sqlx::query("SELECT data FROM cities WHERE id = $1").bind(id).fetch_optional(self.pool()).await.map_err(db_err)?;
        row.map(|r| from_json(&r.get::<String, _>("data"))).transpose()
    }

    async fn upsert_city(&self, c: &City) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO cities (id, country_code, name, data, updated_at, seq) VALUES ($1, $2, $3, $4, $5, (SELECT COALESCE(MAX(seq), 0) + 1 FROM cities)) \
             ON CONFLICT (id) DO UPDATE SET country_code = excluded.country_code, name = excluded.name, data = excluded.data, updated_at = excluded.updated_at",
        )
        .bind(&c.id)
        .bind(&c.country_code)
        .bind(&c.name)
        .bind(to_json(c)?)
        .bind(now_iso())
        .execute(self.pool())
        .await
        .map_err(db_err)?;
        Ok(())
    }
}
