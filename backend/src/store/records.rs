use super::*;
use sqlx::Row;

impl Store {
    /// Insert or update a daily price for `(company_id, date, source)`.
    pub async fn upsert_price(&self, p: &PricePoint) -> Result<()> {
        sqlx::query(PRICE_UPSERT_SQL)
            .bind(p.company_id)
            .bind(p.date)
            .bind(p.open)
            .bind(p.high)
            .bind(p.low)
            .bind(p.close)
            .bind(p.volume)
            .bind(&p.source)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// List all of a company's prices, oldest first.
    pub async fn get_prices(&self, company_id: i64) -> Result<Vec<PricePoint>> {
        self.get_prices_range(company_id, None, None, None).await
    }

    /// List a company's prices, optionally bounded by date range and row limit.
    pub async fn get_prices_range(
        &self,
        company_id: i64,
        from: Option<chrono::NaiveDate>,
        to: Option<chrono::NaiveDate>,
        limit: Option<i64>,
    ) -> Result<Vec<PricePoint>> {
        let mut sql = String::from(
            "SELECT company_id,date,open,high,low,close,volume,source FROM prices WHERE company_id=?",
        );
        if from.is_some() {
            sql.push_str(" AND date >= ?");
        }
        if to.is_some() {
            sql.push_str(" AND date <= ?");
        }
        sql.push_str(" ORDER BY date");
        if limit.is_some() {
            sql.push_str(" LIMIT ?");
        }
        let mut q = sqlx::query(&sql).bind(company_id);
        if let Some(f) = from {
            q = q.bind(f);
        }
        if let Some(t) = to {
            q = q.bind(t);
        }
        if let Some(l) = limit {
            q = q.bind(l);
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|r| {
                Ok(PricePoint {
                    company_id: r.try_get("company_id")?,
                    date: r.try_get("date")?,
                    open: r.try_get("open")?,
                    high: r.try_get("high")?,
                    low: r.try_get("low")?,
                    close: r.try_get("close")?,
                    volume: r.try_get("volume")?,
                    source: r.try_get("source")?,
                })
            })
            .collect()
    }

    /// Upsert many prices in one transaction.
    pub async fn save_prices(&self, prices: &[PricePoint]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for p in prices {
            sqlx::query(PRICE_UPSERT_SQL)
                .bind(p.company_id)
                .bind(p.date)
                .bind(p.open)
                .bind(p.high)
                .bind(p.low)
                .bind(p.close)
                .bind(p.volume)
                .bind(&p.source)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Upsert many share counts in one transaction, keyed by
    /// `(company_id, as_of, source)`.
    pub async fn save_shares(&self, counts: &[ShareCount]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for c in counts {
            sqlx::query(SHARES_UPSERT_SQL)
                .bind(c.company_id)
                .bind(c.as_of)
                .bind(c.shares)
                .bind(&c.source)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The most recent share count for a company, if any was collected.
    pub async fn latest_shares(&self, company_id: i64) -> Result<Option<ShareCount>> {
        let row = sqlx::query(
            "SELECT company_id,as_of,shares,source FROM shares_outstanding \
             WHERE company_id=? ORDER BY as_of DESC LIMIT 1",
        )
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            Ok(ShareCount {
                company_id: r.try_get("company_id")?,
                as_of: r.try_get("as_of")?,
                shares: r.try_get("shares")?,
                source: r.try_get("source")?,
            })
        })
        .transpose()
    }

    /// Insert or update a financial fact (keyed by its natural composite key).
    pub async fn upsert_fact(&self, f: &FinancialFact) -> Result<()> {
        sqlx::query(FACT_UPSERT_SQL)
            .bind(f.company_id)
            .bind(f.statement.as_str())
            .bind(&f.line_item)
            .bind(f.period_type.as_str())
            .bind(f.period_end)
            .bind(f.value)
            .bind(&f.source)
            .bind(f.fetched_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// List all of a company's financial facts.
    pub async fn get_facts(&self, company_id: i64) -> Result<Vec<FinancialFact>> {
        self.get_facts_range(company_id, None, None, None).await
    }

    /// List a company's facts, optionally bounded by period-end range and limit.
    pub async fn get_facts_range(
        &self,
        company_id: i64,
        from: Option<chrono::NaiveDate>,
        to: Option<chrono::NaiveDate>,
        limit: Option<i64>,
    ) -> Result<Vec<FinancialFact>> {
        let mut sql = String::from(
            "SELECT company_id,statement,line_item,period_type,period_end,value,source,fetched_at \
             FROM financial_facts WHERE company_id=?",
        );
        if from.is_some() {
            sql.push_str(" AND period_end >= ?");
        }
        if to.is_some() {
            sql.push_str(" AND period_end <= ?");
        }
        sql.push_str(" ORDER BY period_end");
        if limit.is_some() {
            sql.push_str(" LIMIT ?");
        }
        let mut q = sqlx::query(&sql).bind(company_id);
        if let Some(f) = from {
            q = q.bind(f);
        }
        if let Some(t) = to {
            q = q.bind(t);
        }
        if let Some(l) = limit {
            q = q.bind(l);
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|r| {
                let statement_token: String = r.try_get("statement")?;
                let statement = StatementKind::parse(&statement_token)
                    .ok_or_else(|| StoreError::Decode(format!("statement: {statement_token}")))?;
                let pt_token: String = r.try_get("period_type")?;
                let period_type = PeriodType::parse(&pt_token)
                    .ok_or_else(|| StoreError::Decode(format!("period_type: {pt_token}")))?;
                Ok(FinancialFact {
                    company_id: r.try_get("company_id")?,
                    statement,
                    line_item: r.try_get("line_item")?,
                    period_type,
                    period_end: r.try_get("period_end")?,
                    value: r.try_get("value")?,
                    source: r.try_get("source")?,
                    fetched_at: r.try_get("fetched_at")?,
                })
            })
            .collect()
    }

    /// Insert a news item, ignoring duplicates by `dedup_hash`.
    /// Returns `true` if a new row was inserted.
    pub async fn insert_news(&self, n: &NewsItem) -> Result<bool> {
        let res = sqlx::query(NEWS_INSERT_SQL)
            .bind(n.company_id)
            .bind(&n.title)
            .bind(&n.description)
            .bind(&n.url)
            .bind(&n.source)
            .bind(n.published_at)
            .bind(&n.dedup_hash)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Insert many news items in one transaction, ignoring duplicates.
    pub async fn save_news(&self, items: &[NewsItem]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for n in items {
            sqlx::query(NEWS_INSERT_SQL)
                .bind(n.company_id)
                .bind(&n.title)
                .bind(&n.description)
                .bind(&n.url)
                .bind(&n.source)
                .bind(n.published_at)
                .bind(&n.dedup_hash)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// List a company's news, newest first.
    pub async fn get_news(&self, company_id: i64) -> Result<Vec<NewsItem>> {
        let rows = sqlx::query(
            "SELECT company_id,title,description,url,source,published_at,dedup_hash \
             FROM news WHERE company_id=? ORDER BY published_at DESC",
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(NewsItem {
                    company_id: r.try_get("company_id")?,
                    title: r.try_get("title")?,
                    description: r.try_get("description")?,
                    url: r.try_get("url")?,
                    source: r.try_get("source")?,
                    published_at: r.try_get("published_at")?,
                    dedup_hash: r.try_get("dedup_hash")?,
                })
            })
            .collect()
    }

    /// Insert or update a derived ratio, keyed by (company, period, metric).
    pub async fn upsert_ratio(&self, r: &Ratio) -> Result<()> {
        bind_ratio(sqlx::query(RATIO_UPSERT_SQL), r).execute(&self.pool).await?;
        Ok(())
    }

    /// Upsert many ratios in one transaction.
    pub async fn save_ratios(&self, ratios: &[Ratio]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for r in ratios {
            bind_ratio(sqlx::query(RATIO_UPSERT_SQL), r).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// List a company's ratios ordered by period then metric. When `period` is
    /// given, only that period type (annual/quarterly) is returned.
    pub async fn get_ratios(
        &self,
        company_id: i64,
        period: Option<PeriodType>,
    ) -> Result<Vec<Ratio>> {
        let mut sql = String::from(
            "SELECT company_id,period_end,period_type,metric,value,computed_at FROM ratios \
             WHERE company_id=?",
        );
        if period.is_some() {
            sql.push_str(" AND period_type=?");
        }
        sql.push_str(" ORDER BY period_end, metric");
        let mut q = sqlx::query(&sql).bind(company_id);
        if let Some(p) = period {
            q = q.bind(p.as_str());
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter().map(|r| ratio_from_row(&r)).collect()
    }

    /// Insert a flagged discrepancy, returning its new id.
    pub async fn insert_discrepancy(&self, d: &Discrepancy) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO discrepancies \
             (company_id,field,period,source_a,value_a,source_b,value_b,pct_diff,flagged_at) \
             VALUES (?,?,?,?,?,?,?,?,?) RETURNING id",
        )
        .bind(d.company_id)
        .bind(&d.field)
        .bind(&d.period)
        .bind(&d.source_a)
        .bind(d.value_a)
        .bind(&d.source_b)
        .bind(d.value_b)
        .bind(d.pct_diff)
        .bind(d.flagged_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Persist reconciled facts + discrepancies in a single transaction.
    /// One commit per call keeps bulk collection fast and the WAL bounded.
    pub async fn save_reconciled(
        &self,
        facts: &[FinancialFact],
        discrepancies: &[Discrepancy],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for f in facts {
            sqlx::query(FACT_UPSERT_SQL)
                .bind(f.company_id)
                .bind(f.statement.as_str())
                .bind(&f.line_item)
                .bind(f.period_type.as_str())
                .bind(f.period_end)
                .bind(f.value)
                .bind(&f.source)
                .bind(f.fetched_at)
                .execute(&mut *tx)
                .await?;
        }
        for d in discrepancies {
            sqlx::query(DISCREPANCY_INSERT_SQL)
                .bind(d.company_id)
                .bind(&d.field)
                .bind(&d.period)
                .bind(&d.source_a)
                .bind(d.value_a)
                .bind(&d.source_b)
                .bind(d.value_b)
                .bind(d.pct_diff)
                .bind(d.flagged_at)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// List a company's flagged discrepancies.
    pub async fn get_discrepancies(&self, company_id: i64) -> Result<Vec<Discrepancy>> {
        let rows = sqlx::query(
            "SELECT company_id,field,period,source_a,value_a,source_b,value_b,pct_diff,flagged_at \
             FROM discrepancies WHERE company_id=? ORDER BY id",
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(Discrepancy {
                    company_id: r.try_get("company_id")?,
                    field: r.try_get("field")?,
                    period: r.try_get("period")?,
                    source_a: r.try_get("source_a")?,
                    value_a: r.try_get("value_a")?,
                    source_b: r.try_get("source_b")?,
                    value_b: r.try_get("value_b")?,
                    pct_diff: r.try_get("pct_diff")?,
                    flagged_at: r.try_get("flagged_at")?,
                })
            })
            .collect()
    }
}
