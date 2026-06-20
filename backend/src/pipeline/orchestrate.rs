use super::*;
use chrono::NaiveDate;

/// Collect every company in the store (the full US universe once bootstrapped).
///
/// A failure on one company is recorded in `summary.failed` and skipped — one
/// bad company never aborts a full pass.
///
/// When `options.cutoff` is `Some`, only companies not collected since then are
/// processed (incremental); successful collections are timestamped.
///
/// Up to `options.concurrency` companies are fetched at once; politeness is
/// enforced by the shared rate limiter in the HTTP client, not a sleep.
pub async fn collect_all(
    store: &Store,
    sources: &CollectSources<'_>,
    options: &CollectOptions,
    progress: &dyn CollectProgress,
) -> Result<CollectSummary, StoreError> {
    let companies = match options.cutoff {
        Some(c) => store.companies_due(c).await?,
        None => store.all_companies().await?,
    };

    let total = companies.len();
    progress.start(total);
    let counter = std::sync::atomic::AtomicUsize::new(0);

    let outcomes = futures::stream::iter(companies)
        // Graceful shutdown: stop launching new companies; in-flight ones
        // (already buffered) finish, then the stream completes.
        .take_while(|_| {
            let go = !store.is_shutting_down();
            async move { go }
        })
        .map(|company| {
            let counter = &counter;
            async move {
                let done_so_far = || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                match collect_company(store, sources, options, &company).await {
                    Ok(report) => {
                        progress.company_done(done_so_far(), total, &company.ticker, true);
                        Ok(report)
                    }
                    Err(e) => {
                        tracing::warn!("collect failed for {}: {e}", company.ticker);
                        progress.company_done(done_so_far(), total, &company.ticker, false);
                        Err(())
                    }
                }
            }
        })
        .buffer_unordered(options.concurrency.max(1))
        .collect::<Vec<_>>()
        .await;

    let mut summary = CollectSummary::default();
    for outcome in outcomes {
        match outcome {
            Ok(report) => {
                summary.companies += 1;
                summary.facts_written += report.facts_written;
                summary.discrepancies_written += report.discrepancies_written;
                summary.source_errors += report.source_errors.len();
            }
            Err(()) => summary.failed += 1,
        }
    }
    Ok(summary)
}

/// A company whose last price is older than this (and that EDGAR can no longer
/// find) is treated as delisted.
const DELISTED_STALE_DAYS: i64 = 30;

/// Decide a company's listing status from one collect run's signals.
///
/// Conservative to avoid false positives from transient outages: only
/// `"delisted"` when EDGAR reports the CIK missing (404) **and** no fresh prices
/// arrived this run **and** the newest stored price is stale (or none exists).
/// Otherwise `"active"`, which self-heals a company once data returns.
fn delisting_status(
    edgar_missing: bool,
    new_prices: usize,
    latest_price_date: Option<NaiveDate>,
    now: DateTime<Utc>,
) -> &'static str {
    let prices_stale = match latest_price_date {
        None => true,
        Some(d) => (now.date_naive() - d).num_days() > DELISTED_STALE_DAYS,
    };
    if edgar_missing && new_prices == 0 && prices_stale {
        "delisted"
    } else {
        "active"
    }
}

/// One company's full collect: facts ingest, then best-effort prices (first,
/// so Graham sees a price), news, collection timestamp, metric recompute, and a
/// listing-status (active/delisted) refresh.
async fn collect_company(
    store: &Store,
    sources: &CollectSources<'_>,
    options: &CollectOptions,
    company: &Company,
) -> Result<IngestReport, StoreError> {
    let target = target_of(company);
    let report =
        ingest(store, sources.facts, company.id, &target, options.threshold, options.now).await?;
    let new_prices =
        collect_prices_for(store, sources.prices, company.id, &target, options.now)
            .await
            .unwrap_or(0);
    let _ = collect_news_for(store, sources.news, company.id, &target, options.now).await;
    // Fill in sector/industry/website once (best-effort) so the directory and
    // sectors page populate without a separate `enrich` run.
    if company.sector.is_none() && !sources.profiles.is_empty() {
        let _ = super::enrich::enrich_company(store, sources.profiles, company).await;
    }
    let _ = store.mark_collected(company.id, options.now).await;
    let _ = recompute_metrics(store, company.id, options.min_revenue, options.now).await;
    // Index pseudo-companies (synthetic `IDX-` CIK) have no EDGAR facts; never
    // flag them delisted.
    if !company.cik.starts_with("IDX-") {
        let edgar_missing = report
            .source_errors
            .iter()
            .any(|(s, m)| s == "edgar" && m.contains("404"));
        let latest = store.latest_price_date_any(company.id).await.unwrap_or(None);
        let status = delisting_status(edgar_missing, new_prices, latest, options.now);
        let _ = store.set_company_status(company.id, status).await;
    }
    Ok(report)
}

