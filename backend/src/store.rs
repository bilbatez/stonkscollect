//! SQLite-backed persistence + Parquet export. All SQL lives here.

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{ArrayRef, Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::{Row, SqlitePool};

use crate::domain::{
    CollectionRun, Company, Discrepancy, FinancialFact, NewCompany, NewsItem, PeriodType,
    PricePoint, Ratio, StatementKind,
};

/// Errors returned by the store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invalid stored value: {0}")]
    Decode(String),
    #[error("{0}")]
    Other(String),
}

/// Wrap any displayable error as `StoreError::Other`.
fn other<E: std::fmt::Display>(e: E) -> StoreError {
    StoreError::Other(e.to_string())
}

type Result<T> = std::result::Result<T, StoreError>;

// Shared write SQL, reused by single-row and batched (transactional) methods.
const FACT_UPSERT_SQL: &str = "INSERT INTO financial_facts \
     (company_id,statement,line_item,period_type,period_end,value,source,fetched_at) \
     VALUES (?,?,?,?,?,?,?,?) \
     ON CONFLICT(company_id,statement,line_item,period_type,period_end,source) \
     DO UPDATE SET value=excluded.value, fetched_at=excluded.fetched_at";

const DISCREPANCY_INSERT_SQL: &str = "INSERT INTO discrepancies \
     (company_id,field,period,source_a,value_a,source_b,value_b,pct_diff,flagged_at) \
     VALUES (?,?,?,?,?,?,?,?,?)";

const COMPANY_UPSERT_SQL: &str = "INSERT INTO companies (cik,ticker,name,exchange,sector,industry) \
     VALUES (?,?,?,?,?,?) \
     ON CONFLICT(ticker) DO UPDATE SET \
     cik=excluded.cik, name=excluded.name, exchange=excluded.exchange, \
     sector=excluded.sector, industry=excluded.industry";

const PRICE_UPSERT_SQL: &str = "INSERT INTO prices (company_id,date,close,volume,source) \
     VALUES (?,?,?,?,?) \
     ON CONFLICT(company_id,date,source) DO UPDATE SET close=excluded.close, volume=excluded.volume";

const NEWS_INSERT_SQL: &str = "INSERT OR IGNORE INTO news \
     (company_id,title,description,url,source,published_at,dedup_hash) VALUES (?,?,?,?,?,?,?)";

const RATIO_UPSERT_SQL: &str = "INSERT INTO ratios (company_id,period_end,metric,value,computed_at) \
     VALUES (?,?,?,?,?) \
     ON CONFLICT(company_id,period_end,metric) DO UPDATE SET value=excluded.value, computed_at=excluded.computed_at";

