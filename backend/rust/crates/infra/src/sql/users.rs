use async_trait::async_trait;
use feast_application::ports::{ExternalIdentity, UserRepository};
use feast_application::AppResult;
use feast_domain::model::{AuthProvider, Role, User, UserStatus};
use sqlx::any::AnyRow;
use sqlx::{AnyConnection, Row};

use super::{db_err, now_iso, SqlStore};

const COLS: &str = "id, name, phone, email, role, status, created_at, password_hash";

fn row_to_user(r: &AnyRow, restaurant_ids: Vec<String>) -> User {
    User {
        id: r.get("id"),
        name: r.get("name"),
        phone: r.get("phone"),
        email: r.get("email"),
        role: Role::parse(r.get::<String, _>("role").as_str()).unwrap_or(Role::Customer),
        restaurant_ids,
        status: if r.get::<String, _>("status") == "BLOCKED" { UserStatus::Blocked } else { UserStatus::Active },
        created_at: r.get("created_at"),
    }
}

impl SqlStore {
    async fn restaurant_ids(&self, user_id: &str) -> AppResult<Vec<String>> {
        let rows = sqlx::query("SELECT restaurant_id FROM user_restaurants WHERE user_id = $1 ORDER BY restaurant_id")
            .bind(user_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        Ok(rows.iter().map(|r| r.get("restaurant_id")).collect())
    }

    async fn load_users(&self, rows: Vec<AnyRow>) -> AppResult<Vec<User>> {
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: String = r.get("id");
            let ids = self.restaurant_ids(&id).await?;
            out.push(row_to_user(&r, ids));
        }
        Ok(out)
    }

    async fn one_user(&self, sql: &str, arg: &str) -> AppResult<Option<(User, Option<String>)>> {
        let Some(r) = sqlx::query(sql).bind(arg).fetch_optional(self.pool()).await.map_err(db_err)? else {
            return Ok(None);
        };
        let id: String = r.get("id");
        let ids = self.restaurant_ids(&id).await?;
        Ok(Some((row_to_user(&r, ids), r.get("password_hash"))))
    }
}

async fn write_links(conn: &mut AnyConnection, user: &User) -> AppResult<()> {
    sqlx::query("DELETE FROM user_restaurants WHERE user_id = $1").bind(&user.id).execute(&mut *conn).await.map_err(db_err)?;
    for rid in &user.restaurant_ids {
        sqlx::query("INSERT INTO user_restaurants (user_id, restaurant_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(&user.id)
            .bind(rid)
            .execute(&mut *conn)
            .await
            .map_err(db_err)?;
    }
    Ok(())
}

#[async_trait]
impl UserRepository for SqlStore {
    async fn get(&self, id: &str) -> AppResult<Option<User>> {
        Ok(self.one_user(&format!("SELECT {COLS} FROM users WHERE id = $1"), id).await?.map(|(u, _)| u))
    }

    async fn find_by_phone(&self, phone: &str) -> AppResult<Option<User>> {
        Ok(self.one_user(&format!("SELECT {COLS} FROM users WHERE phone = $1"), phone).await?.map(|(u, _)| u))
    }

    async fn find_by_email(&self, email: &str) -> AppResult<Option<(User, Option<String>)>> {
        self.one_user(&format!("SELECT {COLS} FROM users WHERE email = $1"), &email.trim().to_lowercase()).await
    }

    async fn list(&self) -> AppResult<Vec<User>> {
        let rows = sqlx::query(&format!("SELECT {COLS} FROM users ORDER BY created_at")).fetch_all(self.pool()).await.map_err(db_err)?;
        self.load_users(rows).await
    }

    async fn owners_of(&self, restaurant_id: &str) -> AppResult<Vec<User>> {
        let rows = sqlx::query(&format!(
            "SELECT {} FROM users u JOIN user_restaurants ur ON ur.user_id = u.id \
             WHERE ur.restaurant_id = $1 AND u.role = 'OWNER' AND u.status = 'ACTIVE'",
            COLS.split(", ").map(|c| format!("u.{c}")).collect::<Vec<_>>().join(", ")
        ))
        .bind(restaurant_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err)?;
        self.load_users(rows).await
    }

    async fn insert(&self, user: &User, password_hash: Option<&str>) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        sqlx::query("INSERT INTO users (id, name, phone, email, role, status, password_hash, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)")
            .bind(&user.id)
            .bind(&user.name)
            .bind(&user.phone)
            .bind(user.email.as_ref().map(|e| e.to_lowercase()))
            .bind(user.role.as_str())
            .bind(if user.is_active() { "ACTIVE" } else { "BLOCKED" })
            .bind(password_hash)
            .bind(&user.created_at)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        write_links(&mut tx, user).await?;
        tx.commit().await.map_err(db_err)
    }

    async fn update(&self, user: &User) -> AppResult<()> {
        let mut tx = self.pool().begin().await.map_err(db_err)?;
        sqlx::query("UPDATE users SET name = $1, phone = $2, email = $3, role = $4, status = $5 WHERE id = $6")
            .bind(&user.name)
            .bind(&user.phone)
            .bind(user.email.as_ref().map(|e| e.to_lowercase()))
            .bind(user.role.as_str())
            .bind(if user.is_active() { "ACTIVE" } else { "BLOCKED" })
            .bind(&user.id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        write_links(&mut tx, user).await?;
        tx.commit().await.map_err(db_err)
    }

    async fn find_by_identity(&self, provider: AuthProvider, subject: &str) -> AppResult<Option<User>> {
        let sql = format!(
            "SELECT {} FROM users u JOIN user_identities i ON i.user_id = u.id WHERE i.provider = $1 AND i.subject = $2",
            COLS.split(", ").map(|c| format!("u.{c}")).collect::<Vec<_>>().join(", ")
        );
        let Some(r) = sqlx::query(&sql).bind(provider.as_str()).bind(subject).fetch_optional(self.pool()).await.map_err(db_err)? else {
            return Ok(None);
        };
        let id: String = r.get("id");
        let ids = self.restaurant_ids(&id).await?;
        Ok(Some(row_to_user(&r, ids)))
    }

    async fn link_identity(&self, user_id: &str, identity: &ExternalIdentity) -> AppResult<()> {
        sqlx::query("INSERT INTO user_identities (provider, subject, user_id, email, created_at) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING")
            .bind(identity.provider.as_str())
            .bind(&identity.subject)
            .bind(user_id)
            .bind(&identity.email)
            .bind(now_iso())
            .execute(self.pool())
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn identities_of(&self, user_id: &str) -> AppResult<Vec<AuthProvider>> {
        let rows = sqlx::query("SELECT provider FROM user_identities WHERE user_id = $1 ORDER BY created_at")
            .bind(user_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        Ok(rows.iter().filter_map(|r| AuthProvider::parse(r.get::<String, _>("provider").as_str())).collect())
    }
}
