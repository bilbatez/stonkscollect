//! Ingest orchestration: reconcile multi-source facts and persist the
//! canonical values plus any flagged discrepancies.

use chrono::{DateTime, Utc};
use futures::StreamExt;

use crate::collectors::edgar::CompanyRef;
use crate::collectors::{FactSource, NewsSource, PriceSource, SourceTarget};
use crate::domain::{Company, FinancialFact, GrahamScore, NewCompany};
use crate::{graham, ratios};
use crate::reconcile::reconcile;
use crate::store::{Store, StoreError};

/// Recompute a company's derived ratios and Graham score (called right after
/// its facts are collected, so only changed companies are recomputed).
pub async fn recompute_metrics(
    store: &Store,
    company_id: i64,
    min_revenue: f64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    recompute_ratios(store, company_id, now).await?;
    recompute_graham(store, company_id, min_revenue, now).await
}

/// Recompute and persist a company's Graham defensive-investor score from its
/// stored facts + latest price.
pub async fn recompute_graham(
    store: &Store,
    company_id: i64,
    min_revenue: f64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let facts = store.get_facts(company_id).await?;
    let price = store.latest_price(company_id).await?;
    let a = graham::assess(&facts, price, min_revenue);
    store
        .save_graham_score(&GrahamScore {
            company_id,
            score: a.score as i64,
            passes_defensive: a.passes_defensive,
            graham_number: a.graham_number,
            ncav_per_share: a.ncav_per_share,
            margin_of_safety: a.margin_of_safety,
            net_net: a.net_net,
            computed_at: now,
        })
        .await
}

fn target_of(c: &Company) -> SourceTarget {
    SourceTarget {
        cik: c.cik.clone(),
        symbol: c.ticker.clone(),
    }
}

/// Gather prices from all sources for one company (best-effort) and persist.
pub async fn collect_prices_for(
    store: &Store,
    sources: &[&dyn PriceSource],
    company_id: i64,
    target: &SourceTarget,
) -> Result<usize, StoreError> {
    let mut all = Vec::new();
    for s in sources {
        match s.fetch_prices(company_id, target).await {
            Ok(p) => all.extend(p),
            Err(e) => tracing::warn!("price source {} failed: {e}", s.name()),
        }
    }
    store.save_prices(&all).await?;
    Ok(all.len())
}

/// Gather news from all sources for one company (best-effort) and persist.
pub async fn collect_news_for(
    store: &Store,
    sources: &[&dyn NewsSource],
    company_id: i64,
    target: &SourceTarget,
    now: DateTime<Utc>,
) -> Result<usize, StoreError> {
    let mut all = Vec::new();
    for s in sources {
        match s.fetch_news(company_id, target, now).await {
            Ok(n) => all.extend(n),
            Err(e) => tracing::warn!("news source {} failed: {e}", s.name()),
        }
    }
    store.save_news(&all).await?;
    Ok(all.len())
}

/// Recompute and persist ratios for one company from its stored facts.
pub async fn recompute_ratios(
    store: &Store,
    company_id: i64,
    now: DateTime<Utc>,
) -> Result<usize, StoreError> {
    let facts = store.get_facts(company_id).await?;
    let prices = store.get_prices(company_id).await?;
    let computed = ratios::compute(company_id, &facts, &prices, now);
    store.save_ratios(&computed).await?;
    Ok(computed.len())
}

/// Collect prices for every company (throttled, per-company isolation).
pub async fn collect_prices_all(
    store: &Store,
    sources: &[&dyn PriceSource],
    delay: std::time::Duration,
) -> Result<CollectSummary, StoreError> {
    let companies = store.all_companies().await?;
    let mut s = CollectSummary::default();
    for (i, c) in companies.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(delay).await;
        }
        match collect_prices_for(store, sources, c.id, &target_of(c)).await {
            Ok(n) => {
                s.companies += 1;
                s.facts_written += n;
            }
            Err(e) => {
                tracing::warn!("prices failed for {}: {e}", c.ticker);
                s.failed += 1;
            }
        }
    }
    Ok(s)
}

