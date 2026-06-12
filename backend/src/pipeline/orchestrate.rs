use super::*;

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
    news_sources: &[&dyn NewsSource],
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
                        // news, timestamp + per-company metric recompute.
                        let _ = collect_prices_for(store, price_sources, company.id, &target, now).await;
                        let _ = collect_news_for(store, news_sources, company.id, &target, now).await;
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
    news_sources: &[&dyn NewsSource],
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
                let _ = collect_prices_for(store, price_sources, company.id, &target, now).await;
                let _ = collect_news_for(store, news_sources, company.id, &target, now).await;
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