/// Run a full collect for each known ticker. Unknown tickers (not yet
/// bootstrapped) are skipped. Returns the per-ticker reports.
pub async fn collect_tickers(
    store: &Store,
    sources: &CollectSources<'_>,
    tickers: &[String],
    options: &CollectOptions,
) -> Result<Vec<(String, IngestReport)>, StoreError> {
    let mut outcomes = Vec::new();
    for ticker in tickers {
        // Graceful shutdown: stop before starting the next ticker.
        if store.is_shutting_down() {
            break;
        }
        let Some(company) = store.get_company(ticker).await? else {
            continue;
        };
        match collect_company(store, sources, options, &company).await {
            Ok(report) => outcomes.push((ticker.clone(), report)),
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
///
/// Sources that report nothing this run (304 Not Modified, errors, missing
/// keys) are represented by their stored facts, so an already-canonical value
/// keeps outranking a fresh value from a lesser source.
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
    store.save_source_errors(company_id, &source_errors, now).await?;
    supplement_with_stored_facts(store, company_id, &mut all_facts).await?;
    let (facts_written, discrepancies_written) =
        persist_facts(store, &all_facts, threshold, now).await?;
    Ok(IngestReport {
        facts_written,
        discrepancies_written,
        source_errors,
    })
}

/// Add a company's stored facts for every source absent from `fresh`, so
/// reconciliation always sees each source's latest known values.
async fn supplement_with_stored_facts(
    store: &Store,
    company_id: i64,
    fresh: &mut Vec<FinancialFact>,
) -> Result<(), StoreError> {
    let fresh_sources: std::collections::HashSet<String> =
        fresh.iter().map(|f| f.source.clone()).collect();
    let stored = store.get_facts(company_id).await?;
    fresh.extend(stored.into_iter().filter(|f| !fresh_sources.contains(&f.source)));
    Ok(())
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
    store.save_shares(&dei_share_counts(&result.canonical)).await?;
    Ok((result.canonical.len(), result.discrepancies.len()))
}

/// Extract DEI cover-page share counts from canonical facts, so the
/// `shares_outstanding` history fills in alongside the facts table.
fn dei_share_counts(facts: &[FinancialFact]) -> Vec<ShareCount> {
    facts
        .iter()
        .filter(|f| f.line_item == "SharesOutstandingDei")
        .map(|f| ShareCount {
            company_id: f.company_id,
            as_of: f.period_end,
            shares: f.value,
            source: f.source.clone(),
        })
        .collect()
}

#[cfg(test)]
mod delisting_tests {
    use super::delisting_status;
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};

    fn now() -> DateTime<Utc> {
        chrono::Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap()
    }

    #[test]
    fn delisted_only_when_missing_no_new_and_stale() {
        let old = NaiveDate::from_ymd_opt(2024, 1, 1); // >30d before 2024-06-01
        let recent = NaiveDate::from_ymd_opt(2024, 5, 28); // within 30d
        assert_eq!(delisting_status(true, 0, None, now()), "delisted");
        assert_eq!(delisting_status(true, 0, old, now()), "delisted");
        assert_eq!(delisting_status(true, 3, old, now()), "active");
        assert_eq!(delisting_status(false, 0, old, now()), "active");
        assert_eq!(delisting_status(true, 0, recent, now()), "active");
    }
}

