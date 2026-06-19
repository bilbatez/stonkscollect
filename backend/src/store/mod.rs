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

use crate::domain::*;
use crate::net::{LoginThrottle, Validators, LOGIN_MAX_ATTEMPTS, LOGIN_WINDOW};

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

type SqliteQuery<'q> = sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>;

/// Bind a [`Ratio`]'s columns onto a ratios upsert query (order matches `RATIO_UPSERT_SQL`).
fn bind_ratio<'q>(q: SqliteQuery<'q>, r: &'q Ratio) -> SqliteQuery<'q> {
    q.bind(r.company_id)
        .bind(r.period_end)
        .bind(r.period_type.as_str())
        .bind(&r.metric)
        .bind(r.value)
        .bind(r.computed_at)
}

/// Decode the `companies` columns of a row into a [`Company`].
fn company_from_row(r: &sqlx::sqlite::SqliteRow) -> Result<Company> {
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
        employees: r.try_get("employees")?,
        status: r.try_get("status")?,
    })
}

/// Decode a ratios row (incl. its period_type token) into a [`Ratio`].
fn ratio_from_row(r: &sqlx::sqlite::SqliteRow) -> Result<Ratio> {
    let pt: String = r.try_get("period_type")?;
    Ok(Ratio {
        company_id: r.try_get("company_id")?,
        period_end: r.try_get("period_end")?,
        period_type: PeriodType::parse(&pt)
            .ok_or_else(|| StoreError::Decode(format!("bad period_type: {pt}")))?,
        metric: r.try_get("metric")?,
        value: r.try_get("value")?,
        computed_at: r.try_get("computed_at")?,
    })
}

// Shared write SQL, reused by single-row and batched (transactional) methods.
const FACT_UPSERT_SQL: &str = "INSERT INTO financial_facts \
     (company_id,statement,line_item,period_type,period_end,value,source,fetched_at) \
     VALUES (?,?,?,?,?,?,?,?) \
     ON CONFLICT(company_id,statement,line_item,period_type,period_end,source) \
     DO UPDATE SET value=excluded.value, fetched_at=excluded.fetched_at";

// `period` binds as '' for None: the unique key needs a NOT NULL period
// (SQLite treats NULLs as distinct), and re-flagging updates in place.
const DISCREPANCY_UPSERT_SQL: &str = "INSERT INTO discrepancies \
     (company_id,field,period,source_a,value_a,source_b,value_b,pct_diff,flagged_at) \
     VALUES (?,?,?,?,?,?,?,?,?) \
     ON CONFLICT(company_id,field,period,source_a,source_b) DO UPDATE SET \
     value_a=excluded.value_a, value_b=excluded.value_b, \
     pct_diff=excluded.pct_diff, flagged_at=excluded.flagged_at";

const COMPANY_UPSERT_SQL: &str = "INSERT INTO companies (cik,ticker,name,exchange,sector,industry) \
     VALUES (?,?,?,?,?,?) \
     ON CONFLICT(ticker) DO UPDATE SET \
     cik=excluded.cik, name=excluded.name, exchange=excluded.exchange, \
     sector=excluded.sector, industry=excluded.industry";

const PRICE_UPSERT_SQL: &str = "INSERT INTO prices (company_id,date,open,high,low,close,volume,source) \
     VALUES (?,?,?,?,?,?,?,?) \
     ON CONFLICT(company_id,date,source) DO UPDATE SET \
     open=excluded.open, high=excluded.high, low=excluded.low, close=excluded.close, volume=excluded.volume";

const NEWS_INSERT_SQL: &str = "INSERT OR IGNORE INTO news \
     (company_id,title,description,url,source,published_at,dedup_hash) VALUES (?,?,?,?,?,?,?)";

const SHARES_UPSERT_SQL: &str = "INSERT INTO shares_outstanding (company_id,as_of,shares,source) \
     VALUES (?,?,?,?) \
     ON CONFLICT(company_id,as_of,source) DO UPDATE SET shares=excluded.shares";

const RATIO_UPSERT_SQL: &str = "INSERT INTO ratios (company_id,period_end,period_type,metric,value,computed_at) \
     VALUES (?,?,?,?,?,?) \
     ON CONFLICT(company_id,period_end,period_type,metric) DO UPDATE SET value=excluded.value, computed_at=excluded.computed_at";

const DB_POOL_MAX_CONNECTIONS: u32 = 16;
/// How long SQLite retries a locked DB before erroring (concurrent writers).
const DB_BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// How long a caller waits for a free pooled connection before erroring.
const DB_ACQUIRE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

const SELECT_COMPANY_COLS: &str =
    "c.id,c.cik,c.ticker,c.name,c.exchange,c.sector,c.industry,c.description,c.website,c.employees,c.status";

const SELECT_GRAHAM_COLS: &str =
    "g.company_id,g.score,g.passes_defensive,g.graham_number,\
     g.ncav_per_share,g.margin_of_safety,g.net_net,g.computed_at";

/// Process-local, config-derived policy knobs the API consults at request time.
///
/// Held by [`Store`] (the shared `Arc<Store>`) so handlers can read configured
/// values instead of hard-coded constants. Defaults match the historical
/// hard-coded behavior; production overrides via [`Store::with_policy`].
#[derive(Clone)]
pub struct Policy {
    /// Graham "adequate size" revenue floor used by the assessment endpoint.
    pub graham_min_revenue: f64,
}

impl Default for Policy {
    fn default() -> Self {
        Self { graham_min_revenue: crate::graham::DEFAULT_MIN_REVENUE }
    }
}

/// SQLite-backed data store. Also holds the process-local login throttle and
/// [`Policy`], since the `Arc<Store>` is the shared handle every request sees.
pub struct Store {
    pool: SqlitePool,
    login_throttle: LoginThrottle,
    policy: Policy,
}

