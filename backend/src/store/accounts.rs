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

    /// A user's editable profile `(email, display_name)` by id.
    pub async fn user_profile(&self, user_id: i64) -> Result<Option<(String, String)>> {
        let row = sqlx::query("SELECT email,display_name FROM users WHERE id=?")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(r) => Ok(Some((r.try_get("email")?, r.try_get("display_name")?))),
            None => Ok(None),
        }
    }

    /// Update a user's email + display name (errors on duplicate email).
    pub async fn update_profile(
        &self,
        user_id: i64,
        email: &str,
        display_name: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE users SET email=?, display_name=? WHERE id=?")
            .bind(email)
            .bind(display_name)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// A user's stored password hash by id (for change-password verification).
    pub async fn user_password_hash(&self, user_id: i64) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT password_hash FROM users WHERE id=?")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// Replace a user's password hash.
    pub async fn update_password_hash(&self, user_id: i64, password_hash: &str) -> Result<()> {
        sqlx::query("UPDATE users SET password_hash=? WHERE id=?")
            .bind(password_hash)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
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

    // --- Watch groups (tags): a company may belong to many of a user's groups ---

    /// A user's watch groups, ordered by name.
    pub async fn list_groups(&self, user_id: i64) -> Result<Vec<WatchGroup>> {
        let rows = sqlx::query("SELECT id,name FROM watch_groups WHERE user_id=? ORDER BY name")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|r| Ok(WatchGroup { id: r.try_get("id")?, name: r.try_get("name")? }))
            .collect()
    }

    /// Create a named group for a user, returning its id (errors on duplicate name).
    pub async fn create_group(&self, user_id: i64, name: &str) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO watch_groups (user_id,name) VALUES (?,?) RETURNING id",
        )
        .bind(user_id)
        .bind(name)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Rename a user's group (no-op if the group isn't theirs).
    pub async fn rename_group(&self, user_id: i64, group_id: i64, name: &str) -> Result<()> {
        sqlx::query("UPDATE watch_groups SET name=? WHERE id=? AND user_id=?")
            .bind(name)
            .bind(group_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Delete a user's group; its memberships cascade away.
    pub async fn delete_group(&self, user_id: i64, group_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM watch_groups WHERE id=? AND user_id=?")
            .bind(group_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Tag a company into one of the user's groups (idempotent; ownership-checked).
    pub async fn add_to_group(&self, user_id: i64, group_id: i64, company_id: i64) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO watch_group_members (group_id,company_id) \
             SELECT ?,? WHERE EXISTS (SELECT 1 FROM watch_groups WHERE id=? AND user_id=?)",
        )
        .bind(group_id)
        .bind(company_id)
        .bind(group_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Untag a company from one of the user's groups (ownership-checked).
    pub async fn remove_from_group(
        &self,
        user_id: i64,
        group_id: i64,
        company_id: i64,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM watch_group_members WHERE group_id=? AND company_id=? \
             AND EXISTS (SELECT 1 FROM watch_groups WHERE id=? AND user_id=?)",
        )
        .bind(group_id)
        .bind(company_id)
        .bind(group_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// All `(company_id, group_id)` memberships across a user's groups.
    pub async fn watch_group_memberships(&self, user_id: i64) -> Result<Vec<(i64, i64)>> {
        let rows = sqlx::query(
            "SELECT m.company_id, m.group_id FROM watch_group_members m \
             JOIN watch_groups g ON g.id = m.group_id WHERE g.user_id=?",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| Ok((r.try_get("company_id")?, r.try_get("group_id")?)))
            .collect()
    }
}
