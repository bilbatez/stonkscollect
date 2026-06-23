//! Binary entrypoint. Thin CLI glue only — all logic lives in the library.
//!
//!   stonkscollect serve              run the REST API
//!   stonkscollect bootstrap          fetch SEC ticker->CIK universe into the DB
//!   stonkscollect collect [--ticker] scrape + reconcile + persist, then exit

use std::net::SocketAddr;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use stonkscollect_backend::collectors::edgar::EdgarCollector;
use stonkscollect_backend::collectors::edgar_ownership::OwnershipCollector;
use stonkscollect_backend::collectors::fmp::FmpCollector;
use stonkscollect_backend::collectors::news::{FinnhubCollector, YahooNewsCollector};
use stonkscollect_backend::collectors::scrape::ScrapeCollector;
use stonkscollect_backend::collectors::yahoo::{YahooCollector, YahooProfileCollector};
use stonkscollect_backend::collectors::{
    FactSource, HolderSource, NewsSource, PriceSource, ProfileSource, SourceTarget,
};
use stonkscollect_backend::config::Config;
use stonkscollect_backend::http::ReqwestClient;
use stonkscollect_backend::net::{RateLimiter, RetryPolicy};
use stonkscollect_backend::scheduler::Tier;
use stonkscollect_backend::store::Store;
use stonkscollect_backend::{app, pipeline, scheduler};

/// Build a rate-limited, retrying HTTP client sharing `limiter`.
fn http_client(user_agent: &str, limiter: &Arc<RateLimiter>) -> ReqwestClient {
    ReqwestClient::with_limiter(user_agent, RetryPolicy::default(), Some(limiter.clone()))
}

/// Live, single-line progress for the `collect --all` CLI run.
struct CliProgress;
impl pipeline::CollectProgress for CliProgress {
    fn start(&self, total: usize) {
        if total == 0 {
            eprintln!("No companies in the database. Run `make bootstrap` first.");
        } else {
            eprintln!("Collecting {total} companies…");
        }
    }
    fn company_done(&self, done: usize, total: usize, ticker: &str, ok: bool) {
        let mark = if ok { "ok" } else { "FAIL" };
        // \r overwrites the line in place; \x1b[K clears any trailing chars.
        eprint!("\r[{done}/{total}] {ticker} {mark}\x1b[K");
        if done == total {
            eprintln!();
        }
    }
}

#[derive(Parser)]
#[command(name = "stonkscollect", about = "US-equity fundamental data collector")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the REST API server.
    Serve,
    /// Fetch SEC's ticker->CIK directory and upsert companies.
    Bootstrap,
    /// Collect data now, persist, and exit.
    Collect {
        /// Ticker to collect (repeatable). Defaults to the TICKERS env list.
        #[arg(long = "ticker")]
        tickers: Vec<String>,
        /// Collect the entire bootstrapped US universe (overrides --ticker).
        #[arg(long)]
        all: bool,
    },
    /// Enrich company profiles (description, sector/industry, website) from
    /// EDGAR + Yahoo. Keyless; idempotent.
    Enrich {
        #[arg(long = "ticker")]
        tickers: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    /// Dev only: seed an `admin`/`admin` login (idempotent). Never use in prod.
    SeedAdmin,
}