/// Map a `sort_by` token + `sort_dir` to a safe (whitelisted) ORDER BY clause for
/// `list_companies`. Tokens are matched literally — no user input reaches the SQL string.
fn companies_sort_expr(sort_by: Option<&str>, sort_dir: Option<&str>) -> String {
    let dir = if sort_dir == Some("desc") { "DESC" } else { "ASC" };
    match sort_by {
        Some("name") => format!("c.name {dir}"),
        Some("industry") => format!("COALESCE(c.industry,'') {dir}"),
        Some("score") => format!("COALESCE(g.score,-1) {dir}"),
        _ => format!("c.ticker {dir}"),
    }
}

/// Map a `sort_by` token + `sort_dir` to a safe (whitelisted) ORDER BY clause for
/// `screen`. Default (no `sort_by`) preserves the original score-desc ordering.
fn screen_sort_expr(sort_by: Option<&str>, sort_dir: Option<&str>) -> String {
    let dir = if sort_dir == Some("desc") { "DESC" } else { "ASC" };
    match sort_by {
        Some("ticker") => format!("c.ticker {dir}"),
        Some("graham_number") => format!("COALESCE(g.graham_number,-1) {dir}"),
        Some("margin_of_safety") => format!("COALESCE(g.margin_of_safety,-1e9) {dir}"),
        _ => "g.score DESC, c.ticker ASC".to_string(),
    }
}

/// Builds the dynamic JOIN + WHERE fragments for ratio-based screener filters.
struct ScreenQueryBuilder {
    extra_joins: String,
    extra_conditions: String,
    binds: Vec<f64>,
}

impl ScreenQueryBuilder {
    fn new() -> Self {
        Self { extra_joins: String::new(), extra_conditions: String::new(), binds: Vec::new() }
    }

    /// Add a LEFT JOIN + WHERE condition for the latest annual value of `metric`.
    /// Only adds SQL if at least one of `min_val`/`max_val` is `Some`.
    fn add_ratio_filter(&mut self, metric: &str, min_val: Option<f64>, max_val: Option<f64>) {
        if min_val.is_none() && max_val.is_none() {
            return;
        }
        let alias = format!("r_{metric}");
        self.extra_joins.push_str(&format!(
            " LEFT JOIN (SELECT r1.company_id, r1.value AS val \
             FROM ratios r1 \
             WHERE r1.metric='{metric}' AND r1.period_type='annual' \
             AND r1.period_end = (\
               SELECT MAX(r2.period_end) FROM ratios r2 \
               WHERE r2.company_id=r1.company_id AND r2.metric='{metric}' \
               AND r2.period_type='annual'\
             )) {alias} ON {alias}.company_id=c.id"
        ));
        if let Some(v) = min_val {
            self.extra_conditions.push_str(&format!(" AND {alias}.val >= ?"));
            self.binds.push(v);
        }
        if let Some(v) = max_val {
            self.extra_conditions.push_str(&format!(" AND {alias}.val <= ?"));
            self.binds.push(v);
        }
    }
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
            .busy_timeout(DB_BUSY_TIMEOUT)
            .foreign_keys(true);
        // WAL lets the read-only API run concurrently with writers. The pool is
        // sized above the default collection concurrency so parallel ingest
        // never waits on a connection; acquire_timeout still bounds any wait.
        let pool = SqlitePoolOptions::new()
            .max_connections(DB_POOL_MAX_CONNECTIONS)
            .acquire_timeout(DB_ACQUIRE_TIMEOUT)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await.map_err(other)?;
        Ok(Self {
            pool,
            login_throttle: LoginThrottle::new(LOGIN_MAX_ATTEMPTS, LOGIN_WINDOW),
            policy: Policy::default(),
        })
    }

    /// Override the runtime [`Policy`] (builder-style; call before sharing).
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// The runtime policy knobs.
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// The shared login brute-force throttle.
    pub fn login_throttle(&self) -> &LoginThrottle {
        &self.login_throttle
    }

    /// Close the underlying connection pool. After this, queries error
    /// (used to exercise best-effort failure paths).
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

mod accounts;
mod analytics;
mod companies;
mod records;
mod runs;

pub use analytics::ScreenFilter;

