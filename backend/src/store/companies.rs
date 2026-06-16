use super::*;

impl Store {
    /// Insert a company, returning its new id.
    pub async fn insert_company(&self, c: &NewCompany) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO companies (cik,ticker,name,exchange,sector,industry) \
             VALUES (?,?,?,?,?,?) RETURNING id",
        )
        .bind(&c.cik)
        .bind(&c.ticker)
        .bind(&c.name)
        .bind(&c.exchange)
        .bind(&c.sector)
        .bind(&c.industry)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Insert a company or update it if the ticker already exists. Returns its id.
    /// Idempotent — safe to re-run when bootstrapping the ticker universe.
    pub async fn upsert_company(&self, c: &NewCompany) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO companies (cik,ticker,name,exchange,sector,industry) \
             VALUES (?,?,?,?,?,?) \
             ON CONFLICT(ticker) DO UPDATE SET \
             cik=excluded.cik, name=excluded.name, exchange=excluded.exchange, \
             sector=excluded.sector, industry=excluded.industry \
             RETURNING id",
        )
        .bind(&c.cik)
        .bind(&c.ticker)
        .bind(&c.name)
        .bind(&c.exchange)
        .bind(&c.sector)
        .bind(&c.industry)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Upsert many companies in a single transaction (fast bulk bootstrap).
    pub async fn upsert_companies(&self, companies: &[NewCompany]) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        for c in companies {
            sqlx::query(COMPANY_UPSERT_SQL)
                .bind(&c.cik)
                .bind(&c.ticker)
                .bind(&c.name)
                .bind(&c.exchange)
                .bind(&c.sector)
                .bind(&c.industry)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(companies.len())
    }

    /// Insert or update a market index pseudo-company (`is_index = 1`) with a
    /// synthetic CIK (`IDX-<symbol>`). Indices reuse the prices pipeline but are
    /// hidden from the company directory and movers. Returns its id.
    pub async fn upsert_index(&self, ticker: &str, name: &str) -> Result<i64> {
        let cik = format!("IDX-{}", ticker.trim_start_matches('^'));
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO companies (cik,ticker,name,is_index) VALUES (?,?,?,1) \
             ON CONFLICT(ticker) DO UPDATE SET cik=excluded.cik, name=excluded.name, is_index=1 \
             RETURNING id",
        )
        .bind(cik)
        .bind(ticker)
        .bind(name)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Apply a profile enrichment to a company. Only the `Some` fields overwrite
    /// (COALESCE keeps the existing value where the update is `None`).
    pub async fn update_company_profile(&self, company_id: i64, p: &CompanyProfile) -> Result<()> {
        sqlx::query(
            "UPDATE companies SET \
             sector=COALESCE(?,sector), industry=COALESCE(?,industry), exchange=COALESCE(?,exchange), \
             website=COALESCE(?,website), description=COALESCE(?,description), \
             employees=COALESCE(?,employees) WHERE id=?",
        )
        .bind(&p.sector)
        .bind(&p.industry)
        .bind(&p.exchange)
        .bind(&p.website)
        .bind(&p.description)
        .bind(p.employees)
        .bind(company_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Fetch a company by ticker.
    pub async fn get_company(&self, ticker: &str) -> Result<Option<Company>> {
        let sql = format!("SELECT {SELECT_COMPANY_COLS} FROM companies c WHERE c.ticker=?");
        let row = sqlx::query(&sql).bind(ticker).fetch_optional(&self.pool).await?;
        row.map(|r| company_from_row(&r)).transpose()
    }

    /// List every company, ordered by ticker (for bulk collection).
    pub async fn all_companies(&self) -> Result<Vec<Company>> {
        let sql = format!("SELECT {SELECT_COMPANY_COLS} FROM companies c ORDER BY c.ticker");
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await?;
        rows.iter().map(company_from_row).collect()
    }

    /// Companies due for collection: never collected, or whose last collection
    /// was before `cutoff`. Ordered by ticker.
    pub async fn companies_due(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Company>> {
        let sql = format!(
            "SELECT {SELECT_COMPANY_COLS} \
             FROM companies c LEFT JOIN company_state s ON s.company_id = c.id \
             WHERE s.last_collected_at IS NULL OR s.last_collected_at < ? \
             ORDER BY c.ticker"
        );
        let rows = sqlx::query(&sql).bind(cutoff).fetch_all(&self.pool).await?;
        rows.iter().map(company_from_row).collect()
    }

    /// Record that a company was just collected (upsert).
    pub async fn mark_collected(
        &self,
        company_id: i64,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO company_state (company_id,last_collected_at) VALUES (?,?) \
             ON CONFLICT(company_id) DO UPDATE SET last_collected_at=excluded.last_collected_at",
        )
        .bind(company_id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
