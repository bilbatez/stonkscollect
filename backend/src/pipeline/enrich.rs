use super::*;

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

/// Enrich one company's profile from all profile sources (best-effort), merging
/// in source order (later non-`None` fields win) and persisting via COALESCE.
pub async fn enrich_company(
    store: &Store,
    profile_sources: &[&dyn ProfileSource],
    company: &Company,
) -> Result<(), StoreError> {
    let target = target_of(company);
    let mut profile = CompanyProfile::default();
    for src in profile_sources {
        match src.fetch_profile(&target).await {
            Ok(p) => profile = profile.overlay(p),
            Err(e) => tracing::debug!("profile source {} failed for {}: {e}", src.name(), company.ticker),
        }
    }
    store.update_company_profile(company.id, &profile).await
}

/// Enrich every company's profile (parallel, best-effort).
pub async fn enrich_all(
    store: &Store,
    profile_sources: &[&dyn ProfileSource],
    concurrency: usize,
    progress: &dyn CollectProgress,
) -> Result<CollectSummary, StoreError> {
    let companies = store.all_companies().await?;
    let total = companies.len();
    progress.start(total);
    let counter = std::sync::atomic::AtomicUsize::new(0);

    let outcomes = futures::stream::iter(companies)
        .map(|company| {
            let counter = &counter;
            async move {
                let ok = enrich_company(store, profile_sources, &company).await.is_ok();
                let done = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                progress.company_done(done, total, &company.ticker, ok);
                ok
            }
        })
        .buffer_unordered(concurrency.max(1))
        .collect::<Vec<_>>()
        .await;

    let mut s = CollectSummary::default();
    for ok in outcomes {
        s.companies += usize::from(ok);
        s.failed += usize::from(!ok);
    }
    Ok(s)
}

/// Enrich an explicit ticker list (unknown tickers skipped). Returns the count enriched.
pub async fn enrich_tickers(
    store: &Store,
    profile_sources: &[&dyn ProfileSource],
    tickers: &[String],
) -> Result<usize, StoreError> {
    let mut n = 0;
    for ticker in tickers {
        if let Some(c) = store.get_company(ticker).await? {
            let _ = enrich_company(store, profile_sources, &c).await;
            n += 1;
        }
    }
    Ok(n)
}

/// Market indices tracked for the dashboard summary, as (Yahoo chart symbol, name).
pub const TRACKED_INDICES: &[(&str, &str)] = &[
    ("^GSPC", "S&P 500"),
    ("^IXIC", "Nasdaq Composite"),
    ("^DJI", "Dow Jones Industrial Average"),
];

/// Seed the tracked market indices as pseudo-companies (idempotent). They are
/// collected like equities but hidden from the directory/movers. Returns count.
pub async fn seed_indices(store: &Store) -> Result<usize, StoreError> {
    for (ticker, name) in TRACKED_INDICES {
        store.upsert_index(ticker, name).await?;
    }
    Ok(TRACKED_INDICES.len())
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
