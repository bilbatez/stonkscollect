//! Ingest orchestration: reconcile multi-source facts and persist the
//! canonical values plus any flagged discrepancies.

use chrono::{DateTime, Utc};

use crate::collectors::edgar::CompanyRef;
use crate::collectors::{FactSource, SourceTarget};
use crate::domain::{FinancialFact, NewCompany};
use crate::reconcile::reconcile;
use crate::store::{Store, StoreError};

/// Upsert a batch of company identities (idempotent). Returns the count.
pub async fn bootstrap_companies(store: &Store, refs: &[CompanyRef]) -> Result<usize, StoreError> {
    for r in refs {
        store
            .upsert_company(&NewCompany {
                cik: r.cik.clone(),
                ticker: r.ticker.clone(),
                name: r.name.clone(),
                exchange: None,
                sector: None,
                industry: None,
            })
            .await?;
    }
    Ok(refs.len())
}

/// Aggregate totals from collecting many companies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectSummary {
    pub companies: usize,
    pub facts_written: usize,
    pub discrepancies_written: usize,
    pub source_errors: usize,
}

/// Collect every company in the store (the full US universe once bootstrapped),
/// sleeping `delay` between companies to stay polite to upstream APIs.
pub async fn collect_all(
    store: &Store,
    sources: &[&dyn FactSource],
    threshold: f64,
    now: DateTime<Utc>,
    delay: std::time::Duration,
) -> Result<CollectSummary, StoreError> {
    let companies = store.all_companies().await?;
    let mut summary = CollectSummary::default();
    for company in &companies {
        // Throttle between companies (not before the first).
        if summary.companies > 0 {
            tokio::time::sleep(delay).await;
        }
        let target = SourceTarget {
            cik: company.cik.clone(),
            symbol: company.ticker.clone(),
        };
        let report = ingest(store, sources, company.id, &target, threshold, now).await?;
        summary.companies += 1;
        summary.facts_written += report.facts_written;
        summary.discrepancies_written += report.discrepancies_written;
        summary.source_errors += report.source_errors.len();
    }
    Ok(summary)
}

/// Run [`ingest`] for each known ticker. Unknown tickers (not yet bootstrapped)
/// are skipped. Returns the per-ticker reports.
pub async fn collect_tickers(
    store: &Store,
    sources: &[&dyn FactSource],
    tickers: &[String],
    threshold: f64,
    now: DateTime<Utc>,
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
        let report = ingest(store, sources, company.id, &target, threshold, now).await?;
        outcomes.push((ticker.clone(), report));
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
    for fact in &result.canonical {
        store.upsert_fact(fact).await?;
    }
    for discrepancy in &result.discrepancies {
        store.insert_discrepancy(discrepancy).await?;
    }
    Ok((result.canonical.len(), result.discrepancies.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::edgar::EdgarCollector;
    use crate::collectors::fmp::FmpCollector;
    use crate::collectors::scrape::ScrapeCollector;
    use crate::collectors::CollectorError;
    use crate::domain::{NewCompany, PeriodType, StatementKind};
    use crate::testutil::{fixed_now, FakeHttp};
    use async_trait::async_trait;
    use chrono::{NaiveDate, TimeZone};

    const EDGAR: &str = include_str!("../tests/fixtures/edgar_companyfacts.json");
    const FMP_INCOME: &str = include_str!("../tests/fixtures/fmp_income.json");
    const SCRAPE_HTML: &str = include_str!("../tests/fixtures/scrape_financials.html");

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
        assert_eq!(FmpCollector::new(FakeHttp::new(""), "K".into()).name(), "fmp");
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

        let summary = collect_all(&store, &sources, 0.05, fixed_now(), std::time::Duration::ZERO)
            .await
            .unwrap();

        assert_eq!(summary.companies, 2);
        assert!(summary.facts_written > 0);
        assert_eq!(summary.clone(), summary);
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
            &["AAPL".to_string(), "UNKNOWN".to_string()],
            0.05,
            fixed_now(),
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
}