/// Collect news for every company (throttled, per-company isolation).
pub async fn collect_news_all(
    store: &Store,
    sources: &[&dyn NewsSource],
    now: DateTime<Utc>,
    delay: std::time::Duration,
) -> Result<CollectSummary, StoreError> {
    let companies = store.all_companies().await?;
    let mut s = CollectSummary::default();
    for (i, c) in companies.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(delay).await;
        }
        match collect_news_for(store, sources, c.id, &target_of(c), now).await {
            Ok(n) => {
                s.companies += 1;
                s.facts_written += n;
            }
            Err(e) => {
                tracing::warn!("news failed for {}: {e}", c.ticker);
                s.failed += 1;
            }
        }
    }
    Ok(s)
}

/// Recompute ratios for every company from stored facts.
pub async fn recompute_ratios_all(
    store: &Store,
    now: DateTime<Utc>,
) -> Result<CollectSummary, StoreError> {
    let companies = store.all_companies().await?;
    let mut s = CollectSummary::default();
    for c in &companies {
        match recompute_ratios(store, c.id, now).await {
            Ok(n) => {
                s.companies += 1;
                s.facts_written += n;
            }
            Err(e) => {
                tracing::warn!("ratios failed for {}: {e}", c.ticker);
                s.failed += 1;
            }
        }
    }
    Ok(s)
}

/// Create a user with `email`/`password` if absent (idempotent). Returns `true`
/// when a new user was created, `false` if one already existed. Used to seed a
/// dev login; never call with a weak password outside development.
pub async fn ensure_user(store: &Store, email: &str, password: &str) -> Result<bool, StoreError> {
    if store.user_credentials(email).await?.is_some() {
        return Ok(false);
    }
    store.create_user(email, &crate::auth::hash_password(password)).await?;
    Ok(true)
}

/// Upsert a batch of company identities (idempotent). Returns the count.
pub async fn bootstrap_companies(store: &Store, refs: &[CompanyRef]) -> Result<usize, StoreError> {
    let companies: Vec<NewCompany> = refs
        .iter()
        .map(|r| NewCompany {
            cik: r.cik.clone(),
            ticker: r.ticker.clone(),
            name: r.name.clone(),
            exchange: None,
            sector: None,
            industry: None,
        })
        .collect();
    store.upsert_companies(&companies).await
}

/// Progress sink for a bulk collection pass. Implementations must be `Sync`
/// because `company_done` is called from the concurrent collection stream.
pub trait CollectProgress: Sync {
    /// Called once before any company is processed, with the total to process.
    fn start(&self, total: usize);
    /// Called as each company finishes (`done` is 1-based, in completion order).
    fn company_done(&self, done: usize, total: usize, ticker: &str, ok: bool);
}

/// A [`CollectProgress`] that reports nothing (for callers that don't care).
pub struct NoProgress;
impl CollectProgress for NoProgress {
    fn start(&self, _total: usize) {}
    fn company_done(&self, _done: usize, _total: usize, _ticker: &str, _ok: bool) {}
}

/// Aggregate totals from collecting many companies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectSummary {
    pub companies: usize,
    pub facts_written: usize,
    pub discrepancies_written: usize,
    pub source_errors: usize,
    /// Companies whose ingest failed (logged and skipped, not fatal).
    pub failed: usize,
}