/// SQLite-backed data store.
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Open (creating if needed) the database at `url` and apply migrations.
    ///
    /// WAL + a busy timeout let the scheduler's concurrent collectors write
    /// without hitting "database is locked"; foreign keys are enforced.
    pub async fn connect(url: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .foreign_keys(true);
        // WAL lets the read-only API run concurrently with writers. The pool is
        // sized above the default collection concurrency so parallel ingest
        // never waits on a connection; acquire_timeout still bounds any wait.
        let pool = SqlitePoolOptions::new()
            .max_connections(16)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await.map_err(other)?;
        Ok(Self { pool })
    }

    /// Close the underlying connection pool. After this, queries error
    /// (used to exercise best-effort failure paths).
    pub async fn close(&self) {
        self.pool.close().await;
    }

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

    /// Fetch a company by ticker.
    pub async fn get_company(&self, ticker: &str) -> Result<Option<Company>> {
        let row = sqlx::query(
            "SELECT id,cik,ticker,name,exchange,sector,industry FROM companies WHERE ticker=?",
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
            })),
            None => Ok(None),
        }
    }

    /// List every company, ordered by ticker (for bulk collection).
    pub async fn all_companies(&self) -> Result<Vec<Company>> {
        let rows = sqlx::query(
            "SELECT id,cik,ticker,name,exchange,sector,industry FROM companies ORDER BY ticker",
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
        let rows = sqlx::query(
            "SELECT c.id,c.cik,c.ticker,c.name,c.exchange,c.sector,c.industry \
             FROM companies c LEFT JOIN company_state s ON s.company_id = c.id \
             WHERE s.last_collected_at IS NULL OR s.last_collected_at < ? \
             ORDER BY c.ticker",
        )
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

    /// Insert or update a daily price for `(company_id, date, source)`.
    pub async fn upsert_price(&self, p: &PricePoint) -> Result<()> {
        sqlx::query(PRICE_UPSERT_SQL)
            .bind(p.company_id)
            .bind(p.date)
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
        let mut sql =
            String::from("SELECT company_id,date,close,volume,source FROM prices WHERE company_id=?");
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
                .bind(p.close)
                .bind(p.volume)
                .bind(&p.source)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
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
        sqlx::query(RATIO_UPSERT_SQL)
            .bind(r.company_id)
            .bind(r.period_end)
            .bind(&r.metric)
            .bind(r.value)
            .bind(r.computed_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Upsert many ratios in one transaction.
    pub async fn save_ratios(&self, ratios: &[Ratio]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for r in ratios {
            sqlx::query(RATIO_UPSERT_SQL)
                .bind(r.company_id)
                .bind(r.period_end)
                .bind(&r.metric)
                .bind(r.value)
                .bind(r.computed_at)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// List a company's ratios ordered by period then metric.
    pub async fn get_ratios(&self, company_id: i64) -> Result<Vec<Ratio>> {
        let rows = sqlx::query(
            "SELECT company_id,period_end,metric,value,computed_at FROM ratios \
             WHERE company_id=? ORDER BY period_end, metric",
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(Ratio {
                    company_id: r.try_get("company_id")?,
                    period_end: r.try_get("period_end")?,
                    metric: r.try_get("metric")?,
                    value: r.try_get("value")?,
                    computed_at: r.try_get("computed_at")?,
                })
            })
            .collect()
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

    /// Record the start of a collection run, returning its id.
    pub async fn start_run(
        &self,
        source: &str,
        scope: Option<&str>,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO collection_runs (source,scope,started_at,status) VALUES (?,?,?,'running') RETURNING id",
        )
        .bind(source)
        .bind(scope)
        .bind(started_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Mark a collection run finished with a final status and optional error.
    pub async fn finish_run(
        &self,
        id: i64,
        status: &str,
        finished_at: chrono::DateTime<chrono::Utc>,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE collection_runs SET status=?, finished_at=?, error=? WHERE id=?",
        )
        .bind(status)
        .bind(finished_at)
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// List the most recent collection runs, newest first.
    pub async fn recent_runs(&self, limit: i64) -> Result<Vec<CollectionRun>> {
        let rows = sqlx::query(
            "SELECT id,source,scope,started_at,finished_at,status,error FROM collection_runs \
             ORDER BY id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(CollectionRun {
                    id: r.try_get("id")?,
                    source: r.try_get("source")?,
                    scope: r.try_get("scope")?,
                    started_at: r.try_get("started_at")?,
                    finished_at: r.try_get("finished_at")?,
                    status: r.try_get("status")?,
                    error: r.try_get("error")?,
                })
            })
            .collect()
    }

    /// Export a company's prices to a Parquet file for portable archiving.
    pub async fn export_prices_parquet(&self, company_id: i64, path: &Path) -> Result<()> {
        let prices = self.get_prices(company_id).await?;
        let dates = StringArray::from_iter_values(prices.iter().map(|p| p.date.to_string()));
        let closes = Float64Array::from(prices.iter().map(|p| p.close).collect::<Vec<_>>());
        let volumes = Int64Array::from(prices.iter().map(|p| p.volume).collect::<Vec<_>>());
        let sources = StringArray::from_iter_values(prices.iter().map(|p| p.source.as_str()));
        let schema = Arc::new(Schema::new(vec![
            Field::new("date", DataType::Utf8, false),
            Field::new("close", DataType::Float64, false),
            Field::new("volume", DataType::Int64, true),
            Field::new("source", DataType::Utf8, false),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(dates) as ArrayRef,
                Arc::new(closes) as ArrayRef,
                Arc::new(volumes) as ArrayRef,
                Arc::new(sources) as ArrayRef,
            ],
        )
        .map_err(other)?;
        let file = std::fs::File::create(path).map_err(other)?;
        let mut writer = ArrowWriter::try_new(file, schema, None).map_err(other)?;
        writer.write(&batch).map_err(other)?;
        writer.close().map_err(other)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::*;
    use crate::testutil::temp_store;
    use chrono::{NaiveDate, TimeZone, Utc};
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    fn sample_company() -> NewCompany {
        NewCompany {
            cik: "0000320193".into(),
            ticker: "AAPL".into(),
            name: "Apple Inc.".into(),
            exchange: Some("NASDAQ".into()),
            sector: Some("Technology".into()),
            industry: None,
        }
    }

    #[tokio::test]
    async fn connect_runs_migrations_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join("test.db").display());
        Store::connect(&url).await.unwrap();
        // Second connect re-validates already-applied migrations without error.
        Store::connect(&url).await.unwrap();
    }

    #[tokio::test]
    async fn insert_and_get_company_round_trips() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let got = store.get_company("AAPL").await.unwrap().unwrap();
        assert_eq!(got.id, id);
        assert_eq!(got.cik, "0000320193");
        assert_eq!(got.exchange, Some("NASDAQ".into()));
        assert_eq!(got.industry, None);
        // exercise derives
        assert_eq!(got.clone(), got);
        assert!(format!("{got:?}").contains("AAPL"));
    }

    #[tokio::test]
    async fn upsert_companies_batch_inserts_in_one_tx() {
        let (store, _d) = temp_store().await;
        let mut msft = sample_company();
        msft.ticker = "MSFT".into();
        let n = store
            .upsert_companies(&[sample_company(), msft])
            .await
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(store.all_companies().await.unwrap().len(), 2);
        // idempotent
        store.upsert_companies(&[sample_company()]).await.unwrap();
        assert_eq!(store.all_companies().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn get_prices_range_filters_by_date_and_limit() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        for d in [
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
        ] {
            store
                .upsert_price(&PricePoint { company_id: id, date: d, close: 1.0, volume: None, source: "fmp".into() })
                .await
                .unwrap();
        }
        let from = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        let to = NaiveDate::from_ymd_opt(2024, 2, 28).unwrap();
        // bounded both sides -> only Feb
        let r = store.get_prices_range(id, Some(from), Some(to), None).await.unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].date, from);
        // limit only
        assert_eq!(store.get_prices_range(id, None, None, Some(2)).await.unwrap().len(), 2);
        // no bounds == get_prices
        assert_eq!(store.get_prices(id).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn get_facts_range_filters_by_period_and_limit() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        for y in [2021, 2022, 2023] {
            store
                .upsert_fact(&FinancialFact {
                    company_id: id,
                    statement: StatementKind::Income,
                    line_item: "Revenue".into(),
                    period_type: PeriodType::Annual,
                    period_end: NaiveDate::from_ymd_opt(y, 12, 31).unwrap(),
                    value: 1.0,
                    source: "edgar".into(),
                    fetched_at: now,
                })
                .await
                .unwrap();
        }
        let from = NaiveDate::from_ymd_opt(2022, 1, 1).unwrap();
        let r = store.get_facts_range(id, Some(from), None, Some(1)).await.unwrap();
        assert_eq!(r.len(), 1); // 2022 (earliest >= from, limited to 1)
        assert_eq!(r[0].period_end, NaiveDate::from_ymd_opt(2022, 12, 31).unwrap());
        let to = NaiveDate::from_ymd_opt(2022, 12, 31).unwrap();
        assert_eq!(store.get_facts_range(id, None, Some(to), None).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn save_prices_news_ratios_batch_persist() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let d = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        store
            .save_prices(&[PricePoint { company_id: id, date: d, close: 1.0, volume: None, source: "fmp".into() }])
            .await
            .unwrap();
        store
            .save_news(&[NewsItem {
                company_id: id,
                title: "Hi".into(),
                description: None,
                url: "u".into(),
                source: "rss".into(),
                published_at: now,
                dedup_hash: "h".into(),
            }])
            .await
            .unwrap();
        store
            .save_ratios(&[Ratio {
                company_id: id,
                period_end: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                metric: "net_margin".into(),
                value: 0.25,
                computed_at: now,
            }])
            .await
            .unwrap();
        assert_eq!(store.get_prices(id).await.unwrap().len(), 1);
        assert_eq!(store.get_news(id).await.unwrap().len(), 1);
        assert_eq!(store.get_ratios(id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn save_reconciled_persists_facts_and_discrepancies_in_one_tx() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let fact = FinancialFact {
            company_id: id,
            statement: StatementKind::Income,
            line_item: "Revenue".into(),
            period_type: PeriodType::Annual,
            period_end: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            value: 100.0,
            source: "edgar".into(),
            fetched_at: now,
        };
        let disc = Discrepancy {
            company_id: id,
            field: "Revenue".into(),
            period: None,
            source_a: "edgar".into(),
            value_a: 100.0,
            source_b: "fmp".into(),
            value_b: 130.0,
            pct_diff: 0.3,
            flagged_at: now,
        };
        store.save_reconciled(&[fact], &[disc]).await.unwrap();
        assert_eq!(store.get_facts(id).await.unwrap().len(), 1);
        assert_eq!(store.get_discrepancies(id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn upsert_company_is_idempotent_and_updates() {
        let (store, _d) = temp_store().await;
        let id1 = store.upsert_company(&sample_company()).await.unwrap();
        let mut updated = sample_company();
        updated.name = "Apple (renamed)".into();
        let id2 = store.upsert_company(&updated).await.unwrap();
        assert_eq!(id1, id2); // same row
        let got = store.get_company("AAPL").await.unwrap().unwrap();
        assert_eq!(got.name, "Apple (renamed)");
    }

    #[tokio::test]
    async fn companies_due_respects_last_collected() {
        let (store, _d) = temp_store().await;
        let id = store.upsert_company(&sample_company()).await.unwrap();
        let t = |h| Utc.with_ymd_and_hms(2024, 1, 1, h, 0, 0).unwrap();

        // never collected -> due
        assert_eq!(store.companies_due(t(12)).await.unwrap().len(), 1);
        // collected at 10:00; cutoff 09:00 -> not due (collected after cutoff)
        store.mark_collected(id, t(10)).await.unwrap();
        assert!(store.companies_due(t(9)).await.unwrap().is_empty());
        // cutoff 11:00 -> due again (collected before cutoff)
        assert_eq!(store.companies_due(t(11)).await.unwrap().len(), 1);
        // re-mark updates in place (still one company)
        store.mark_collected(id, t(20)).await.unwrap();
        assert!(store.companies_due(t(11)).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn all_companies_lists_every_company_ordered() {
        let (store, _d) = temp_store().await;
        store.upsert_company(&sample_company()).await.unwrap();
        let mut msft = sample_company();
        msft.ticker = "MSFT".into();
        store.upsert_company(&msft).await.unwrap();
        let all = store.all_companies().await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].ticker, "AAPL"); // ordered by ticker
        assert_eq!(all[1].ticker, "MSFT");
    }

    #[tokio::test]
    async fn get_company_unknown_returns_none() {
        let (store, _d) = temp_store().await;
        assert_eq!(store.get_company("NOPE").await.unwrap(), None);
    }

    #[tokio::test]
    async fn duplicate_ticker_yields_db_error() {
        let (store, _d) = temp_store().await;
        store.insert_company(&sample_company()).await.unwrap();
        let err = store.insert_company(&sample_company()).await.unwrap_err();
        assert!(matches!(err, StoreError::Db(_)));
    }

    #[tokio::test]
    async fn upsert_price_inserts_then_updates_and_lists_ordered() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let d1 = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
        store
            .upsert_price(&PricePoint { company_id: id, date: d2, close: 10.0, volume: Some(5), source: "fmp".into() })
            .await
            .unwrap();
        store
            .upsert_price(&PricePoint { company_id: id, date: d1, close: 9.0, volume: None, source: "fmp".into() })
            .await
            .unwrap();
        // update existing (d2, fmp)
        store
            .upsert_price(&PricePoint { company_id: id, date: d2, close: 11.0, volume: Some(7), source: "fmp".into() })
            .await
            .unwrap();
        let prices = store.get_prices(id).await.unwrap();
        assert_eq!(prices.len(), 2);
        assert_eq!(prices[0].date, d1);
        assert_eq!(prices[1].close, 11.0);
        assert_eq!(prices[1].volume, Some(7));
        let p = prices[1].clone();
        assert_eq!(p, prices[1]);
        assert!(format!("{p:?}").contains("fmp"));
    }

    #[tokio::test]
    async fn upsert_fact_round_trips_enums() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let fact = FinancialFact {
            company_id: id,
            statement: StatementKind::Income,
            line_item: "Revenue".into(),
            period_type: PeriodType::Annual,
            period_end: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            value: 383_285_000_000.0,
            source: "edgar".into(),
            fetched_at: Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
        };
        store.upsert_fact(&fact).await.unwrap();
        // update same key
        let mut updated = fact.clone();
        updated.value = 400.0;
        store.upsert_fact(&updated).await.unwrap();
        let facts = store.get_facts(id).await.unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].value, 400.0);
        assert_eq!(facts[0].statement, StatementKind::Income);
        assert_eq!(facts[0].period_type, PeriodType::Annual);
        assert!(format!("{:?}", facts[0]).contains("Revenue"));
    }

    #[tokio::test]
    async fn get_facts_errors_on_bad_statement() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        sqlx::query(
            "INSERT INTO financial_facts (company_id,statement,line_item,period_type,period_end,value,source,fetched_at) VALUES (?,?,?,?,?,?,?,?)",
        )
        .bind(id).bind("bogus").bind("X").bind("annual").bind("2023-12-31").bind(1.0).bind("edgar").bind("2024-01-01T00:00:00Z")
        .execute(&store.pool).await.unwrap();
        let err = store.get_facts(id).await.unwrap_err();
        assert!(matches!(err, StoreError::Decode(_)));
    }

    #[tokio::test]
    async fn get_facts_errors_on_bad_period_type() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        sqlx::query(
            "INSERT INTO financial_facts (company_id,statement,line_item,period_type,period_end,value,source,fetched_at) VALUES (?,?,?,?,?,?,?,?)",
        )
        .bind(id).bind("income").bind("X").bind("weekly").bind("2023-12-31").bind(1.0).bind("edgar").bind("2024-01-01T00:00:00Z")
        .execute(&store.pool).await.unwrap();
        let err = store.get_facts(id).await.unwrap_err();
        assert!(matches!(err, StoreError::Decode(_)));
    }

    #[tokio::test]
    async fn insert_news_dedups_and_lists_newest_first() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let older = NewsItem {
            company_id: id,
            title: "Old".into(),
            description: Some("d".into()),
            url: "http://a".into(),
            source: "reuters".into(),
            published_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            dedup_hash: "h1".into(),
        };
        let newer = NewsItem {
            company_id: id,
            title: "New".into(),
            description: None,
            url: "http://b".into(),
            source: "ap".into(),
            published_at: Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap(),
            dedup_hash: "h2".into(),
        };
        assert!(store.insert_news(&older).await.unwrap());
        assert!(store.insert_news(&newer).await.unwrap());
        // duplicate hash ignored
        assert!(!store.insert_news(&older).await.unwrap());
        let news = store.get_news(id).await.unwrap();
        assert_eq!(news.len(), 2);
        assert_eq!(news[0].title, "New");
        assert_eq!(news[1].description, Some("d".into()));
        assert!(format!("{:?}", news[0].clone()).contains("New"));
    }

    #[tokio::test]
    async fn upsert_ratio_inserts_then_updates_and_lists() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let pe = NaiveDate::from_ymd_opt(2023, 12, 31).unwrap();
        store
            .upsert_ratio(&Ratio {
                company_id: id,
                period_end: pe,
                metric: "pe".into(),
                value: 28.5,
                computed_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            })
            .await
            .unwrap();
        // same (company, period, metric) updates
        store
            .upsert_ratio(&Ratio {
                company_id: id,
                period_end: pe,
                metric: "pe".into(),
                value: 30.0,
                computed_at: Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
            })
            .await
            .unwrap();
        let ratios = store.get_ratios(id).await.unwrap();
        assert_eq!(ratios.len(), 1);
        assert_eq!(ratios[0].value, 30.0);
        assert_eq!(ratios[0].metric, "pe");
    }

    #[tokio::test]
    async fn insert_and_list_discrepancies() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let disc = Discrepancy {
            company_id: id,
            field: "Revenue".into(),
            period: Some("2023-12-31".into()),
            source_a: "edgar".into(),
            value_a: 100.0,
            source_b: "fmp".into(),
            value_b: 110.0,
            pct_diff: 0.1,
            flagged_at: Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
        };
        let did = store.insert_discrepancy(&disc).await.unwrap();
        assert!(did > 0);
        let list = store.get_discrepancies(id).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].pct_diff, 0.1);
        assert_eq!(list[0].period, Some("2023-12-31".into()));
        assert!(format!("{:?}", list[0].clone()).contains("Revenue"));
    }

    #[tokio::test]
    async fn collection_runs_lifecycle_and_recent_order() {
        let (store, _d) = temp_store().await;
        let t = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let id1 = store.start_run("edgar", Some("AAPL"), t).await.unwrap();
        store.finish_run(id1, "ok", t, None).await.unwrap();
        let id2 = store.start_run("fmp", None, t).await.unwrap();
        store.finish_run(id2, "error", t, Some("boom")).await.unwrap();

        let runs = store.recent_runs(10).await.unwrap();
        assert_eq!(runs.len(), 2);
        // newest (highest id) first
        assert_eq!(runs[0].id, id2);
        assert_eq!(runs[0].source, "fmp");
        assert_eq!(runs[0].scope, None);
        assert_eq!(runs[0].status, "error");
        assert_eq!(runs[0].error, Some("boom".into()));
        assert_eq!(runs[0].finished_at, Some(t));
        assert_eq!(runs[1].id, id1);
        assert_eq!(runs[1].scope, Some("AAPL".into()));
        assert_eq!(runs[1].status, "ok");
        assert_eq!(runs[1].error, None);
        assert_eq!(runs[1].clone(), runs[1]);
        assert!(format!("{:?}", runs[0]).contains("fmp"));
    }

    #[tokio::test]
    async fn export_prices_parquet_round_trips() {
        let (store, dir) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        store
            .upsert_price(&PricePoint { company_id: id, date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(), close: 9.5, volume: Some(3), source: "fmp".into() })
            .await
            .unwrap();
        store
            .upsert_price(&PricePoint { company_id: id, date: NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(), close: 10.5, volume: None, source: "fmp".into() })
            .await
            .unwrap();
        let path = dir.path().join("prices.parquet");
        store.export_prices_parquet(id, &path).await.unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let mut reader = ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();
        let batch = reader.next().unwrap().unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 4);
    }

    #[tokio::test]
    async fn export_prices_parquet_bad_path_errors() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let bad = std::path::Path::new("/no/such/dir/prices.parquet");
        let err = store.export_prices_parquet(id, bad).await.unwrap_err();
        assert!(matches!(err, StoreError::Other(_)));
    }

    #[test]
    fn store_error_display_covers_variants() {
        assert!(StoreError::Db(sqlx::Error::RowNotFound).to_string().contains("database"));
        assert!(StoreError::Decode("x".into()).to_string().contains("x"));
        assert!(StoreError::Other("boom".into()).to_string().contains("boom"));
    }
}