#[tokio::main]
async fn main() {
    // Load .env (searching cwd upward) if present; real env vars win. A malformed
    // line (e.g. an unquoted value with a space) aborts the whole load, leaving
    // later vars unset — surface that instead of failing silently. A missing
    // .env is fine (env may be set another way).
    if let Err(e) = dotenvy::dotenv() {
        if !e.not_found() {
            eprintln!("warning: .env was not fully loaded ({e}); some settings may be unset");
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Parse args first so --help/--version short-circuit before touching the DB.
    let command = Cli::parse().command;
    let cfg = Config::parse(|k| std::env::var(k).ok());
    // Shared Arc: axum state, the scheduler loop, and the collectors' HTTP
    // validator cache all hold the same store. A configured Turso URL takes
    // precedence over the local database_url.
    let (db_url, db_token) = match &cfg.turso_database_url {
        Some(url) => (url.as_str(), cfg.turso_auth_token.as_deref()),
        None => (cfg.database_url.as_str(), None),
    };
    let store = Arc::new(Store::open(db_url, db_token).await.expect("open database"));

    match command {
        Command::Serve => serve(store, &cfg).await,
        Command::Bootstrap => bootstrap(&store, &cfg).await,
        Command::Collect { tickers, all } => collect(&store, &cfg, tickers, all).await,
        Command::Enrich { tickers, all } => enrich(&store, &cfg, tickers, all).await,
        Command::SeedAdmin => seed_admin(&store).await,
    }
}

async fn serve(store: Arc<Store>, cfg: &Config) {
    // Ensure the market indices exist (idempotent) so /api/markets/summary works
    // even if `bootstrap` wasn't run with index seeding.
    let _ = pipeline::seed_indices(&store).await;
    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind listener");
    tracing::info!("listening on {addr}");

    // One shutdown signal, fanned out: a `watch` channel both ends await. On the
    // signal we flip the store flag (so in-flight bulk collection winds down) and
    // publish `true` so axum drains and the scheduler loop exits.
    let (tx, _rx) = tokio::sync::watch::channel(false);
    {
        let store = store.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            tracing::info!("shutdown requested; draining");
            store.request_shutdown();
            let _ = tx.send(true);
        });
    }

    // Run the API and the scheduler concurrently and wait for BOTH to wind down
    // gracefully (join, not select) — the scheduler finishes its in-flight
    // collection pass and exits instead of being dropped mid-pass.
    let mut axum_rx = tx.subscribe();
    let server = axum::serve(listener, app(store.clone()))
        .with_graceful_shutdown(async move {
            let _ = axum_rx.wait_for(|down| *down).await;
        });
    let (result, ()) = tokio::join!(server, scheduler_loop(&store, cfg, tx.subscribe()));
    result.expect("server error");
    tracing::info!("shut down");
}

/// Resolves when the process receives Ctrl-C or (on Unix) SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

/// Background loop: sleep until the next tier fires, then collect for that tier
/// (Fundamentals = facts + ratios, Price = prices, News = news). Operates on the
/// whole universe if COLLECT_ALL, else the configured tickers; idle when neither.
async fn scheduler_loop(
    store: &Arc<Store>,
    cfg: &Config,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    if !cfg.collect_all && cfg.tickers.is_empty() {
        tracing::info!("no TICKERS and COLLECT_ALL unset; collection loop idle");
        // Wait for shutdown (then exit) instead of blocking forever, so the
        // process can stop cleanly even with collection disabled.
        let _ = shutdown.wait_for(|down| *down).await;
        return;
    }

    // One rate limiter PER HOST (REQUEST_DELAY_MS spacing each) so EDGAR, Yahoo
    // and the vendors throttle independently and run in parallel.
    let delay = std::time::Duration::from_millis(cfg.request_delay_ms);
    let mk = || Arc::new(RateLimiter::new(delay));

    let edgar =
        EdgarCollector::new(http_client(&cfg.user_agent, &mk())).with_cache(store.clone());
    let yahoo_lim = mk(); // shared by Yahoo prices + Yahoo news
    let yahoo = YahooCollector::new(http_client(&cfg.user_agent, &yahoo_lim));
    let mut fact_sources: Vec<&dyn FactSource> = vec![&edgar];
    // Yahoo is keyless, so prices work without any API key.
    let mut price_sources: Vec<&dyn PriceSource> = vec![&yahoo];
    let fmp;
    if let Some(key) = &cfg.fmp_api_key {
        fmp = FmpCollector::new(http_client(&cfg.user_agent, &mk()), key.clone());
        fact_sources.push(&fmp);
        price_sources.push(&fmp);
    }
    let scrape; // scrape only for targeted runs (not the whole universe)
    if !cfg.collect_all {
        scrape = ScrapeCollector::new(http_client(&cfg.user_agent, &mk()));
        fact_sources.push(&scrape);
    }
    let yahoo_news = YahooNewsCollector::new(http_client(&cfg.user_agent, &yahoo_lim));
    // Keyless per-company news; Finnhub adds more when a key is set.
    let mut news_sources: Vec<&dyn NewsSource> = vec![&yahoo_news];
    let finnhub;
    if let Some(key) = &cfg.finnhub_api_key {
        finnhub = FinnhubCollector::new(http_client(&cfg.user_agent, &mk()), key.clone());
        news_sources.push(&finnhub);
    }
    // EDGAR submissions + Yahoo assetProfile carry sector/industry/website.
    let yahoo_profile = YahooProfileCollector::new(http_client(&cfg.user_agent, &yahoo_lim));
    let profile_sources: Vec<&dyn ProfileSource> = vec![&edgar, &yahoo_profile];

    while let Some((tier, at)) = scheduler::next_tier(chrono::Utc::now()) {
        if *shutdown.borrow() {
            break;
        }
        let now = chrono::Utc::now();
        let wait = (at - now).to_std().unwrap_or(std::time::Duration::ZERO);
        tracing::info!("next collection: {} at {at}", tier.label());
        // Interruptible sleep: wake immediately on shutdown instead of waiting
        // out the (possibly hours-long) inter-tier delay.
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = shutdown.wait_for(|down| *down) => break,
        }
        let fired = chrono::Utc::now();
        let label = tier.label();

        let result = scheduler::run_tracked(store, label, None, fired, || async {
            match tier {
                Tier::Fundamentals => {
                    collect_fundamentals(store, cfg, &fact_sources, &price_sources, &profile_sources, fired).await
                }
                Tier::Price => collect_prices(store, cfg, &price_sources, fired).await,
                Tier::News => collect_news(store, cfg, &news_sources, fired).await,
                Tier::Parquet => pipeline::export_all_parquet(store, &cfg.parquet_dir).await,
            }
        })
        .await;
        report_bulk(label, result);
    }
}

