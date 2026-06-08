//! Binary entrypoint. Thin CLI glue only — all logic lives in the library.
//!
//!   stonkscollect serve              run the REST API
//!   stonkscollect bootstrap          fetch SEC ticker->CIK universe into the DB
//!   stonkscollect collect [--ticker] scrape + reconcile + persist, then exit

use std::net::SocketAddr;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use stonkscollect_backend::collectors::edgar::EdgarCollector;
use stonkscollect_backend::collectors::fmp::FmpCollector;
use stonkscollect_backend::collectors::news::FinnhubCollector;
use stonkscollect_backend::collectors::scrape::ScrapeCollector;
use stonkscollect_backend::collectors::{FactSource, NewsSource, PriceSource, SourceTarget};
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
}

#[tokio::main]
async fn main() {
    // Load .env (searching cwd upward) if present; real env vars win.
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Parse args first so --help/--version short-circuit before touching the DB.
    let command = Cli::parse().command;
    let cfg = Config::parse(|k| std::env::var(k).ok());
    let store = Store::connect(&cfg.database_url).await.expect("open database");

    match command {
        Command::Serve => serve(store, &cfg).await,
        Command::Bootstrap => bootstrap(&store, &cfg).await,
        Command::Collect { tickers, all } => collect(&store, &cfg, tickers, all).await,
    }
}

async fn serve(store: Store, cfg: &Config) {
    let store = Arc::new(store);
    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind listener");
    tracing::info!("listening on {addr}");

    // Serve the API and run the tiered collection loop on the same task.
    tokio::select! {
        result = axum::serve(listener, app(store.clone())) => result.expect("server error"),
        _ = scheduler_loop(&store, cfg) => {}
    }
}

