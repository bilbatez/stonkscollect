use super::*;
use sqlx::Row;

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

    /// Apply a profile enrichment to a company. Only the `Some` fields overwrite
    /// (COALESCE keeps the existing value where the update is `None`).
    pub async fn update_company_profile(&self, company_id: i64, p: &CompanyProfile) -> Result<()> {
        sqlx::query(
            "UPDATE companies SET \
             sector=COALESCE(?,sector), industry=COALESCE(?,industry), exchange=COALESCE(?,exchange), \
             website=COALESCE(?,website), description=COALESCE(?,description) WHERE id=?",
        )
        .bind(&p.sector)
        .bind(&p.industry)
        .bind(&p.exchange)
        .bind(&p.website)
        .bind(&p.description)
        .bind(company_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Fetch a company by ticker.
    pub async fn get_company(&self, ticker: &str) -> Result<Option<Company>> {
        let row = sqlx::query(
            "SELECT id,cik,ticker,name,exchange,sector,industry,description,website FROM companies WHERE ticker=?",
        )
        .bind(ticker)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some(Company {
                id: r.try_get("id")?,
                cik: r.try_get("cik")?,
                ticker: r.try_get("ticker")?,
                name: r.try_get("name")?,
                exchange: r.try_get("exchange")?,
                sector: r.try_get("sector")?,
                industry: r.try_get("industry")?,
                description: r.try_get("description")?,
                website: r.try_get("website")?,
            })),
            None => Ok(None),
        }
    }

    /// List every company, ordered by ticker (for bulk collection).
    pub async fn all_companies(&self) -> Result<Vec<Company>> {
        let rows = sqlx::query(
            "SELECT id,cik,ticker,name,exchange,sector,industry,description,website FROM companies ORDER BY ticker",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(Company {
                    id: r.try_get("id")?,
                    cik: r.try_get("cik")?,
                    ticker: r.try_get("ticker")?,
                    name: r.try_get("name")?,
                    exchange: r.try_get("exchange")?,
                    sector: r.try_get("sector")?,
                    industry: r.try_get("industry")?,
                    description: r.try_get("description")?,
                    website: r.try_get("website")?,
                })
            })
            .collect()
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
        let rows = sqlx::query(&sql)
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(Company {
                    id: r.try_get("id")?,
                    cik: r.try_get("cik")?,
                    ticker: r.try_get("ticker")?,
                    name: r.try_get("name")?,
                    exchange: r.try_get("exchange")?,
                    sector: r.try_get("sector")?,
                    industry: r.try_get("industry")?,
                    description: r.try_get("description")?,
                    website: r.try_get("website")?,
                })
            })
            .collect()
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
