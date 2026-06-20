use super::*;

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

pub(crate) fn target_of(c: &Company) -> SourceTarget {
    SourceTarget {
        cik: c.cik.clone(),
        symbol: c.ticker.clone(),
    }
}

/// Days re-fetched before the last stored price, so upstream revisions of
/// recent bars (splits, late corrections) are picked up by the overlap.
const PRICE_REFRESH_OVERLAP_DAYS: i64 = 7;

/// Gather prices from all sources for one company (best-effort) and persist.
/// Each source is asked only for bars since its last stored date (minus a
/// small overlap); a source with no stored prices backfills its full window.
pub async fn collect_prices_for(
    store: &Store,
    sources: &[&dyn PriceSource],
    company_id: i64,
    target: &SourceTarget,
    now: DateTime<Utc>,
) -> Result<usize, StoreError> {
    let mut all = Vec::new();
    for s in sources {
        let since = store
            .latest_price_date(company_id, s.name())
            .await?
            .map(|last| last - chrono::Duration::days(PRICE_REFRESH_OVERLAP_DAYS));
        match s.fetch_prices(company_id, target, now, since).await {
            Ok(p) => all.extend(p),
            // Best-effort: a missing/delisted ticker (e.g. Yahoo 404) is expected
            // across a 10k-company run, so log at debug, not warn.
            Err(e) => tracing::debug!("price source {} failed: {e}", s.name()),
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
            Err(e) => tracing::debug!("news source {} failed: {e}", s.name()),
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

/// Collect prices for every company (parallel, per-company isolation;
/// politeness comes from each client's shared per-host rate limiter).
pub async fn collect_prices_all(
    store: &Store,
    sources: &[&dyn PriceSource],
    now: DateTime<Utc>,
    concurrency: usize,
) -> Result<CollectSummary, StoreError> {
    let companies = store.all_companies().await?;
    let outcomes = futures::stream::iter(companies)
        .take_while(|_| {
            let go = !store.is_shutting_down();
            async move { go }
        })
        .map(|c| async move {
            collect_prices_for(store, sources, c.id, &target_of(&c), now)
                .await
                .map_err(|e| tracing::warn!("prices failed for {}: {e}", c.ticker))
        })
        .buffer_unordered(concurrency.max(1))
        .collect::<Vec<_>>()
        .await;
    Ok(summarize(outcomes))
}

/// Collect news for every company (parallel, per-company isolation;
/// politeness comes from each client's shared per-host rate limiter).
pub async fn collect_news_all(
    store: &Store,
    sources: &[&dyn NewsSource],
    now: DateTime<Utc>,
    concurrency: usize,
) -> Result<CollectSummary, StoreError> {
    let companies = store.all_companies().await?;
    let outcomes = futures::stream::iter(companies)
        .take_while(|_| {
            let go = !store.is_shutting_down();
            async move { go }
        })
        .map(|c| async move {
            collect_news_for(store, sources, c.id, &target_of(&c), now)
                .await
                .map_err(|e| tracing::warn!("news failed for {}: {e}", c.ticker))
        })
        .buffer_unordered(concurrency.max(1))
        .collect::<Vec<_>>()
        .await;
    Ok(summarize(outcomes))
}

/// Fold per-company outcomes (items written, or a logged failure) into totals.
fn summarize(outcomes: Vec<Result<usize, ()>>) -> CollectSummary {
    let mut s = CollectSummary::default();
    for outcome in outcomes {
        match outcome {
            Ok(written) => {
                s.companies += 1;
                s.facts_written += written;
            }
            Err(()) => s.failed += 1,
        }
    }
    s
}

/// Export every company's stored prices as one Parquet file per ticker under
/// `dir` (created if missing). Per-company failures are counted, not fatal.
pub async fn export_all_parquet(
    store: &Store,
    dir: &std::path::Path,
) -> Result<CollectSummary, StoreError> {
    std::fs::create_dir_all(dir)
        .map_err(|e| StoreError::Other(format!("create {}: {e}", dir.display())))?;
    let companies = store.all_companies().await?;
    let mut s = CollectSummary::default();
    for c in &companies {
        // Graceful shutdown: stop before exporting the next company.
        if store.is_shutting_down() {
            break;
        }
        let filename = format!("{}.parquet", c.ticker.replace('/', "-"));
        match store.export_prices_parquet(c.id, &dir.join(filename)).await {
            Ok(()) => s.companies += 1,
            Err(e) => {
                tracing::warn!("parquet export failed for {}: {e}", c.ticker);
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
