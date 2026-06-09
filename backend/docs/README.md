# StonksCollect — Backend Documentation

The backend is a single Rust crate that collects US-equity fundamentals, prices,
and news from multiple sources, cross-checks them (SEC EDGAR canonical), derives
ratios and a Benjamin-Graham defensive-investor scorecard, stores everything in
SQLite, and serves a REST API for the dashboard.

> Not real-time. The model is **latest-and-stored**: collect on a schedule (or on
> demand), persist history locally, serve from the database.

## Documents

| Doc | What's inside |
|-----|---------------|
| [architecture.md](architecture.md) | Layering, the source-collector trait model, dependency-injection seams, data flow, design rules |
| [data-model.md](data-model.md) | SQLite schema, migrations, domain types |
| [collection.md](collection.md) | Sources, the ingest/collect pipeline, reconciliation, rate limiting, the scheduler, the CLI |
| [analysis.md](analysis.md) | Derived ratios and the Graham scorecard (formulas + criteria) |
| [api.md](api.md) | REST endpoints, auth (argon2 + bearer sessions), middleware, error handling |
| [configuration.md](configuration.md) | Every environment variable + defaults |
| [testing.md](testing.md) | TDD workflow, coverage gates, how to run |

## Tech stack

- **Rust 1.91**, `axum` 0.7 (HTTP) on `tokio`, `tower`/`tower-http` (middleware).
- `sqlx` (SQLite, WAL) for storage; `arrow`/`parquet` for export.
- `reqwest` (rustls) for outbound HTTP; `scraper` + `rss` for parsing.
- `argon2` + `sha2` for auth; `cron` for scheduling; `clap` for the CLI.
- `serde`, `thiserror`, `tracing`, `chrono`, `futures`, `dotenvy`.

## Crate layout

```
backend/
  src/
    lib.rs        app(Arc<Store>) router + middleware layers
    main.rs       CLI: serve | bootstrap | collect | seed-admin   (thin glue, coverage-excluded)
    config.rs     env-driven Config (pure parse(getter))
    domain.rs     typed models + value objects (no I/O)
    store.rs      SQLite CRUD, range queries, OHLC, Parquet export, login throttle holder
    collectors/   edgar, fmp, yahoo (prices), news (rss/yahoo/finnhub), scrape + source traits
    http.rs       reqwest client with retry + rate-limit   (coverage-excluded glue)
    net.rs        RetryPolicy + RateLimiter + LoginThrottle (pure, time-injected)
    reconcile.rs  canonical selection + discrepancy flagging (pure)
    ratios.rs     derived ratios per (period_end, period_type) (pure)
    graham.rs     Graham defensive scorecard, Graham Number, NCAV (pure)
    pipeline.rs   ingest, collect_all/collect_tickers (parallel, incremental), recompute
    scheduler.rs  tiered cron expressions + next_after + best-effort run tracking
    auth.rs       argon2 password hashing + opaque session tokens (pure)
    api.rs        axum REST handlers + AuthUser extractor
  migrations/     SQL (0001_init … 0006_ratios_period)
  tests/          integration tests + fixtures/
```

## Quick start

```
cd backend
cp ../.env.example ../.env       # set USER_AGENT to a real contact (SEC requires it)
cargo run -- bootstrap           # load the SEC ticker -> CIK universe (~10k)
cargo run -- collect --ticker AAPL --ticker KO   # facts + prices + news + metrics
cargo run -- serve               # REST API on :8080 + scheduled background collection
cargo run -- seed-admin          # dev only: create admin@admin.com / admin
```