/// Collect every company in the store (the full US universe once bootstrapped),
/// sleeping `delay` between companies to stay polite to upstream APIs.
///
/// A failure on one company is recorded in `summary.failed` and skipped — one
/// bad company never aborts a full pass.
///
/// When `cutoff` is `Some`, only companies not collected since `cutoff` are
/// processed (incremental); successful collections are timestamped.
///
/// Up to `concurrency` companies are fetched at once; politeness is enforced by
/// the shared rate limiter in the HTTP client, not a per-company sleep.
#[allow(clippy::too_many_arguments)]
pub async fn collect_all(
    store: &Store,
    sources: &[&dyn FactSource],
    price_sources: &[&dyn PriceSource],
    threshold: f64,
    now: DateTime<Utc>,
    concurrency: usize,
    cutoff: Option<DateTime<Utc>>,
    min_revenue: f64,
    progress: &dyn CollectProgress,
) -> Result<CollectSummary, StoreError> {
    let companies = match cutoff {
        Some(c) => store.companies_due(c).await?,
        None => store.all_companies().await?,
    };

    let total = companies.len();
    progress.start(total);
    let counter = std::sync::atomic::AtomicUsize::new(0);

    let outcomes = futures::stream::iter(companies)
        .map(|company| {
            let counter = &counter;
            async move {
                let target = target_of(&company);
                let result = ingest(store, sources, company.id, &target, threshold, now).await;
                let done = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                match result {
                    Ok(report) => {
                        // Best-effort: prices first (so Graham sees a price), then
                        // timestamp + per-company metric recompute.
                        let _ = collect_prices_for(store, price_sources, company.id, &target).await;
                        let _ = store.mark_collected(company.id, now).await;
                        let _ = recompute_metrics(store, company.id, min_revenue, now).await;
                        progress.company_done(done, total, &company.ticker, true);
                        Ok((report.facts_written, report.discrepancies_written, report.source_errors.len()))
                    }
                    Err(e) => {
                        tracing::warn!("collect failed for {}: {e}", company.ticker);
                        progress.company_done(done, total, &company.ticker, false);
                        Err(())
                    }
                }
            }
        })
        .buffer_unordered(concurrency.max(1))
        .collect::<Vec<_>>()
        .await;

    let mut summary = CollectSummary::default();
    for outcome in outcomes {
        match outcome {
            Ok((facts, disc, errs)) => {
                summary.companies += 1;
                summary.facts_written += facts;
                summary.discrepancies_written += disc;
                summary.source_errors += errs;
            }
            Err(()) => summary.failed += 1,
        }
    }
    Ok(summary)
}

/// Run [`ingest`] for each known ticker. Unknown tickers (not yet bootstrapped)
/// are skipped. Returns the per-ticker reports.
#[allow(clippy::too_many_arguments)]
pub async fn collect_tickers(
    store: &Store,
    sources: &[&dyn FactSource],
    price_sources: &[&dyn PriceSource],
    tickers: &[String],
    threshold: f64,
    now: DateTime<Utc>,
    min_revenue: f64,
) -> Result<Vec<(String, IngestReport)>, StoreError> {
    let mut outcomes = Vec::new();
    for ticker in tickers {
        let Some(company) = store.get_company(ticker).await? else {
            continue;
        };
        let target = SourceTarget {
            cik: company.cik.clone(),
            symbol: company.ticker.clone(),
        };
        match ingest(store, sources, company.id, &target, threshold, now).await {
            Ok(report) => {
                let _ = collect_prices_for(store, price_sources, company.id, &target).await;
                let _ = recompute_metrics(store, company.id, min_revenue, now).await;
                outcomes.push((ticker.clone(), report));
            }
            Err(e) => tracing::warn!("collect failed for {ticker}: {e}"),
        }
    }
    Ok(outcomes)
}

/// Outcome of an ingest run across multiple sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestReport {
    pub facts_written: usize,
    pub discrepancies_written: usize,
    /// `(source name, error message)` for sources that failed; ingest continues.
    pub source_errors: Vec<(String, String)>,
}

/// Collect facts from every source (best-effort — a failing source is recorded,
/// not fatal), reconcile, and persist canonical facts + discrepancies.
pub async fn ingest(
    store: &Store,
    sources: &[&dyn FactSource],
    company_id: i64,
    target: &SourceTarget,
    threshold: f64,
    now: DateTime<Utc>,
) -> Result<IngestReport, StoreError> {
    let mut all_facts = Vec::new();
    let mut source_errors = Vec::new();
    for source in sources {
        match source.fetch_facts(company_id, target, now).await {
            Ok(facts) => all_facts.extend(facts),
            Err(e) => source_errors.push((source.name().to_string(), e.to_string())),
        }
    }
    let (facts_written, discrepancies_written) =
        persist_facts(store, &all_facts, threshold, now).await?;
    Ok(IngestReport {
        facts_written,
        discrepancies_written,
        source_errors,
    })
}

