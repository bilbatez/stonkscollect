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
