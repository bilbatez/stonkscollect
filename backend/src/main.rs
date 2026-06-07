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
use stonkscollect_backend::collectors::scrape::ScrapeCollector;
use stonkscollect_backend::collectors::FactSource;
use stonkscollect_backend::config::Config;
use stonkscollect_backend::http::ReqwestClient;
use stonkscollect_backend::store::Store;
use stonkscollect_backend::{app, pipeline, scheduler};

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
    /// Collect data for tickers now, persist, and exit.
    Collect {
        /// Ticker to collect (repeatable). Defaults to the TICKERS env list.
        #[arg(long = "ticker")]
        tickers: Vec<String>,
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
        Command::Collect { tickers } => collect(&store, &cfg, tickers).await,
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

/// Background loop: sleep until the next tier fires, then collect the configured
/// tickers (wrapped in a tracked run). Runs only if tickers are configured.
async fn scheduler_loop(store: &Store, cfg: &Config) {
    if cfg.tickers.is_empty() {
        tracing::info!("no TICKERS configured; collection loop idle");
        std::future::pending::<()>().await;
    }

    let edgar = EdgarCollector::new(ReqwestClient::new(&cfg.user_agent));
    let scrape = ScrapeCollector::new(ReqwestClient::new(&cfg.user_agent));
    let mut sources: Vec<&dyn FactSource> = vec![&edgar, &scrape];
    let fmp;
    if let Some(key) = &cfg.fmp_api_key {
        fmp = FmpCollector::new(ReqwestClient::new(&cfg.user_agent), key.clone());
        sources.push(&fmp);
    }

    while let Some((tier, at)) = scheduler::next_tier(chrono::Utc::now()) {
        let now = chrono::Utc::now();
        let wait = (at - now).to_std().unwrap_or(std::time::Duration::ZERO);
        tracing::info!("next collection: {} at {at}", tier.label());
        tokio::time::sleep(wait).await;

        let fired = chrono::Utc::now();
        let result = scheduler::run_tracked(store, tier.label(), None, fired, || {
            pipeline::collect_tickers(store, &sources, &cfg.tickers, cfg.reconcile_threshold, fired)
        })
        .await;
        match result {
            Ok(outcomes) => tracing::info!("{} tier collected {} tickers", tier.label(), outcomes.len()),
            Err(e) => tracing::error!("{} tier collection failed: {e}", tier.label()),
        }
    }
}

async fn bootstrap(store: &Store, cfg: &Config) {
    let edgar = EdgarCollector::new(ReqwestClient::new(&cfg.user_agent));
    let refs = edgar.collect_company_tickers().await.expect("fetch tickers");
    let n = pipeline::bootstrap_companies(store, &refs)
        .await
        .expect("bootstrap companies");
    tracing::info!("bootstrapped {n} companies");
}

async fn collect(store: &Store, cfg: &Config, mut tickers: Vec<String>) {
    if tickers.is_empty() {
        tickers = cfg.tickers.clone();
    }
    tickers.iter_mut().for_each(|t| *t = t.to_uppercase());

    let edgar = EdgarCollector::new(ReqwestClient::new(&cfg.user_agent));
    let scrape = ScrapeCollector::new(ReqwestClient::new(&cfg.user_agent));
    let mut sources: Vec<&dyn FactSource> = vec![&edgar, &scrape];

    let fmp;
    if let Some(key) = &cfg.fmp_api_key {
        fmp = FmpCollector::new(ReqwestClient::new(&cfg.user_agent), key.clone());
        sources.push(&fmp);
    }

    let now = chrono::Utc::now();
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