/// Reconcile `facts` (from any mix of sources) and persist the canonical value
/// per period plus flagged discrepancies. Returns
/// `(facts_written, discrepancies_written)`.
pub async fn persist_facts(
    store: &Store,
    facts: &[FinancialFact],
    threshold: f64,
    now: DateTime<Utc>,
) -> Result<(usize, usize), StoreError> {
    let result = reconcile(facts, threshold, now);
    store
        .save_reconciled(&result.canonical, &result.discrepancies)
        .await?;
    Ok((result.canonical.len(), result.discrepancies.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::edgar::EdgarCollector;
    use crate::collectors::fmp::FmpCollector;
    use crate::collectors::scrape::ScrapeCollector;
    use crate::collectors::news::FinnhubCollector;
    use crate::collectors::CollectorError;
    use crate::domain::{NewCompany, NewsItem, PeriodType, PricePoint, StatementKind};
    use crate::testutil::{fixed_now, FakeHttp};
    use async_trait::async_trait;
    use chrono::{Duration as ChronoDuration, NaiveDate, TimeZone};
    use std::time::Duration;

    const EDGAR: &str = include_str!("../tests/fixtures/edgar_companyfacts.json");
    const FMP_INCOME: &str = include_str!("../tests/fixtures/fmp_income.json");
    const FMP_PRICES: &str = include_str!("../tests/fixtures/fmp_prices.json");
    const NEWS_FINNHUB: &str = include_str!("../tests/fixtures/news_finnhub.json");

    /// A price source whose row has a bad company id -> FK error on save.
    struct BadPriceSource;
    #[async_trait(?Send)]
    impl PriceSource for BadPriceSource {
        fn name(&self) -> &'static str {
            "badp"
        }
        async fn fetch_prices(
            &self,
            _id: i64,
            _t: &SourceTarget,
        ) -> Result<Vec<PricePoint>, CollectorError> {
            Ok(vec![PricePoint {
                company_id: 9_999_999,
                date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                open: None,
                high: None,
                low: None,
                close: 1.0,
                volume: None,
                source: "badp".into(),
            }])
        }
    }

    /// A news source whose item has a bad company id -> FK error on save.
    struct BadNewsSource;
    #[async_trait(?Send)]
    impl NewsSource for BadNewsSource {
        fn name(&self) -> &'static str {
            "badn"
        }
        async fn fetch_news(
            &self,
            _id: i64,
            _t: &SourceTarget,
            now: DateTime<Utc>,
        ) -> Result<Vec<NewsItem>, CollectorError> {
            Ok(vec![NewsItem {
                company_id: 9_999_999,
                title: "x".into(),
                description: None,
                url: "u".into(),
                source: "badn".into(),
                published_at: now,
                dedup_hash: "h".into(),
            }])
        }
    }
    const SCRAPE_HTML: &str = include_str!("../tests/fixtures/scrape_financials.html");

    /// A source returning a fact for a non-existent company id, so persisting it
    /// fails the FK constraint -> a per-company StoreError (not a source error).
    struct BadCompanyIdSource;
    #[async_trait(?Send)]
    impl FactSource for BadCompanyIdSource {
        fn name(&self) -> &'static str {
            "bad"
        }
        async fn fetch_facts(
            &self,
            _company_id: i64,
            _target: &SourceTarget,
            now: DateTime<Utc>,
        ) -> Result<Vec<FinancialFact>, CollectorError> {
            Ok(vec![FinancialFact {
                company_id: 9_999_999, // no such company -> FK violation on save
                statement: StatementKind::Income,
                line_item: "Revenue".into(),
                period_type: PeriodType::Annual,
                period_end: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                value: 1.0,
                source: "bad".into(),
                fetched_at: now,
            }])
        }
    }

    /// A source that always fails, to exercise error capture.
    struct FailingSource;
    #[async_trait(?Send)]
    impl FactSource for FailingSource {
        fn name(&self) -> &'static str {
            "boom"
        }
        async fn fetch_facts(
            &self,
            _company_id: i64,
            _target: &SourceTarget,
            _now: DateTime<Utc>,
        ) -> Result<Vec<FinancialFact>, CollectorError> {
            Err(CollectorError::Http("down".into()))
        }
    }

    async fn store_with_company() -> (Store, i64, tempfile::TempDir) {
        let (store, dir) = crate::testutil::temp_store().await;
        let id = store
            .insert_company(&NewCompany {
                cik: "1".into(),
                ticker: "AAPL".into(),
                name: "Apple".into(),
                exchange: None,
                sector: None,
                industry: None,
            })
            .await
            .unwrap();
        (store, id, dir)
    }

    fn fact(company_id: i64, source: &str, item: &str, value: f64) -> FinancialFact {
        FinancialFact {
            company_id,
            statement: StatementKind::Income,
            line_item: item.to_string(),
            period_type: PeriodType::Annual,
            period_end: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            value,
            source: source.to_string(),
            fetched_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    #[tokio::test]
    async fn persists_canonical_facts_and_flags_discrepancies() {
        let (store, id, _d) = store_with_company().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let facts = vec![
            fact(id, "edgar", "Revenue", 100.0),
            fact(id, "fmp", "Revenue", 130.0), // diverges -> discrepancy
            fact(id, "edgar", "NetIncome", 20.0),
            fact(id, "fmp", "NetIncome", 20.0), // agrees
        ];
        let (facts_written, discrepancies) = persist_facts(&store, &facts, 0.05, now).await.unwrap();
        assert_eq!(facts_written, 2);
        assert_eq!(discrepancies, 1);

        let stored = store.get_facts(id).await.unwrap();
        let revenue = stored.iter().find(|f| f.line_item == "Revenue").unwrap();
        assert_eq!(revenue.value, 100.0); // canonical = EDGAR
        assert_eq!(store.get_discrepancies(id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn surfaces_store_errors() {
        let (store, id, _d) = store_with_company().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        store.close().await;
        let err = persist_facts(&store, &[fact(id, "edgar", "Revenue", 1.0)], 0.05, now).await;
        assert!(err.is_err());
    }

    #[test]
    fn fact_sources_expose_stable_names() {
        assert_eq!(EdgarCollector::new(FakeHttp::new("")).name(), "edgar");
        assert_eq!(
            FactSource::name(&FmpCollector::new(FakeHttp::new(""), "K".into())),
            "fmp"
        );
        assert_eq!(ScrapeCollector::new(FakeHttp::new("")).name(), "scrape");
    }

    #[tokio::test]
    async fn ingest_aggregates_all_sources_and_persists() {
        let (store, id, _d) = store_with_company().await;
        let edgar = EdgarCollector::new(FakeHttp::new(EDGAR));
        let fmp = FmpCollector::new(FakeHttp::new(FMP_INCOME), "KEY".into());
        let scrape = ScrapeCollector::new(FakeHttp::new(SCRAPE_HTML));
        let sources: [&dyn FactSource; 3] = [&edgar, &fmp, &scrape];
        let target = SourceTarget {
            cik: "320193".into(),
            symbol: "AAPL".into(),
        };

        let report = ingest(&store, &sources, id, &target, 0.05, fixed_now())
            .await
            .unwrap();

        assert!(report.source_errors.is_empty());
        assert!(report.facts_written > 0);
        assert!(!store.get_facts(id).await.unwrap().is_empty());
        // exercise IngestReport derives
        assert_eq!(report.clone(), report);
        assert!(format!("{report:?}").contains("facts_written"));
    }

    #[tokio::test]
    async fn bootstrap_companies_upserts_idempotently() {
        let (store, _id, _d) = store_with_company().await;
        let refs = vec![
            CompanyRef { cik: "0000320193".into(), ticker: "AAPL".into(), name: "Apple".into() },
            CompanyRef { cik: "0000789019".into(), ticker: "MSFT".into(), name: "Microsoft".into() },
        ];
        assert_eq!(bootstrap_companies(&store, &refs).await.unwrap(), 2);
        // rerun is safe and still resolves
        bootstrap_companies(&store, &refs).await.unwrap();
        assert_eq!(store.get_company("MSFT").await.unwrap().unwrap().cik, "0000789019");
    }

    #[tokio::test]
    async fn ensure_user_is_idempotent_and_sets_password() {
        let (store, _d) = crate::testutil::temp_store().await;
        assert!(ensure_user(&store, "admin", "admin").await.unwrap()); // created
        assert!(!ensure_user(&store, "admin", "admin").await.unwrap()); // already exists
        let (_, hash) = store.user_credentials("admin").await.unwrap().unwrap();
        assert!(crate::auth::verify_password(&hash, "admin"));
    }

    #[tokio::test]
    async fn collect_all_iterates_every_company() {
        let (store, _id, _d) = store_with_company().await; // inserts AAPL (cik "1")
        bootstrap_companies(
            &store,
            &[
                CompanyRef { cik: "320193".into(), ticker: "AAPL".into(), name: "Apple".into() },
                CompanyRef { cik: "320193".into(), ticker: "MSFT".into(), name: "Microsoft".into() },
            ],
        )
        .await
        .unwrap();
        let edgar = EdgarCollector::new(FakeHttp::new(EDGAR));
        let sources: [&dyn FactSource; 1] = [&edgar];

        let summary = collect_all(&store, &sources, &[], 0.05, fixed_now(), 2, None, 500_000_000.0, &NoProgress)
            .await
            .unwrap();

        assert_eq!(summary.companies, 2);
        assert!(summary.facts_written > 0);
        assert_eq!(summary.clone(), summary);
    }

    #[tokio::test]
    async fn collect_all_skips_recently_collected() {
        let (store, _id, _d) = store_with_company().await;
        bootstrap_companies(
            &store,
            &[CompanyRef { cik: "320193".into(), ticker: "AAPL".into(), name: "Apple".into() }],
        )
        .await
        .unwrap();
        let edgar = EdgarCollector::new(FakeHttp::new(EDGAR));
        let sources: [&dyn FactSource; 1] = [&edgar];
        let now = fixed_now();
        let cutoff = Some(now - ChronoDuration::hours(1));

        // First pass: never collected -> due -> collected (marked at `now`).
        let s1 = collect_all(&store, &sources, &[], 0.05, now, 2, cutoff, 500_000_000.0, &NoProgress).await.unwrap();
        assert_eq!(s1.companies, 1);
        // Second pass, same cutoff: collected at `now` (>= cutoff) -> skipped.
        let s2 = collect_all(&store, &sources, &[], 0.05, now, 2, cutoff, 500_000_000.0, &NoProgress).await.unwrap();
        assert_eq!(s2.companies, 0);
    }

    #[tokio::test]
    async fn collect_all_continues_past_a_failing_company() {
        let (store, _id, _d) = store_with_company().await; // 1 company (AAPL)
        assert_eq!(BadCompanyIdSource.name(), "bad");
        let sources: [&dyn FactSource; 1] = [&BadCompanyIdSource];
        let summary = collect_all(&store, &sources, &[], 0.05, fixed_now(), 2, None, 500_000_000.0, &NoProgress)
            .await
            .unwrap(); // pass did NOT abort
        assert_eq!(summary.companies, 0);
        assert_eq!(summary.failed, 1);
    }

    #[derive(Default)]
    struct CountProgress {
        start_calls: std::sync::atomic::AtomicUsize,
        started_total: std::sync::atomic::AtomicUsize,
        done: std::sync::atomic::AtomicUsize,
    }
    impl CollectProgress for CountProgress {
        fn start(&self, total: usize) {
            use std::sync::atomic::Ordering::SeqCst;
            self.start_calls.fetch_add(1, SeqCst);
            self.started_total.store(total, SeqCst);
        }
        fn company_done(&self, _done: usize, _total: usize, _ticker: &str, _ok: bool) {
            self.done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn collect_all_reports_progress() {
        use std::sync::atomic::Ordering::SeqCst;
        let (store, _id, _d) = store_with_company().await;
        bootstrap_companies(
            &store,
            &[
                CompanyRef { cik: "320193".into(), ticker: "AAPL".into(), name: "Apple".into() },
                CompanyRef { cik: "320193".into(), ticker: "MSFT".into(), name: "Microsoft".into() },
            ],
        )
        .await
        .unwrap();
        let edgar = EdgarCollector::new(FakeHttp::new(EDGAR));
        let sources: [&dyn FactSource; 1] = [&edgar];
        let p = CountProgress::default();

        let summary =
            collect_all(&store, &sources, &[], 0.05, fixed_now(), 2, None, 500_000_000.0, &p)
                .await
                .unwrap();

        assert_eq!(p.start_calls.load(SeqCst), 1); // start fired once
        assert_eq!(p.started_total.load(SeqCst), 2); // with the full count
        assert_eq!(p.done.load(SeqCst), 2); // one per company finished
        assert_eq!(summary.companies, 2);
    }

    /// A price source returning one valid bar for the requested company.
    struct GoodPriceSource;
    #[async_trait(?Send)]
    impl PriceSource for GoodPriceSource {
        fn name(&self) -> &'static str {
            "good"
        }
        async fn fetch_prices(
            &self,
            id: i64,
            _t: &SourceTarget,
        ) -> Result<Vec<PricePoint>, CollectorError> {
            Ok(vec![PricePoint {
                company_id: id,
                date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                open: None,
                high: None,
                low: None,
                close: 50.0,
                volume: None,
                source: "good".into(),
            }])
        }
    }

    #[tokio::test]
    async fn collect_collects_prices_so_graham_gets_valuation() {
        let (store, _id, _d) = store_with_company().await;
        bootstrap_companies(
            &store,
            &[CompanyRef { cik: "320193".into(), ticker: "AAPL".into(), name: "Apple".into() }],
        )
        .await
        .unwrap();
        let edgar = EdgarCollector::new(FakeHttp::new(EDGAR));
        let sources: [&dyn FactSource; 1] = [&edgar];
        let price_sources: [&dyn PriceSource; 1] = [&GoodPriceSource];
        assert_eq!(GoodPriceSource.name(), "good");

        collect_tickers(&store, &sources, &price_sources, &["AAPL".to_string()], 0.05, fixed_now(), 500_000_000.0)
            .await
            .unwrap();

        let id = store.get_company("AAPL").await.unwrap().unwrap().id;
        let price = store.latest_price(id).await.unwrap();
        assert!(price.is_some(), "price persisted by the collect path");
        // and that price reaches Graham: the P/E criterion is now computed
        // (price / EPS) instead of being reported as "insufficient data".
        let facts = store.get_facts(id).await.unwrap();
        let a = graham::assess(&facts, price, 500_000_000.0);
        let pe = a.criteria.iter().find(|c| c.name == "P/E <= 15").unwrap();
        assert_ne!(pe.detail, "insufficient data", "P/E computed from the collected price");
        // a Graham score row was persisted for the company
        assert!(store.get_graham_score(id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn collect_tickers_skips_a_failing_company() {
        let (store, _id, _d) = store_with_company().await; // AAPL exists
        let sources: [&dyn FactSource; 1] = [&BadCompanyIdSource];
        let outcomes = collect_tickers(
            &store,
            &sources,
            &[],
            &["AAPL".to_string()],
            0.05,
            fixed_now(),
            500_000_000.0,
        )
        .await
        .unwrap(); // did NOT abort
        assert!(outcomes.is_empty()); // the failing company was skipped
    }

    #[tokio::test]
    async fn collect_tickers_collects_known_and_skips_unknown() {
        let (store, _id, _d) = store_with_company().await;
        bootstrap_companies(
            &store,
            &[CompanyRef { cik: "320193".into(), ticker: "AAPL".into(), name: "Apple".into() }],
        )
        .await
        .unwrap();
        let edgar = EdgarCollector::new(FakeHttp::new(EDGAR));
        let sources: [&dyn FactSource; 1] = [&edgar];

        let outcomes = collect_tickers(
            &store,
            &sources,
            &[],
            &["AAPL".to_string(), "UNKNOWN".to_string()],
            0.05,
            fixed_now(),
            500_000_000.0,
        )
        .await
        .unwrap();

        assert_eq!(outcomes.len(), 1); // UNKNOWN skipped
        assert_eq!(outcomes[0].0, "AAPL");
        assert!(outcomes[0].1.facts_written > 0);
    }

    #[tokio::test]
    async fn ingest_records_source_errors_without_aborting() {
        let (store, id, _d) = store_with_company().await;
        let edgar = EdgarCollector::new(FakeHttp::new(EDGAR));
        let sources: [&dyn FactSource; 2] = [&FailingSource, &edgar];
        let target = SourceTarget {
            cik: "320193".into(),
            symbol: "AAPL".into(),
        };

        let report = ingest(&store, &sources, id, &target, 0.05, fixed_now())
            .await
            .unwrap();

        assert_eq!(report.source_errors.len(), 1);
        assert_eq!(report.source_errors[0].0, "boom");
        // the healthy EDGAR source still produced + persisted facts
        assert!(report.facts_written > 0);
    }

    fn target() -> SourceTarget {
        SourceTarget { cik: "320193".into(), symbol: "AAPL".into() }
    }

    #[tokio::test]
    async fn collect_prices_for_persists_and_skips_bad_source() {
        let (store, id, _d) = store_with_company().await;
        let good = FmpCollector::new(FakeHttp::new(FMP_PRICES), "K".into());
        let bad = FmpCollector::new(FakeHttp::new("nope"), "K".into()); // parse error -> warn
        assert_eq!(PriceSource::name(&good), "fmp");
        let sources: [&dyn PriceSource; 2] = [&good, &bad];
        let n = collect_prices_for(&store, &sources, id, &target()).await.unwrap();
        assert_eq!(n, 2);
        assert_eq!(store.get_prices(id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn collect_news_for_persists_and_skips_bad_source() {
        let (store, id, _d) = store_with_company().await;
        let good = FinnhubCollector::new(FakeHttp::new(NEWS_FINNHUB), "K".into());
        let bad = FinnhubCollector::new(FakeHttp::new("nope"), "K".into());
        assert_eq!(NewsSource::name(&good), "finnhub");
        let sources: [&dyn NewsSource; 2] = [&good, &bad];
        let n = collect_news_for(&store, &sources, id, &target(), fixed_now()).await.unwrap();
        assert_eq!(n, 2);
        assert_eq!(store.get_news(id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn recompute_ratios_persists_from_facts() {
        let (store, id, _d) = store_with_company().await;
        store
            .save_reconciled(&[fact(id, "edgar", "Revenue", 100.0), fact(id, "edgar", "NetIncome", 25.0)], &[])
            .await
            .unwrap();
        let n = recompute_ratios(&store, id, fixed_now()).await.unwrap();
        assert_eq!(n, 1); // net_margin
        assert_eq!(store.get_ratios(id, None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn collect_prices_all_iterates_and_records_failure() {
        let (store, id, _d) = store_with_company().await;
        let good = FmpCollector::new(FakeHttp::new(FMP_PRICES), "K".into());
        let ok_sources: [&dyn PriceSource; 1] = [&good];
        let s = collect_prices_all(&store, &ok_sources, Duration::ZERO).await.unwrap();
        assert_eq!(s.companies, 1);
        assert!(!store.get_prices(id).await.unwrap().is_empty());

        assert_eq!(BadPriceSource.name(), "badp");
        let bad_sources: [&dyn PriceSource; 1] = [&BadPriceSource];
        let s2 = collect_prices_all(&store, &bad_sources, Duration::ZERO).await.unwrap();
        assert_eq!(s2.failed, 1);
    }

    #[tokio::test]
    async fn collect_news_all_iterates_and_records_failure() {
        let (store, id, _d) = store_with_company().await;
        let good = FinnhubCollector::new(FakeHttp::new(NEWS_FINNHUB), "K".into());
        let ok_sources: [&dyn NewsSource; 1] = [&good];
        let s = collect_news_all(&store, &ok_sources, fixed_now(), Duration::ZERO).await.unwrap();
        assert_eq!(s.companies, 1);
        assert!(!store.get_news(id).await.unwrap().is_empty());

        assert_eq!(BadNewsSource.name(), "badn");
        let bad_sources: [&dyn NewsSource; 1] = [&BadNewsSource];
        let s2 = collect_news_all(&store, &bad_sources, fixed_now(), Duration::ZERO).await.unwrap();
        assert_eq!(s2.failed, 1);
    }

    #[tokio::test]
    async fn recompute_graham_persists_a_score() {
        let (store, id, _d) = store_with_company().await;
        store
            .save_reconciled(
                &[
                    fact(id, "edgar", "Revenue", 1_000_000_000.0),
                    fact(id, "edgar", "NetIncome", 100_000_000.0),
                    fact(id, "edgar", "CurrentAssets", 400.0),
                    fact(id, "edgar", "CurrentLiabilities", 100.0),
                    fact(id, "edgar", "TotalLiabilities", 150.0),
                    fact(id, "edgar", "StockholdersEquity", 1000.0),
                    fact(id, "edgar", "SharesOutstanding", 100.0),
                    fact(id, "edgar", "Eps", 2.0),
                ],
                &[],
            )
            .await
            .unwrap();
        recompute_graham(&store, id, 500_000_000.0, fixed_now()).await.unwrap();
        let score = store.get_graham_score(id).await.unwrap().unwrap();
        assert!(score.score >= 1);
        assert!(score.graham_number.is_some());
    }

    #[tokio::test]
    async fn recompute_ratios_all_iterates() {
        let (store, id, _d) = store_with_company().await;
        store
            .save_reconciled(&[fact(id, "edgar", "Revenue", 100.0), fact(id, "edgar", "NetIncome", 25.0)], &[])
            .await
            .unwrap();
        let s = recompute_ratios_all(&store, fixed_now()).await.unwrap();
        assert_eq!(s.companies, 1);
        assert_eq!(store.get_ratios(id, None).await.unwrap().len(), 1);
    }
}
