use super::*;

impl Store {
    // --- Auth: users, sessions, watchlists ---

    /// Create a user, returning its id (errors on duplicate email).
    pub async fn create_user(&self, email: &str, password_hash: &str) -> Result<i64> {
        let id: i64 =
            query_scalar("INSERT INTO users (email,password_hash) VALUES (?,?) RETURNING id")
                .bind(email)
                .bind(password_hash)
                .fetch_one(&self.db)
                .await?;
        Ok(id)
    }

    /// `(id, password_hash)` for a login lookup by email.
    pub async fn user_credentials(&self, email: &str) -> Result<Option<(i64, String)>> {
        let row = query("SELECT id,password_hash FROM users WHERE email=?")
            .bind(email)
            .fetch_optional(&self.db)
            .await?;
        match row {
            Some(r) => Ok(Some((r.try_get("id")?, r.try_get("password_hash")?))),
            None => Ok(None),
        }
    }

    /// A user's email by id.
    pub async fn user_email(&self, user_id: i64) -> Result<Option<String>> {
        Ok(query_scalar("SELECT email FROM users WHERE id=?")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await?)
    }

    /// A user's editable profile `(email, display_name)` by id.
    pub async fn user_profile(&self, user_id: i64) -> Result<Option<(String, String)>> {
        let row = query("SELECT email,display_name FROM users WHERE id=?")
            .bind(user_id)
            .fetch_optional(&self.db)
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
        query("UPDATE users SET email=?, display_name=? WHERE id=?")
            .bind(email)
            .bind(display_name)
            .bind(user_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// A user's stored password hash by id (for change-password verification).
    pub async fn user_password_hash(&self, user_id: i64) -> Result<Option<String>> {
        Ok(query_scalar("SELECT password_hash FROM users WHERE id=?")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await?)
    }

    /// Replace a user's password hash.
    pub async fn update_password_hash(&self, user_id: i64, password_hash: &str) -> Result<()> {
        query("UPDATE users SET password_hash=? WHERE id=?")
            .bind(password_hash)
            .bind(user_id)
            .execute(&self.db)
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
        query("INSERT INTO sessions (token_hash,user_id,expires_at) VALUES (?,?,?)")
            .bind(token_hash)
            .bind(user_id)
            .bind(expires_at)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// The user id for a valid (unexpired) session token hash.
    pub async fn session_user(
        &self,
        token_hash: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<i64>> {
        Ok(query_scalar(
            "SELECT user_id FROM sessions WHERE token_hash=? AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.db)
        .await?)
    }

    /// Delete a session (logout).
    pub async fn delete_session(&self, token_hash: &str) -> Result<()> {
        query("DELETE FROM sessions WHERE token_hash=?")
            .bind(token_hash)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Add a company to a user's watchlist (idempotent).
    pub async fn add_watch(&self, user_id: i64, company_id: i64) -> Result<()> {
        query("INSERT OR IGNORE INTO watchlists (user_id,company_id) VALUES (?,?)")
            .bind(user_id)
            .bind(company_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Remove a company from a user's watchlist.
    pub async fn remove_watch(&self, user_id: i64, company_id: i64) -> Result<()> {
        query("DELETE FROM watchlists WHERE user_id=? AND company_id=?")
            .bind(user_id)
            .bind(company_id)
            .execute(&self.db)
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
        let rows = query(&sql).bind(user_id).fetch_all(&self.db).await?;
        rows.iter().map(company_from_row).collect()
    }

    // --- Watch groups (tags): a company may belong to many of a user's groups ---

    /// A user's watch groups, ordered by name.
    pub async fn list_groups(&self, user_id: i64) -> Result<Vec<WatchGroup>> {
        let rows = query("SELECT id,name FROM watch_groups WHERE user_id=? ORDER BY name")
            .bind(user_id)
            .fetch_all(&self.db)
            .await?;
        rows.iter()
            .map(|r| Ok(WatchGroup { id: r.try_get("id")?, name: r.try_get("name")? }))
            .collect()
    }

    /// Create a named group for a user, returning its id (errors on duplicate name).
    pub async fn create_group(&self, user_id: i64, name: &str) -> Result<i64> {
        let id: i64 = query_scalar(
            "INSERT INTO watch_groups (user_id,name) VALUES (?,?) RETURNING id",
        )
        .bind(user_id)
        .bind(name)
        .fetch_one(&self.db)
        .await?;
        Ok(id)
    }

    /// Rename a user's group (no-op if the group isn't theirs).
    pub async fn rename_group(&self, user_id: i64, group_id: i64, name: &str) -> Result<()> {
        query("UPDATE watch_groups SET name=? WHERE id=? AND user_id=?")
            .bind(name)
            .bind(group_id)
            .bind(user_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Delete a user's group; its memberships cascade away.
    pub async fn delete_group(&self, user_id: i64, group_id: i64) -> Result<()> {
        query("DELETE FROM watch_groups WHERE id=? AND user_id=?")
            .bind(group_id)
            .bind(user_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Tag a company into one of the user's groups (idempotent; ownership-checked).
    pub async fn add_to_group(&self, user_id: i64, group_id: i64, company_id: i64) -> Result<()> {
        query(
            "INSERT OR IGNORE INTO watch_group_members (group_id,company_id) \
             SELECT ?,? WHERE EXISTS (SELECT 1 FROM watch_groups WHERE id=? AND user_id=?)",
        )
        .bind(group_id)
        .bind(company_id)
        .bind(group_id)
        .bind(user_id)
        .execute(&self.db)
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
        query(
            "DELETE FROM watch_group_members WHERE group_id=? AND company_id=? \
             AND EXISTS (SELECT 1 FROM watch_groups WHERE id=? AND user_id=?)",
        )
        .bind(group_id)
        .bind(company_id)
        .bind(group_id)
        .bind(user_id)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    /// All `(company_id, group_id)` memberships across a user's groups.
    pub async fn watch_group_memberships(&self, user_id: i64) -> Result<Vec<(i64, i64)>> {
        let rows = query(
            "SELECT m.company_id, m.group_id FROM watch_group_members m \
             JOIN watch_groups g ON g.id = m.group_id WHERE g.user_id=?",
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await?;
        rows.iter()
            .map(|r| Ok((r.try_get("company_id")?, r.try_get("group_id")?)))
            .collect()
    }

    // --- Per-user settings (theme + Graham thresholds) ---

    /// A user's saved settings, or defaults when no row / NULL columns exist.
    pub async fn get_settings(&self, user_id: i64) -> Result<UserSettings> {
        let row = query(
            "SELECT theme,graham_min_revenue,pe_max,pb_max,pe_pb_max,current_ratio_min,eps_growth_min \
             FROM user_settings WHERE user_id=?",
        )
        .bind(user_id)
        .fetch_optional(&self.db)
        .await?;
        let d = crate::graham::GrahamConfig::default();
        match row {
            Some(r) => Ok(UserSettings {
                theme: r.try_get("theme")?,
                graham: crate::graham::GrahamConfig {
                    min_revenue: r.try_get::<Option<f64>>("graham_min_revenue")?.unwrap_or(d.min_revenue),
                    pe_max: r.try_get::<Option<f64>>("pe_max")?.unwrap_or(d.pe_max),
                    pb_max: r.try_get::<Option<f64>>("pb_max")?.unwrap_or(d.pb_max),
                    pe_pb_max: r.try_get::<Option<f64>>("pe_pb_max")?.unwrap_or(d.pe_pb_max),
                    current_ratio_min: r
                        .try_get::<Option<f64>>("current_ratio_min")?
                        .unwrap_or(d.current_ratio_min),
                    eps_growth_min: r.try_get::<Option<f64>>("eps_growth_min")?.unwrap_or(d.eps_growth_min),
                },
            }),
            None => Ok(UserSettings::default()),
        }
    }

    /// Insert or replace a user's settings.
    pub async fn save_settings(&self, user_id: i64, s: &UserSettings) -> Result<()> {
        query(
            "INSERT INTO user_settings \
             (user_id,theme,graham_min_revenue,pe_max,pb_max,pe_pb_max,current_ratio_min,eps_growth_min) \
             VALUES (?,?,?,?,?,?,?,?) \
             ON CONFLICT(user_id) DO UPDATE SET theme=excluded.theme, \
             graham_min_revenue=excluded.graham_min_revenue, pe_max=excluded.pe_max, \
             pb_max=excluded.pb_max, pe_pb_max=excluded.pe_pb_max, \
             current_ratio_min=excluded.current_ratio_min, eps_growth_min=excluded.eps_growth_min",
        )
        .bind(user_id)
        .bind(&s.theme)
        .bind(s.graham.min_revenue)
        .bind(s.graham.pe_max)
        .bind(s.graham.pb_max)
        .bind(s.graham.pe_pb_max)
        .bind(s.graham.current_ratio_min)
        .bind(s.graham.eps_growth_min)
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