#[cfg(test)]
mod tests {
    use super::*;
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
                .upsert_price(&PricePoint { company_id: id, date: d, open: None, high: None, low: None, close: 1.0, volume: None, source: "fmp".into() })
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
            .save_prices(&[PricePoint { company_id: id, date: d, open: None, high: None, low: None, close: 1.0, volume: None, source: "fmp".into() }])
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
                period_type: PeriodType::Annual,
                metric: "net_margin".into(),
                value: 0.25,
                computed_at: now,
            }])
            .await
            .unwrap();
        assert_eq!(store.get_prices(id).await.unwrap().len(), 1);
        assert_eq!(store.get_news(id).await.unwrap().len(), 1);
        assert_eq!(store.get_ratios(id, None).await.unwrap().len(), 1);
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
    async fn update_company_profile_overwrites_present_fields_only() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap(); // exchange NASDAQ, sector Technology
        store
            .update_company_profile(
                id,
                &CompanyProfile {
                    sector: Some("Basic Materials".into()),
                    industry: Some("Building Materials".into()),
                    exchange: None, // keep existing NASDAQ
                    website: Some("https://x.com".into()),
                    description: Some("makes things".into()),
                    employees: Some(10961),
                },
            )
            .await
            .unwrap();
        let c = store.get_company("AAPL").await.unwrap().unwrap();
        assert_eq!(c.sector.as_deref(), Some("Basic Materials")); // overwritten
        assert_eq!(c.industry.as_deref(), Some("Building Materials"));
        assert_eq!(c.exchange.as_deref(), Some("NASDAQ")); // kept (update was None)
        assert_eq!(c.website.as_deref(), Some("https://x.com"));
        assert_eq!(c.description.as_deref(), Some("makes things"));
        assert_eq!(c.employees, Some(10961));

        // a partial second update only touches description
        store
            .update_company_profile(id, &CompanyProfile { description: Some("v2".into()), ..Default::default() })
            .await
            .unwrap();
        let c = store.get_company("AAPL").await.unwrap().unwrap();
        assert_eq!(c.description.as_deref(), Some("v2"));
        assert_eq!(c.website.as_deref(), Some("https://x.com")); // unchanged
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
            .upsert_price(&PricePoint { company_id: id, date: d2, open: None, high: None, low: None, close: 10.0, volume: Some(5), source: "fmp".into() })
            .await
            .unwrap();
        store
            .upsert_price(&PricePoint { company_id: id, date: d1, open: None, high: None, low: None, close: 9.0, volume: None, source: "fmp".into() })
            .await
            .unwrap();
        // update existing (d2, fmp)
        store
            .upsert_price(&PricePoint { company_id: id, date: d2, open: None, high: None, low: None, close: 11.0, volume: Some(7), source: "fmp".into() })
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
    async fn get_ratios_errors_on_bad_period_type() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        sqlx::query(
            "INSERT INTO ratios (company_id,period_end,period_type,metric,value,computed_at) VALUES (?,?,?,?,?,?)",
        )
        .bind(id).bind("2023-12-31").bind("weekly").bind("pe").bind(1.0).bind("2024-01-01T00:00:00Z")
        .execute(&store.pool).await.unwrap();
        let err = store.get_ratios(id, None).await.unwrap_err();
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
                period_type: PeriodType::Annual,
                metric: "pe".into(),
                value: 28.5,
                computed_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            })
            .await
            .unwrap();
        // same (company, period, period_type, metric) updates
        store
            .upsert_ratio(&Ratio {
                company_id: id,
                period_end: pe,
                period_type: PeriodType::Annual,
                metric: "pe".into(),
                value: 30.0,
                computed_at: Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
            })
            .await
            .unwrap();
        // a quarterly row with the same end date coexists (distinct period_type)
        store
            .upsert_ratio(&Ratio {
                company_id: id,
                period_end: pe,
                period_type: PeriodType::Quarterly,
                metric: "pe".into(),
                value: 7.0,
                computed_at: Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
            })
            .await
            .unwrap();
        let ratios = store.get_ratios(id, None).await.unwrap();
        assert_eq!(ratios.len(), 2); // annual + quarterly, no collision
        // period filter narrows to one
        let annual = store.get_ratios(id, Some(PeriodType::Annual)).await.unwrap();
        assert_eq!(annual.len(), 1);
        assert_eq!(annual[0].value, 30.0);
        assert_eq!(annual[0].period_type, PeriodType::Annual);
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
    async fn reflagging_a_discrepancy_updates_instead_of_duplicating() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let disc = |value_b: f64, hour: u32| Discrepancy {
            company_id: id,
            field: "Revenue".into(),
            period: Some("2023".into()),
            source_a: "edgar".into(),
            value_a: 100.0,
            source_b: "fmp".into(),
            value_b,
            pct_diff: (value_b - 100.0) / 100.0,
            flagged_at: Utc.with_ymd_and_hms(2024, 1, 1, hour, 0, 0).unwrap(),
        };
        store.save_reconciled(&[], &[disc(110.0, 1)]).await.unwrap();
        store.save_reconciled(&[], &[disc(130.0, 2)]).await.unwrap();

        let list = store.get_discrepancies(id).await.unwrap();
        assert_eq!(list.len(), 1, "same key updates in place");
        assert_eq!(list[0].value_b, 130.0);
        assert_eq!(list[0].flagged_at, Utc.with_ymd_and_hms(2024, 1, 1, 2, 0, 0).unwrap());

        // a different period is a distinct row
        let mut other = disc(120.0, 3);
        other.period = Some("2022".into());
        store.save_reconciled(&[], &[other]).await.unwrap();
        assert_eq!(store.get_discrepancies(id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn users_sessions_lifecycle() {
        let (store, _d) = temp_store().await;
        let uid = store.create_user("a@e.com", "hash").await.unwrap();
        // duplicate email errors
        assert!(store.create_user("a@e.com", "h2").await.is_err());
        assert_eq!(store.user_credentials("a@e.com").await.unwrap(), Some((uid, "hash".into())));
        assert_eq!(store.user_credentials("none@e.com").await.unwrap(), None);
        assert_eq!(store.user_email(uid).await.unwrap(), Some("a@e.com".into()));

        let t = |h| Utc.with_ymd_and_hms(2024, 1, 1, h, 0, 0).unwrap();
        store.create_session("th", uid, t(12)).await.unwrap();
        // valid before expiry, invalid after
        assert_eq!(store.session_user("th", t(11)).await.unwrap(), Some(uid));
        assert_eq!(store.session_user("th", t(13)).await.unwrap(), None);
        assert_eq!(store.session_user("nope", t(11)).await.unwrap(), None);
        store.delete_session("th").await.unwrap();
        assert_eq!(store.session_user("th", t(11)).await.unwrap(), None);
    }

    #[tokio::test]
    async fn user_profile_and_password_updates() {
        let (store, _d) = temp_store().await;
        let uid = store.create_user("a@e.com", "hash").await.unwrap();
        // default display name is blank
        assert_eq!(store.user_profile(uid).await.unwrap(), Some(("a@e.com".into(), "".into())));
        assert_eq!(store.user_profile(999).await.unwrap(), None);
        // update email + display name
        store.update_profile(uid, "b@e.com", "Bob").await.unwrap();
        assert_eq!(store.user_profile(uid).await.unwrap(), Some(("b@e.com".into(), "Bob".into())));
        // colliding with another user's email errors
        let other = store.create_user("c@e.com", "h").await.unwrap();
        assert!(store.update_profile(other, "b@e.com", "X").await.is_err());
        // password hash read + replace
        assert_eq!(store.user_password_hash(uid).await.unwrap(), Some("hash".into()));
        assert_eq!(store.user_password_hash(999).await.unwrap(), None);
        store.update_password_hash(uid, "newhash").await.unwrap();
        assert_eq!(store.user_password_hash(uid).await.unwrap(), Some("newhash".into()));
    }

    #[tokio::test]
    async fn company_status_and_delisted_filtering() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        // default status is active
        assert_eq!(store.company_status(id).await.unwrap(), Some("active".into()));
        assert_eq!(store.company_status(999).await.unwrap(), None);
        // latest price date across sources: none, then the newest
        assert_eq!(store.latest_price_date_any(id).await.unwrap(), None);
        for (d, src) in [("2024-01-02", "fmp"), ("2024-03-01", "yahoo")] {
            store
                .upsert_price(&PricePoint {
                    company_id: id,
                    date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
                    open: None,
                    high: None,
                    low: None,
                    close: 1.0,
                    volume: None,
                    source: src.into(),
                })
                .await
                .unwrap();
        }
        assert_eq!(store.latest_price_date_any(id).await.unwrap(), NaiveDate::from_ymd_opt(2024, 3, 1));
        // active company shows in the directory
        assert_eq!(store.list_companies(None, &[], None, None, 10, 0, false).await.unwrap().1, 1);
        // mark delisted -> hidden by default, shown when included
        store.set_company_status(id, "delisted").await.unwrap();
        assert_eq!(store.company_status(id).await.unwrap(), Some("delisted".into()));
        assert_eq!(store.list_companies(None, &[], None, None, 10, 0, false).await.unwrap().1, 0);
        let (rows, total) = store.list_companies(None, &[], None, None, 10, 0, true).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].0.status, "delisted");
    }

    #[tokio::test]
    async fn graham_scores_save_get_and_screen() {
        let (store, _d) = temp_store().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let aapl = store.upsert_company(&sample_company()).await.unwrap();
        let mut msft = sample_company();
        msft.ticker = "MSFT".into();
        let msft_id = store.upsert_company(&msft).await.unwrap();

        let score = |cid, s, def| GrahamScore {
            company_id: cid,
            score: s,
            passes_defensive: def,
            graham_number: Some(100.0),
            ncav_per_share: None,
            margin_of_safety: Some(0.2),
            net_net: false,
            computed_at: now,
        };
        store.save_graham_score(&score(aapl, 8, true)).await.unwrap();
        store.save_graham_score(&score(msft_id, 4, false)).await.unwrap();
        // upsert is idempotent
        store.save_graham_score(&score(aapl, 8, true)).await.unwrap();

        assert_eq!(store.get_graham_score(aapl).await.unwrap().unwrap().score, 8);

        // screen: all (min 0) -> AAPL(8) before MSFT(4); total reflects all matches
        let (all, total) = store.screen(&ScreenFilter::default()).await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(total, 2);
        assert_eq!(all[0].0.ticker, "AAPL");
        // defensive only -> just AAPL
        let defensive = ScreenFilter { defensive_only: true, ..Default::default() };
        let (def, def_total) = store.screen(&defensive).await.unwrap();
        assert_eq!(def.len(), 1);
        assert_eq!(def_total, 1);
        assert_eq!(def[0].0.ticker, "AAPL");
        // min_score filter
        let scored = ScreenFilter { min_score: 5, ..Default::default() };
        assert_eq!(store.screen(&scored).await.unwrap().0.len(), 1);
        // net-net filter (none set) -> empty
        let net_net = ScreenFilter { net_net_only: true, ..Default::default() };
        assert_eq!(store.screen(&net_net).await.unwrap().1, 0);
        // offset paginates: skip the first of two
        let paged = ScreenFilter { offset: 1, ..Default::default() };
        let (page2, _) = store.screen(&paged).await.unwrap();
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].0.ticker, "MSFT");
        // the filter itself is plain data
        assert_eq!(paged.clone(), paged);
        assert!(format!("{paged:?}").contains("offset: 1"));
    }

    #[tokio::test]
    async fn list_companies_paginates_searches_and_attaches_scores() {
        let (store, _d) = temp_store().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let aapl = store.upsert_company(&sample_company()).await.unwrap();
        let mut msft = sample_company();
        msft.ticker = "MSFT".into();
        msft.name = "Microsoft".into();
        store.upsert_company(&msft).await.unwrap();
        store
            .save_graham_score(&GrahamScore {
                company_id: aapl,
                score: 6,
                passes_defensive: false,
                graham_number: Some(100.0),
                ncav_per_share: None,
                margin_of_safety: Some(0.1),
                net_net: false,
                computed_at: now,
            })
            .await
            .unwrap();

        // page of all, ordered by ticker; AAPL has a score, MSFT does not
        let (rows, total) = store.list_companies(None, &[], None, None, 10, 0, false).await.unwrap();
        assert_eq!(total, 2);
        assert_eq!(rows[0].0.ticker, "AAPL");
        assert!(rows[0].1.is_some());
        assert!(rows[1].1.is_none());
        // search by name
        let (msrows, mstotal) = store.list_companies(Some("micro"), &[], None, None, 10, 0, false).await.unwrap();
        assert_eq!(mstotal, 1);
        assert_eq!(msrows[0].0.ticker, "MSFT");
        // limit + offset
        let (p2, _) = store.list_companies(None, &[], None, None, 1, 1, false).await.unwrap();
        assert_eq!(p2.len(), 1);
        assert_eq!(p2[0].0.ticker, "MSFT");

        // LIKE wildcards in query must be escaped and treated literally
        let (pct, pct_total) = store.list_companies(Some("%"), &[], None, None, 10, 0, false).await.unwrap();
        assert_eq!(pct_total, 0, "bare % should not match any company");
        assert!(pct.is_empty());
        let (und, und_total) = store.list_companies(Some("_"), &[], None, None, 10, 0, false).await.unwrap();
        assert_eq!(und_total, 0, "bare _ should not match any company");
        assert!(und.is_empty());
    }

    #[tokio::test]
    async fn list_companies_per_column_filters_narrow_and_combine() {
        let (store, _d) = temp_store().await;
        let mut a = sample_company();
        a.ticker = "AAA".into();
        a.name = "Alpha Corp".into();
        a.industry = Some("Software".into());
        store.upsert_company(&a).await.unwrap();
        let mut b = sample_company();
        b.cik = "0000000002".into();
        b.ticker = "BBB".into();
        b.name = "Beta Corp".into();
        b.industry = Some("Hardware".into());
        store.upsert_company(&b).await.unwrap();
        let mut c = sample_company();
        c.cik = "0000000003".into();
        c.ticker = "CCC".into();
        c.name = "Gamma Inc".into();
        c.industry = Some("Software".into());
        store.upsert_company(&c).await.unwrap();

        // filter by ticker
        let (rows, total) =
            store.list_companies(None, &[("ticker", "AAA")], None, None, 10, 0, false).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.ticker, "AAA");

        // filter by name
        let (rows, total) =
            store.list_companies(None, &[("name", "Beta")], None, None, 10, 0, false).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].0.ticker, "BBB");

        // filter by industry narrows to the two Software companies
        let (rows, total) =
            store.list_companies(None, &[("industry", "Software")], None, None, 10, 0, false).await.unwrap();
        assert_eq!(total, 2);
        let tickers: Vec<_> = rows.iter().map(|r| r.0.ticker.as_str()).collect();
        assert!(tickers.contains(&"AAA"));
        assert!(tickers.contains(&"CCC"));

        // combining filters ANDs them: Software + name Gamma -> only CCC
        let (rows, total) = store
            .list_companies(None, &[("industry", "Software"), ("name", "Gamma")], None, None, 10, 0, false)
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].0.ticker, "CCC");

        // a column outside the allow-list is ignored (no narrowing)
        let (_rows, total) =
            store.list_companies(None, &[("sector", "Technology")], None, None, 10, 0, false).await.unwrap();
        assert_eq!(total, 3);

        // per-column filter combines with the global q
        let (rows, total) = store
            .list_companies(Some("AAA"), &[("industry", "Software")], None, None, 10, 0, false)
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].0.ticker, "AAA");
    }

    #[test]
    fn companies_sort_expr_whitelist() {
        assert_eq!(companies_sort_expr(Some("name"), Some("desc")), "c.name DESC");
        assert_eq!(companies_sort_expr(Some("industry"), Some("asc")), "COALESCE(c.industry,'') ASC");
        assert_eq!(companies_sort_expr(Some("score"), Some("desc")), "COALESCE(g.score,-1) DESC");
        assert_eq!(companies_sort_expr(Some("ticker"), None), "c.ticker ASC");
        assert_eq!(companies_sort_expr(None, None), "c.ticker ASC");
        // unknown token falls back to ticker (SQL injection attempt returns safe fallback)
        assert_eq!(companies_sort_expr(Some("'; DROP TABLE companies; --"), None), "c.ticker ASC");
    }

    #[test]
    fn screen_sort_expr_whitelist() {
        assert_eq!(screen_sort_expr(Some("ticker"), Some("asc")), "c.ticker ASC");
        assert_eq!(screen_sort_expr(Some("graham_number"), Some("desc")), "COALESCE(g.graham_number,-1) DESC");
        assert_eq!(screen_sort_expr(Some("margin_of_safety"), None), "COALESCE(g.margin_of_safety,-1e9) ASC");
        // default (no sort_by): score DESC with ticker tiebreaker
        assert_eq!(screen_sort_expr(None, None), "g.score DESC, c.ticker ASC");
        // unknown sort_by falls back to default score ordering
        assert_eq!(screen_sort_expr(Some("bogus"), Some("desc")), "g.score DESC, c.ticker ASC");
    }

    #[tokio::test]
    async fn list_companies_sort_by_score_and_dir() {
        let (store, _d) = temp_store().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let aapl = store.upsert_company(&sample_company()).await.unwrap();
        let mut msft = sample_company();
        msft.ticker = "MSFT".into();
        msft.name = "Microsoft".into();
        let msft_id = store.upsert_company(&msft).await.unwrap();
        store
            .save_graham_score(&GrahamScore {
                company_id: aapl,
                score: 8,
                passes_defensive: true,
                graham_number: None,
                ncav_per_share: None,
                margin_of_safety: None,
                net_net: false,
                computed_at: now,
            })
            .await
            .unwrap();
        store
            .save_graham_score(&GrahamScore {
                company_id: msft_id,
                score: 3,
                passes_defensive: false,
                graham_number: None,
                ncav_per_share: None,
                margin_of_safety: None,
                net_net: false,
                computed_at: now,
            })
            .await
            .unwrap();

        // sort by score desc -> AAPL(8) first
        let (rows, _) = store.list_companies(None, &[], Some("score"), Some("desc"), 10, 0, false).await.unwrap();
        assert_eq!(rows[0].0.ticker, "AAPL");
        assert_eq!(rows[1].0.ticker, "MSFT");

        // sort by score asc -> MSFT(3) first
        let (rows, _) = store.list_companies(None, &[], Some("score"), Some("asc"), 10, 0, false).await.unwrap();
        assert_eq!(rows[0].0.ticker, "MSFT");
        assert_eq!(rows[1].0.ticker, "AAPL");

        // unknown sort_by falls back to ticker asc
        let (rows, _) = store.list_companies(None, &[], Some("bogus"), None, 10, 0, false).await.unwrap();
        assert_eq!(rows[0].0.ticker, "AAPL");
    }

    #[tokio::test]
    async fn screen_sort_by_ticker() {
        let (store, _d) = temp_store().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let aapl = store.upsert_company(&sample_company()).await.unwrap();
        let mut msft = sample_company();
        msft.ticker = "MSFT".into();
        let msft_id = store.upsert_company(&msft).await.unwrap();
        let g = |cid, s| GrahamScore {
            company_id: cid,
            score: s,
            passes_defensive: false,
            graham_number: None,
            ncav_per_share: None,
            margin_of_safety: None,
            net_net: false,
            computed_at: now,
        };
        store.save_graham_score(&g(aapl, 5)).await.unwrap();
        store.save_graham_score(&g(msft_id, 5)).await.unwrap();

        // default (no sort_by) -> score DESC (both equal), tie-breaks by ticker -> AAPL, MSFT
        let (rows, _) = store.screen(&ScreenFilter::default()).await.unwrap();
        assert_eq!(rows[0].0.ticker, "AAPL");

        // sort by ticker desc -> MSFT first
        let by_ticker_desc = ScreenFilter {
            sort_by: Some("ticker".into()),
            sort_dir: Some("desc".into()),
            ..Default::default()
        };
        let (rows, _) = store.screen(&by_ticker_desc).await.unwrap();
        assert_eq!(rows[0].0.ticker, "MSFT");
        assert_eq!(rows[1].0.ticker, "AAPL");
    }

    #[tokio::test]
    async fn save_shares_upserts_by_company_date_source() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let count = |shares: f64| ShareCount {
            company_id: id,
            as_of: NaiveDate::from_ymd_opt(2023, 9, 30).unwrap(),
            shares,
            source: "edgar".into(),
        };
        store.save_shares(&[count(100.0)]).await.unwrap();
        // same (company, as_of, source) updates in place
        store.save_shares(&[count(110.0)]).await.unwrap();
        let latest = store.latest_shares(id).await.unwrap().unwrap();
        assert_eq!(latest.shares, 110.0);
        assert_eq!(latest.as_of, NaiveDate::from_ymd_opt(2023, 9, 30).unwrap());
        assert_eq!(latest.clone(), latest);
        assert!(format!("{latest:?}").contains("edgar"));
    }

    #[tokio::test]
    async fn latest_shares_returns_newest_or_none() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        assert_eq!(store.latest_shares(id).await.unwrap(), None);
        for (d, n) in [("2022-09-30", 90.0), ("2023-09-30", 80.0)] {
            store
                .save_shares(&[ShareCount {
                    company_id: id,
                    as_of: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
                    shares: n,
                    source: "edgar".into(),
                }])
                .await
                .unwrap();
        }
        assert_eq!(store.latest_shares(id).await.unwrap().unwrap().shares, 80.0);
    }

    #[tokio::test]
    async fn latest_price_date_is_per_source() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        assert_eq!(store.latest_price_date(id, "fmp").await.unwrap(), None);
        for (d, src) in [("2024-03-01", "fmp"), ("2024-04-01", "yahoo"), ("2024-02-01", "yahoo")] {
            store
                .upsert_price(&PricePoint {
                    company_id: id,
                    date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
                    open: None,
                    high: None,
                    low: None,
                    close: 1.0,
                    volume: None,
                    source: src.into(),
                })
                .await
                .unwrap();
        }
        assert_eq!(
            store.latest_price_date(id, "fmp").await.unwrap(),
            NaiveDate::from_ymd_opt(2024, 3, 1)
        );
        assert_eq!(
            store.latest_price_date(id, "yahoo").await.unwrap(),
            NaiveDate::from_ymd_opt(2024, 4, 1)
        );
    }

    #[tokio::test]
    async fn latest_price_returns_newest() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        assert_eq!(store.latest_price(id).await.unwrap(), None);
        for (d, c) in [(("2024-01-02"), 10.0), (("2024-03-01"), 20.0)] {
            store
                .upsert_price(&PricePoint {
                    company_id: id,
                    date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
                    open: None,
                    high: None,
                    low: None,
                    close: c,
                    volume: None,
                    source: "fmp".into(),
                })
                .await
                .unwrap();
        }
        assert_eq!(store.latest_price(id).await.unwrap(), Some(20.0));
    }

    #[tokio::test]
    async fn watchlist_add_list_remove() {
        let (store, _d) = temp_store().await;
        let uid = store.create_user("a@e.com", "h").await.unwrap();
        let cid = store.insert_company(&sample_company()).await.unwrap();
        store.add_watch(uid, cid).await.unwrap();
        store.add_watch(uid, cid).await.unwrap(); // idempotent
        let list = store.list_watch(uid).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].ticker, "AAPL");
        store.remove_watch(uid, cid).await.unwrap();
        assert!(store.list_watch(uid).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn watch_groups_crud_membership_and_isolation() {
        let (store, _d) = temp_store().await;
        let uid = store.create_user("a@e.com", "h").await.unwrap();
        let other = store.create_user("b@e.com", "h").await.unwrap();
        let cid = store.insert_company(&sample_company()).await.unwrap();
        store.add_watch(uid, cid).await.unwrap();

        // create groups; ordered by name
        let tech = store.create_group(uid, "Tech").await.unwrap();
        let div = store.create_group(uid, "Dividends").await.unwrap();
        let groups = store.list_groups(uid).await.unwrap();
        assert_eq!(groups.iter().map(|g| g.name.clone()).collect::<Vec<_>>(), ["Dividends", "Tech"]);
        // exercise WatchGroup derives
        assert_eq!(groups[0].clone(), groups[0]);
        assert!(format!("{:?}", groups[0]).contains("Dividends"));
        // duplicate name for the same user errors; other user can reuse it
        assert!(store.create_group(uid, "Tech").await.is_err());
        store.create_group(other, "Tech").await.unwrap();

        // tag the company into both groups (idempotent)
        store.add_to_group(uid, tech, cid).await.unwrap();
        store.add_to_group(uid, tech, cid).await.unwrap();
        store.add_to_group(uid, div, cid).await.unwrap();
        let mut members = store.watch_group_memberships(uid).await.unwrap();
        members.sort();
        let mut expected_members = vec![(cid, tech), (cid, div)];
        expected_members.sort();
        assert_eq!(members, expected_members);
        // memberships flow into watch_quotes
        let q = store.watch_quotes(uid).await.unwrap();
        let mut gids = q[0].group_ids.clone();
        gids.sort();
        let mut expected = vec![tech, div];
        expected.sort();
        assert_eq!(gids, expected);

        // another user can't tag into someone else's group (no-op)
        store.add_to_group(other, tech, cid).await.unwrap();
        assert_eq!(store.watch_group_memberships(uid).await.unwrap().len(), 2);

        // untag (ownership-checked), rename, delete (cascades members)
        store.remove_from_group(uid, div, cid).await.unwrap();
        assert_eq!(store.watch_group_memberships(uid).await.unwrap().len(), 1);
        store.rename_group(uid, tech, "Technology").await.unwrap();
        assert!(store.list_groups(uid).await.unwrap().iter().any(|g| g.name == "Technology"));
        store.delete_group(uid, tech).await.unwrap();
        assert!(store.watch_group_memberships(uid).await.unwrap().is_empty());
        // div remains until deleted too
        store.delete_group(uid, div).await.unwrap();
        assert!(store.list_groups(uid).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn source_errors_save_and_list_newest_first() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let t = |h| Utc.with_ymd_and_hms(2024, 1, 1, h, 0, 0).unwrap();
        assert!(store.recent_source_errors(id, 10).await.unwrap().is_empty());

        store
            .save_source_errors(id, &[("edgar".into(), "503".into())], t(1))
            .await
            .unwrap();
        store
            .save_source_errors(
                id,
                &[("fmp".into(), "timeout".into()), ("yahoo".into(), "404".into())],
                t(2),
            )
            .await
            .unwrap();
        // saving an empty batch is a no-op, not an error
        store.save_source_errors(id, &[], t(3)).await.unwrap();

        let errors = store.recent_source_errors(id, 10).await.unwrap();
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0].occurred_at, t(2)); // newest first
        assert_eq!(errors[2].source, "edgar");
        assert_eq!(errors[2].message, "503");
        assert_eq!(errors[0].clone(), errors[0]);
        assert!(format!("{:?}", errors[1]).contains("fmp") || format!("{:?}", errors[1]).contains("yahoo"));
        // limit applies
        assert_eq!(store.recent_source_errors(id, 1).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ownership_saves_upserts_and_lists_by_position() {
        let (store, _d) = temp_store().await;
        let id = store.insert_company(&sample_company()).await.unwrap();
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        let hold = |holder: &str, shares: f64, as_of| OwnershipHolding {
            company_id: id, holder: holder.into(), kind: "insider".into(),
            shares, as_of, source: "edgar".into(),
        };
        assert!(store.get_ownership(id).await.unwrap().is_empty());
        store
            .save_ownership(&[hold("Tim Cook", 100.0, d(2024, 1, 2)), hold("Jeff Williams", 300.0, d(2024, 2, 1))])
            .await
            .unwrap();
        // re-saving the same (holder, as_of, source) updates the share count in place
        store.save_ownership(&[hold("Tim Cook", 150.0, d(2024, 1, 2))]).await.unwrap();
        let rows = store.get_ownership(id).await.unwrap();
        assert_eq!(rows.len(), 2);
        // newest as_of first, then larger position
        assert_eq!(rows[0].holder, "Jeff Williams");
        assert_eq!(rows[0].as_of, d(2024, 2, 1));
        assert_eq!(rows[1].holder, "Tim Cook");
        assert_eq!(rows[1].shares, 150.0); // upserted
        // an empty batch is a no-op
        store.save_ownership(&[]).await.unwrap();
        assert_eq!(store.get_ownership(id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn day_changes_errors_when_store_is_closed() {
        let (store, _d) = temp_store().await;
        store.close().await;
        assert!(store.day_changes().await.is_err());
    }

    #[tokio::test]
    async fn indices_are_collected_but_hidden_from_directory_and_movers() {
        let (store, _d) = temp_store().await;
        let aapl = store.insert_company(&sample_company()).await.unwrap();
        let idx = store.upsert_index("^GSPC", "S&P 500").await.unwrap();
        // re-seeding the same index is idempotent (keeps a single row)
        assert_eq!(store.upsert_index("^GSPC", "S&P 500").await.unwrap(), idx);
        let d1 = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
        for (cid, c1, c2) in [(aapl, 100.0, 110.0), (idx, 4000.0, 4200.0)] {
            for (d, close) in [(d1, c1), (d2, c2)] {
                store
                    .upsert_price(&PricePoint {
                        company_id: cid, date: d, open: None, high: None, low: None,
                        close, volume: Some(1), source: "yahoo".into(),
                    })
                    .await
                    .unwrap();
            }
        }
        // collection still sees the index ...
        assert_eq!(store.all_companies().await.unwrap().len(), 2);
        // ... but the directory hides it
        let (rows, total) = store.list_companies(None, &[], None, None, 10, 0, false).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.ticker, "AAPL");
        // ... and so do the movers
        let movers = store.day_changes().await.unwrap();
        assert_eq!(movers.len(), 1);
        assert_eq!(movers[0].company.ticker, "AAPL");
        // index_changes returns only indices with their computed move
        let idx_rows = store.index_changes().await.unwrap();
        assert_eq!(idx_rows.len(), 1);
        assert_eq!(idx_rows[0].company.ticker, "^GSPC");
        assert!((idx_rows[0].change - 200.0).abs() < 1e-9);
        assert_eq!(idx_rows[0].company.cik, "IDX-GSPC");
    }

    #[tokio::test]
    async fn http_validators_roundtrip_and_overwrite() {
        let (store, _d) = temp_store().await;
        assert_eq!(store.http_validators("https://u").await.unwrap(), None);
        store
            .save_http_validators(
                "https://u",
                &Validators { etag: Some("\"v1\"".into()), last_modified: None },
            )
            .await
            .unwrap();
        let v = store.http_validators("https://u").await.unwrap().unwrap();
        assert_eq!(v.etag.as_deref(), Some("\"v1\""));
        assert_eq!(v.last_modified, None);

        store
            .save_http_validators(
                "https://u",
                &Validators { etag: Some("\"v2\"".into()), last_modified: Some("lm".into()) },
            )
            .await
            .unwrap();
        let v = store.http_validators("https://u").await.unwrap().unwrap();
        assert_eq!(v.etag.as_deref(), Some("\"v2\""));
        assert_eq!(v.last_modified.as_deref(), Some("lm"));
    }

    #[tokio::test]
    async fn validator_repo_is_best_effort_on_a_broken_store() {
        use crate::net::HttpValidatorRepo;
        let (store, _d) = temp_store().await;
        store
            .save_http_validators("https://u", &Validators { etag: Some("e".into()), last_modified: None })
            .await
            .unwrap();
        assert_eq!(
            HttpValidatorRepo::validators(&store, "https://u").await.unwrap().etag.as_deref(),
            Some("e")
        );
        store.close().await;
        // a failing cache reads as "no validators" and swallows writes
        assert!(HttpValidatorRepo::validators(&store, "https://u").await.is_none());
        HttpValidatorRepo::store_validators(&store, "https://u", &Validators::default()).await;
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
            .upsert_price(&PricePoint { company_id: id, date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(), open: None, high: None, low: None, close: 9.5, volume: Some(3), source: "fmp".into() })
            .await
            .unwrap();
        store
            .upsert_price(&PricePoint { company_id: id, date: NaiveDate::from_ymd_opt(2024, 1, 3).unwrap(), open: None, high: None, low: None, close: 10.5, volume: None, source: "fmp".into() })
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
        assert_eq!(batch.num_columns(), 7); // date, open, high, low, close, volume, source
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

    #[tokio::test]
    async fn screen_sector_filter() {
        let (store, _d) = temp_store().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let aapl = store.upsert_company(&sample_company()).await.unwrap(); // sector=Technology
        let mut msft = sample_company();
        msft.ticker = "MSFT".into();
        msft.sector = Some("Healthcare".into());
        let msft_id = store.upsert_company(&msft).await.unwrap();
        for (cid, s) in [(aapl, 8i64), (msft_id, 6)] {
            store.save_graham_score(&GrahamScore {
                company_id: cid, score: s, passes_defensive: true,
                graham_number: None, ncav_per_share: None, margin_of_safety: None,
                net_net: false, computed_at: now,
            }).await.unwrap();
        }
        let in_sector =
            |s: &str| ScreenFilter { sector: Some(s.to_string()), ..Default::default() };
        // sector=Technology -> only AAPL
        let (rows, total) = store.screen(&in_sector("Technology")).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].0.ticker, "AAPL");
        // sector=Healthcare -> only MSFT
        let (rows, _) = store.screen(&in_sector("Healthcare")).await.unwrap();
        assert_eq!(rows[0].0.ticker, "MSFT");
        // sector=Unknown -> empty
        let (_, total) = store.screen(&in_sector("Unknown")).await.unwrap();
        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn get_peers_returns_same_sector_excluding_self() {
        let (store, _d) = temp_store().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let aapl = store.upsert_company(&sample_company()).await.unwrap(); // Technology
        let mut msft = sample_company();
        msft.ticker = "MSFT".into();
        msft.sector = Some("Technology".into());
        let msft_id = store.upsert_company(&msft).await.unwrap();
        let mut jnj = sample_company();
        jnj.ticker = "JNJ".into();
        jnj.sector = Some("Healthcare".into());
        store.upsert_company(&jnj).await.unwrap();

        store.save_graham_score(&GrahamScore {
            company_id: msft_id, score: 5, passes_defensive: false,
            graham_number: None, ncav_per_share: None, margin_of_safety: None,
            net_net: false, computed_at: now,
        }).await.unwrap();

        // AAPL's peers: same sector (Technology), excludes AAPL itself
        let peers = store.get_peers(aapl, Some("Technology"), 10).await.unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].0.ticker, "MSFT");
        assert_eq!(peers[0].1.as_ref().unwrap().score, 5);

        // No sector -> empty
        let none_peers = store.get_peers(aapl, None, 10).await.unwrap();
        assert!(none_peers.is_empty());

        // MSFT's peers returns AAPL (no score -> None)
        let msft_peers = store.get_peers(msft_id, Some("Technology"), 10).await.unwrap();
        assert_eq!(msft_peers.len(), 1);
        assert_eq!(msft_peers[0].0.ticker, "AAPL");
        assert!(msft_peers[0].1.is_none());
    }

    #[tokio::test]
    async fn notes_save_get_delete() {
        let (store, _d) = temp_store().await;
        let uid = store.create_user("a@e.com", "h").await.unwrap();
        let cid = store.insert_company(&sample_company()).await.unwrap();
        let t = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        // no note yet
        assert_eq!(store.get_note(uid, cid).await.unwrap(), None);

        // save creates
        store.save_note(uid, cid, "my thoughts", t).await.unwrap();
        assert_eq!(store.get_note(uid, cid).await.unwrap(), Some("my thoughts".into()));

        // save again updates
        store.save_note(uid, cid, "updated", t).await.unwrap();
        assert_eq!(store.get_note(uid, cid).await.unwrap(), Some("updated".into()));

        // delete removes
        store.delete_note(uid, cid).await.unwrap();
        assert_eq!(store.get_note(uid, cid).await.unwrap(), None);

        // delete non-existent is a no-op (no error)
        store.delete_note(uid, cid).await.unwrap();
    }
}