/// Background loop: sleep until the next tier fires, then collect for that tier
/// (Fundamentals = facts + ratios, Price = prices, News = news). Operates on the
/// whole universe if COLLECT_ALL, else the configured tickers; idle when neither.
async fn scheduler_loop(store: &Store, cfg: &Config) {
    if !cfg.collect_all && cfg.tickers.is_empty() {
        tracing::info!("no TICKERS and COLLECT_ALL unset; collection loop idle");
        std::future::pending::<()>().await;
    }

    // One shared rate limiter keeps total request rate polite. One FMP instance
    // serves both the FactSource and PriceSource roles.
    let limiter = Arc::new(RateLimiter::new(std::time::Duration::from_millis(cfg.request_delay_ms)));
    let delay = std::time::Duration::from_millis(cfg.request_delay_ms);

    let edgar = EdgarCollector::new(http_client(&cfg.user_agent, &limiter));
    let mut fact_sources: Vec<&dyn FactSource> = vec![&edgar];
    let fmp;
    let mut price_sources: Vec<&dyn PriceSource> = Vec::new();
    if let Some(key) = &cfg.fmp_api_key {
        fmp = FmpCollector::new(http_client(&cfg.user_agent, &limiter), key.clone());
        fact_sources.push(&fmp);
        price_sources.push(&fmp);
    }
    let scrape; // scrape only for targeted runs (not the whole universe)
    if !cfg.collect_all {
        scrape = ScrapeCollector::new(http_client(&cfg.user_agent, &limiter));
        fact_sources.push(&scrape);
    }
    let finnhub;
    let mut news_sources: Vec<&dyn NewsSource> = Vec::new();
    if let Some(key) = &cfg.finnhub_api_key {
        finnhub = FinnhubCollector::new(http_client(&cfg.user_agent, &limiter), key.clone());
        news_sources.push(&finnhub);
    }

    while let Some((tier, at)) = scheduler::next_tier(chrono::Utc::now()) {
        let now = chrono::Utc::now();
        let wait = (at - now).to_std().unwrap_or(std::time::Duration::ZERO);
        tracing::info!("next collection: {} at {at}", tier.label());
        tokio::time::sleep(wait).await;
        let fired = chrono::Utc::now();
        let label = tier.label();

        let result = scheduler::run_tracked(store, label, None, fired, || async {
            match tier {
                Tier::Fundamentals => collect_fundamentals(store, cfg, &fact_sources, fired).await,
                Tier::Price => collect_prices(store, cfg, &price_sources, fired, delay).await,
                Tier::News => collect_news(store, cfg, &news_sources, fired, delay).await,
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
    now: chrono::DateTime<chrono::Utc>,
) -> Result<pipeline::CollectSummary, stonkscollect_backend::store::StoreError> {
    let cutoff = cfg
        .collect_max_age_hrs
        .map(|h| now - chrono::Duration::hours(h as i64));
    let mut summary = if cfg.collect_all {
        pipeline::collect_all(store, sources, cfg.reconcile_threshold, now, cfg.collect_concurrency, cutoff).await?
    } else {
        let outcomes =
            pipeline::collect_tickers(store, sources, &cfg.tickers, cfg.reconcile_threshold, now).await?;
        let mut s = pipeline::CollectSummary::default();
        for (_t, r) in &outcomes {
            s.companies += 1;
            s.facts_written += r.facts_written;
        }
        s
    };
    summary.discrepancies_written += pipeline::recompute_ratios_all(store, now).await?.facts_written;
    Ok(summary)
}

async fn collect_prices(
    store: &Store,
    cfg: &Config,
    sources: &[&dyn PriceSource],
    _now: chrono::DateTime<chrono::Utc>,
    delay: std::time::Duration,
) -> Result<pipeline::CollectSummary, stonkscollect_backend::store::StoreError> {
    if cfg.collect_all {
        return pipeline::collect_prices_all(store, sources, delay).await;
    }
    let mut s = pipeline::CollectSummary::default();
    for ticker in &cfg.tickers {
        if let Some(c) = store.get_company(ticker).await? {
            let t = SourceTarget { cik: c.cik.clone(), symbol: c.ticker.clone() };
            s.facts_written += pipeline::collect_prices_for(store, sources, c.id, &t).await?;
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
    delay: std::time::Duration,
) -> Result<pipeline::CollectSummary, stonkscollect_backend::store::StoreError> {
    if cfg.collect_all {
        return pipeline::collect_news_all(store, sources, now, delay).await;
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

async fn bootstrap(store: &Store, cfg: &Config) {
    let limiter = Arc::new(RateLimiter::new(std::time::Duration::from_millis(cfg.request_delay_ms)));
    let edgar = EdgarCollector::new(http_client(&cfg.user_agent, &limiter));
    let refs = edgar.collect_company_tickers().await.expect("fetch tickers");
    let n = pipeline::bootstrap_companies(store, &refs)
        .await
        .expect("bootstrap companies");
    tracing::info!("bootstrapped {n} companies");
}

async fn collect(store: &Store, cfg: &Config, mut tickers: Vec<String>, all: bool) {
    let bulk = all || cfg.collect_all;

    let limiter = Arc::new(RateLimiter::new(std::time::Duration::from_millis(cfg.request_delay_ms)));
    let edgar = EdgarCollector::new(http_client(&cfg.user_agent, &limiter));
    let mut sources: Vec<&dyn FactSource> = vec![&edgar];
    let fmp;
    if let Some(key) = &cfg.fmp_api_key {
        fmp = FmpCollector::new(http_client(&cfg.user_agent, &limiter), key.clone());
        sources.push(&fmp);
    }
    let scrape;
    if !bulk {
        scrape = ScrapeCollector::new(http_client(&cfg.user_agent, &limiter));
        sources.push(&scrape);
    }

    let now = chrono::Utc::now();
    if bulk {
        // One-shot CLI always collects (no freshness cutoff).
        let s = pipeline::collect_all(store, &sources, cfg.reconcile_threshold, now, cfg.collect_concurrency, None)
            .await
            .expect("collect all");
        report_bulk("collect", Ok(s));
    } else {
        if tickers.is_empty() {
            tickers = cfg.tickers.clone();
        }
        tickers.iter_mut().for_each(|t| *t = t.to_uppercase());
        let outcomes = pipeline::collect_tickers(store, &sources, &tickers, cfg.reconcile_threshold, now)
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
    // Derive ratios from whatever facts now exist.
    let r = pipeline::recompute_ratios_all(store, now).await.expect("recompute ratios");
    tracing::info!("computed ratios for {} companies", r.companies);
}
