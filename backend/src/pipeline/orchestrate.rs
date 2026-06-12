use super::*;

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

/// One company's full collect: facts ingest, then best-effort prices (first,
/// so Graham sees a price), news, collection timestamp, and metric recompute.
async fn collect_company(
    store: &Store,
    sources: &CollectSources<'_>,
    options: &CollectOptions,
    company: &Company,
) -> Result<IngestReport, StoreError> {
    let target = target_of(company);
    let report =
        ingest(store, sources.facts, company.id, &target, options.threshold, options.now).await?;
    let _ = collect_prices_for(store, sources.prices, company.id, &target, options.now).await;
    let _ = collect_news_for(store, sources.news, company.id, &target, options.now).await;
    let _ = store.mark_collected(company.id, options.now).await;
    let _ = recompute_metrics(store, company.id, options.min_revenue, options.now).await;
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