/// Collect facts (universe or tickers) then recompute ratios.
async fn collect_fundamentals(
    store: &Store,
    cfg: &Config,
    sources: &[&dyn FactSource],
    price_sources: &[&dyn PriceSource],
    profile_sources: &[&dyn ProfileSource],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<pipeline::CollectSummary, stonkscollect_backend::store::StoreError> {
    let cutoff = cfg
        .collect_max_age_hrs
        .map(|h| now - chrono::Duration::hours(h as i64));
    // News is handled by the dedicated News tier in serve.
    let collect_sources = pipeline::CollectSources {
        facts: sources,
        prices: price_sources,
        news: &[],
        profiles: profile_sources,
    };
    let options = collect_options(cfg, now, cutoff);
    // Prices, ratios + Graham scores are recomputed per company inside collect_*
    // (only for companies actually collected this pass), not the whole universe.
    if cfg.collect_all {
        pipeline::collect_all(store, &collect_sources, &options, &pipeline::NoProgress).await
    } else {
        let outcomes =
            pipeline::collect_tickers(store, &collect_sources, &cfg.tickers, &options).await?;
        let mut s = pipeline::CollectSummary::default();
        for (_t, r) in &outcomes {
            s.companies += 1;
            s.facts_written += r.facts_written;
        }
        Ok(s)
    }
}

fn collect_options(
    cfg: &Config,
    now: chrono::DateTime<chrono::Utc>,
    cutoff: Option<chrono::DateTime<chrono::Utc>>,
) -> pipeline::CollectOptions {
    pipeline::CollectOptions {
        threshold: cfg.reconcile_threshold,
        now,
        concurrency: cfg.collect_concurrency,
        cutoff,
        min_revenue: cfg.graham_min_revenue,
    }
}

async fn collect_prices(
    store: &Store,
    cfg: &Config,
    sources: &[&dyn PriceSource],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<pipeline::CollectSummary, stonkscollect_backend::store::StoreError> {
    if cfg.collect_all {
        return pipeline::collect_prices_all(store, sources, now, cfg.collect_concurrency).await;
    }
    let mut s = pipeline::CollectSummary::default();
    for ticker in &cfg.tickers {
        if let Some(c) = store.get_company(ticker).await? {
            let t = SourceTarget { cik: c.cik.clone(), symbol: c.ticker.clone() };
            s.facts_written += pipeline::collect_prices_for(store, sources, c.id, &t, now).await?;
            s.companies += 1;
        }
    }
    Ok(s)
}

async fn collect_news(
    store: &Store,
    cfg: &Config,
    sources: &[&dyn NewsSource],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<pipeline::CollectSummary, stonkscollect_backend::store::StoreError> {
    if cfg.collect_all {
        return pipeline::collect_news_all(store, sources, now, cfg.collect_concurrency).await;
    }
    let mut s = pipeline::CollectSummary::default();
    for ticker in &cfg.tickers {
        if let Some(c) = store.get_company(ticker).await? {
            let t = SourceTarget { cik: c.cik.clone(), symbol: c.ticker.clone() };
            s.facts_written += pipeline::collect_news_for(store, sources, c.id, &t, now).await?;
            s.companies += 1;
        }
    }
    Ok(s)
}

fn report_bulk(label: &str, result: Result<pipeline::CollectSummary, stonkscollect_backend::store::StoreError>) {
    match result {
        Ok(s) => tracing::info!(
            "{label} tier: {} companies, {} facts, {} discrepancies, {} source errors, {} failed",
            s.companies, s.facts_written, s.discrepancies_written, s.source_errors, s.failed
        ),
        Err(e) => tracing::error!("{label} tier failed: {e}"),
    }
}

/// Enrich company profiles from EDGAR (industry/exchange), FMP when a key is
/// configured (employees/exchange), and Yahoo (description/website/sector).
/// Idempotent (COALESCE update).
async fn enrich(store: &Arc<Store>, cfg: &Config, mut tickers: Vec<String>, all: bool) {
    let bulk = all || cfg.collect_all;
    let delay = std::time::Duration::from_millis(cfg.request_delay_ms);
    let mk = || Arc::new(RateLimiter::new(delay));
    let edgar =
        EdgarCollector::new(http_client(&cfg.user_agent, &mk())).with_cache(store.clone());
    let yahoo = YahooProfileCollector::new(http_client(&cfg.user_agent, &mk()));
    let fmp = cfg
        .fmp_api_key
        .clone()
        .map(|key| FmpCollector::new(http_client(&cfg.user_agent, &mk()), key));
    // EDGAR first (canonical industry/exchange), FMP fills headcount, Yahoo
    // overlays prose/website/sector last.
    let mut profile_sources: Vec<&dyn ProfileSource> = vec![&edgar];
    if let Some(fmp) = &fmp {
        profile_sources.push(fmp);
    }
    profile_sources.push(&yahoo);

    if bulk {
        let s = pipeline::enrich_all(store, &profile_sources, cfg.collect_concurrency, &CliProgress)
            .await
            .expect("enrich all");
        report_bulk("enrich", Ok(s));
    } else {
        if tickers.is_empty() {
            tickers = cfg.tickers.clone();
        }
        tickers.iter_mut().for_each(|t| *t = t.to_uppercase());
        let n = pipeline::enrich_tickers(store, &profile_sources, &tickers)
            .await
            .expect("enrich tickers");
        tracing::info!("enriched {n} companies");
    }
    // Close the pool so the one-shot process exits (see bootstrap).
    store.close().await;
}

/// Dev convenience: ensure an admin login exists so a developer can sign straight
/// into the dashboard. The email form field requires a valid address, so the
/// login is `admin@admin.com` / `admin`. Insecure by design — development only.
async fn seed_admin(store: &Store) {
    let created = pipeline::ensure_user(store, "admin@admin.com", "admin")
        .await
        .expect("seed admin user");
    if created {
        tracing::warn!("seeded DEV login: admin@admin.com / admin (insecure — dev only)");
    } else {
        tracing::info!("admin user already exists; left unchanged");
    }
    // Close the pool so the one-shot process exits (see bootstrap).
    store.close().await;
}

async fn bootstrap(store: &Store, cfg: &Config) {
    let limiter = Arc::new(RateLimiter::new(std::time::Duration::from_millis(cfg.request_delay_ms)));
    let edgar = EdgarCollector::new(http_client(&cfg.user_agent, &limiter));
    let refs = edgar.collect_company_tickers().await.expect("fetch tickers");
    let n = pipeline::bootstrap_companies(store, &refs)
        .await
        .expect("bootstrap companies");
    let idx = pipeline::seed_indices(store).await.expect("seed indices");
    tracing::info!("bootstrapped {n} companies and {idx} indices");
    // Close the sqlx pool so the one-shot process exits instead of hanging on
    // SQLite's non-daemon background connection threads.
    store.close().await;
}

async fn collect(store: &Arc<Store>, cfg: &Config, mut tickers: Vec<String>, all: bool) {
    // An explicit `--ticker` on the CLI collects just those tickers even when the
    // COLLECT_ALL env default is set (otherwise `collect --ticker AAPL` silently
    // ran the whole ~10k universe).
    let bulk = cfg.collect_is_bulk(all, &tickers);

    // One rate limiter PER HOST (each client owns its clone), so EDGAR, Yahoo and
    // the vendors throttle independently and actually run in parallel under
    // COLLECT_CONCURRENCY. REQUEST_DELAY_MS is the per-host spacing.
    let delay = std::time::Duration::from_millis(cfg.request_delay_ms);
    let mk = || Arc::new(RateLimiter::new(delay));
    let edgar =
        EdgarCollector::new(http_client(&cfg.user_agent, &mk())).with_cache(store.clone());
    let yahoo_lim = mk(); // shared by Yahoo prices + Yahoo news (same provider)
    let yahoo = YahooCollector::new(http_client(&cfg.user_agent, &yahoo_lim));
    let mut sources: Vec<&dyn FactSource> = vec![&edgar];
    // Keyless prices via Yahoo so P/E, P/B and the screener populate without keys.
    let mut price_sources: Vec<&dyn PriceSource> = vec![&yahoo];
    // Keyless per-company news via Yahoo's per-symbol RSS.
    let yahoo_news = YahooNewsCollector::new(http_client(&cfg.user_agent, &yahoo_lim));
    let mut news_sources: Vec<&dyn NewsSource> = vec![&yahoo_news];
    let fmp;
    if let Some(key) = &cfg.fmp_api_key {
        fmp = FmpCollector::new(http_client(&cfg.user_agent, &mk()), key.clone());
        sources.push(&fmp);
        price_sources.push(&fmp);
    }
    let finnhub;
    if let Some(key) = &cfg.finnhub_api_key {
        finnhub = FinnhubCollector::new(http_client(&cfg.user_agent, &mk()), key.clone());
        news_sources.push(&finnhub);
    }
    let scrape;
    if !bulk {
        scrape = ScrapeCollector::new(http_client(&cfg.user_agent, &mk()));
        sources.push(&scrape);
    }

    let now = chrono::Utc::now();
    let yahoo_profile = YahooProfileCollector::new(http_client(&cfg.user_agent, &yahoo_lim));
    let profile_sources: Vec<&dyn ProfileSource> = vec![&edgar, &yahoo_profile];
    let collect_sources = pipeline::CollectSources {
        facts: &sources,
        prices: &price_sources,
        news: &news_sources,
        profiles: &profile_sources,
    };
    // One-shot CLI always collects (no freshness cutoff).
    let options = collect_options(cfg, now, None);
    // collect_* collect prices then recompute ratios + Graham per collected company.
    if bulk {
        let s = pipeline::collect_all(store, &collect_sources, &options, &CliProgress)
            .await
            .expect("collect all");
        report_bulk("collect", Ok(s));
    } else {
        if tickers.is_empty() {
            tickers = cfg.tickers.clone();
        }
        tickers.iter_mut().for_each(|t| *t = t.to_uppercase());
        let outcomes = pipeline::collect_tickers(store, &collect_sources, &tickers, &options)
            .await
            .expect("collect tickers");
        for (ticker, report) in outcomes {
            tracing::info!(
                "{ticker}: {} facts, {} discrepancies, {} source errors",
                report.facts_written,
                report.discrepancies_written,
                report.source_errors.len()
            );
        }
    }

    // Best-effort insider ownership from EDGAR Form 4 (keyless). Separate from
    // the fact/price pipeline: holdings are not period-keyed facts.
    let holders = OwnershipCollector::new(http_client(&cfg.user_agent, &mk()));
    let companies = if bulk {
        store.all_companies().await.unwrap_or_default()
    } else {
        let mut cs = Vec::new();
        for t in &tickers {
            if let Ok(Some(c)) = store.get_company(t).await {
                cs.push(c);
            }
        }
        cs
    };
    for c in &companies {
        let target = SourceTarget { cik: c.cik.clone(), symbol: c.ticker.clone() };
        match holders.fetch_holders(c.id, &target).await {
            Ok(h) if !h.is_empty() => {
                if let Err(e) = store.save_ownership(&h).await {
                    tracing::warn!("save ownership for {} failed: {e}", c.ticker);
                }
            }
            Ok(_) => {}
            Err(e) => tracing::debug!("form 4 collection for {} failed: {e}", c.ticker),
        }
    }

    // Close the pool so the one-shot process exits (see bootstrap).
    store.close().await;
}
