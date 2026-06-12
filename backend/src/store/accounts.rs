use super::*;
use sqlx::Row;

impl Store {
    // --- Auth: users, sessions, watchlists ---

    /// Create a user, returning its id (errors on duplicate email).
    pub async fn create_user(&self, email: &str, password_hash: &str) -> Result<i64> {
        let id: i64 =
            sqlx::query_scalar("INSERT INTO users (email,password_hash) VALUES (?,?) RETURNING id")
                .bind(email)
                .bind(password_hash)
                .fetch_one(&self.pool)
                .await?;
        Ok(id)
    }

    /// `(id, password_hash)` for a login lookup by email.
    pub async fn user_credentials(&self, email: &str) -> Result<Option<(i64, String)>> {
        let row = sqlx::query("SELECT id,password_hash FROM users WHERE email=?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(r) => Ok(Some((r.try_get("id")?, r.try_get("password_hash")?))),
            None => Ok(None),
        }
    }

    /// A user's email by id.
    pub async fn user_email(&self, user_id: i64) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT email FROM users WHERE id=?")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// Persist a session token hash with an expiry.
    pub async fn create_session(
        &self,
        token_hash: &str,
        user_id: i64,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        sqlx::query("INSERT INTO sessions (token_hash,user_id,expires_at) VALUES (?,?,?)")
            .bind(token_hash)
            .bind(user_id)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The user id for a valid (unexpired) session token hash.
    pub async fn session_user(
        &self,
        token_hash: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT user_id FROM sessions WHERE token_hash=? AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Delete a session (logout).
    pub async fn delete_session(&self, token_hash: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash=?")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Add a company to a user's watchlist (idempotent).
    pub async fn add_watch(&self, user_id: i64, company_id: i64) -> Result<()> {
        sqlx::query("INSERT OR IGNORE INTO watchlists (user_id,company_id) VALUES (?,?)")
            .bind(user_id)
            .bind(company_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Remove a company from a user's watchlist.
    pub async fn remove_watch(&self, user_id: i64, company_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM watchlists WHERE user_id=? AND company_id=?")
            .bind(user_id)
            .bind(company_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The companies on a user's watchlist, ordered by ticker.
    pub async fn list_watch(&self, user_id: i64) -> Result<Vec<Company>> {
        let sql = format!(
            "SELECT {SELECT_COMPANY_COLS} \
             FROM watchlists w JOIN companies c ON c.id = w.company_id \
             WHERE w.user_id=? ORDER BY c.ticker"
        );
        let rows = sqlx::query(&sql).bind(user_id).fetch_all(&self.pool).await?;
        rows.iter().map(company_from_row).collect()
    }
}
