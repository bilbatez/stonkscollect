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
use stonkscollect_backend::{app, pipeline};
use stonkscollect_backend::store::Store;

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
        Command::Serve => serve(store, cfg.port).await,
        Command::Bootstrap => bootstrap(&store, &cfg).await,
        Command::Collect { tickers } => collect(&store, &cfg, tickers).await,
    }
}

async fn serve(store: Store, port: u16) {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind listener");
    tracing::info!("listening on {addr}");
    axum::serve(listener, app(Arc::new(store)))
        .await
        .expect("server error");
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
