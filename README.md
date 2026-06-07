# StonksCollect

Collects **US-equity fundamental, price, and news data** from multiple reliable
sources, **cross-checks** them (SEC EDGAR is canonical), stores history locally,
and serves a **dashboard with graphs** for fundamental analysis.

Not real-time — the goal is the *latest* data, stored and queryable, with
discrepancies between sources surfaced so you can trust the numbers.

> Status: all building blocks implemented and tested. The live collection
> *driver* (the loop that runs collectors on a schedule) is the one remaining
> integration step — see [Roadmap](#roadmap).

---

## Architecture

```
                 ┌─────────────────────────── backend (Rust / axum) ───────────────────────────┐
   SEC EDGAR ─┐  │  collectors/                  reconcile          pipeline        store        │
   FMP API   ─┼─▶│  (FactSource trait)  ──facts─▶ (canonical +  ──▶ ingest()  ──▶  SQLite (WAL)   │
   RSS/Finnhub┤  │  edgar · fmp · scrape          discrepancies)                   + Parquet      │
   scrape    ─┘  │        ▲ HttpClient seam                                          export       │
                 │        │                         scheduler (tiered cron)            │          │
                 │        │                                                       REST API        │
                 └────────┼────────────────────────────────────────────────────────┼────────────┘
                          │ (reqwest, real I/O)                              /api    │
                                                                                     ▼
                                                              frontend (React + Vite + ECharts)
                                                              served by nginx, /api → backend
```

- **Collectors** turn one external source into domain models. Network I/O is
  injected via the `HttpClient` trait, so every collector is tested offline
  against captured fixtures. Fact-producing collectors implement the
  `FactSource` trait (Strategy) so the pipeline can aggregate from an
  open-ended set of sources.
- **Reconcile** is pure logic: pick the canonical value per period (EDGAR wins)
  and flag any source that diverges beyond a threshold.
- **Pipeline** (`ingest`) ties it together: collect from all sources
  (best-effort), reconcile, persist canonical facts + discrepancies.
- **Store** is SQLite (WAL) — a single file that's trivial to back up; tables
  also export to Parquet for portable archives.
- **Scheduler** defines tiered cadences (price daily, news several times/day,
  fundamentals weekly) and a best-effort run-tracking wrapper.

## Tech stack

| Layer | Choice |
|-------|--------|
| Backend | Rust 1.91, `axum`, `tokio`, `sqlx` (SQLite), `reqwest`, `scraper`, `arrow`/`parquet`, `cron` |
| Frontend | React 19 + Vite + TypeScript, ECharts (lazy-loaded), Vitest, Playwright |
| Storage | SQLite (WAL) single file + scheduled Parquet export |
| Deploy | Two Dockerfiles (Rust backend, nginx-served frontend) + `docker-compose.yml` |

## Quickstart

Prereqs: Rust toolchain, Node 20+, (optional) Docker with the compose plugin.

The backend is a CLI with three subcommands. Config comes from a `.env` file
(loaded automatically) or real env vars:

```bash
cd backend
cp ../.env.example ../.env        # then edit (set USER_AGENT contact, keys)
mkdir -p ../data

# 1. Populate the ticker -> CIK universe from SEC (~10k companies).
cargo run -- bootstrap

# 2. Scrape + reconcile + persist data for tickers, then exit.
#    (EDGAR needs no key; set FMP_API_KEY / FINNHUB_API_KEY for more sources.)
cargo run -- collect --ticker AAPL --ticker MSFT
#    ...or rely on the TICKERS list from .env:
cargo run -- collect
#    ...or collect the entire bootstrapped US universe (~10k companies,
#    EDGAR-only, throttled by REQUEST_DELAY_MS — takes a while):
cargo run -- collect --all

# 3. Serve the REST API (default :8080). Also runs a background loop that
#    collects on the tiered schedule — the whole universe if COLLECT_ALL=true,
#    else the configured TICKERS (idle if neither is set).
cargo run -- serve
```

```bash
# Frontend (dev server, proxies /api -> :8080)
cd frontend && npm install && npm run dev

# Everything in containers
docker compose up --build      # frontend on :3000, shared ./data volume
```

### Configuration

Copy `.env.example` to `.env` and edit. The backend loads `.env` automatically
(via `dotenvy`); real environment variables override it. Keys:

`DATABASE_URL`, `PORT`, `USER_AGENT` (SEC requires a contact UA), `FMP_API_KEY`,
`FINNHUB_API_KEY`, `TICKERS` (comma-separated), `COLLECT_ALL` (collect the whole
universe; overrides `TICKERS`), `REQUEST_DELAY_MS` (bulk throttle, default 150),
`RECONCILE_THRESHOLD` (default 0.05).

## Project layout

```
backend/        Rust crate (library of tested units + thin bootstrap binary)
  src/
    domain.rs       typed models + value objects (PeriodType, StatementKind, …)
    store.rs        SQLite CRUD + Parquet export
    collectors/     edgar · fmp · news · scrape (behind HttpClient; FactSource)
    http.rs         real reqwest client (I/O glue, coverage-excluded)
    reconcile.rs    canonical selection + discrepancy flagging (pure)
    pipeline.rs     ingest(): collect → reconcile → persist
    scheduler.rs    Tier cron expressions + run_tracked observability
    api.rs          axum REST handlers
    testutil.rs     shared test helpers (FakeHttp, temp_store)
  migrations/   SQL schema
  tests/        integration tests + fixtures/
frontend/       React SPA (src/, src/charts/ lazy echarts, e2e/ Playwright)
data/           mounted volume: stonks.db + parquet exports + backups
```

## Data sources (US only)

- **SEC EDGAR** `data.sec.gov` companyfacts — canonical fundamentals (free, legal).
- **Financial Modeling Prep** — prices, ratios, income-statement facts (cross-check).
- **HTML scrape** (stockanalysis.com style) — gap-fill + cross-check, rate-limited.
- **News** — RSS (Reuters/AP/CNBC/MarketWatch/Yahoo) + Finnhub; title + description, deduped.

Conflicts: every source's value is stored; EDGAR is canonical; mismatches above
a relative threshold are flagged in the `discrepancies` table and the dashboard.

## REST API

`GET /health` and, under `/api`:
`companies/:ticker`, `…/prices`, `…/facts`, `…/ratios`, `…/news`,
`…/discrepancies`, and `runs` (recent collection runs).

## Testing & quality

Strict TDD throughout. Run via the Makefile:

```bash
make test    # backend cargo test + frontend vitest
make cov     # coverage gates: backend functions 100% / lines ≥99%; frontend 100%
make lint    # cargo clippy -D warnings + eslint
make e2e     # Playwright
```

Coverage notes: I/O glue (`http.rs`, `main.rs`, `src/charts/`) is explicitly
excluded, not silently skipped. The backend lines gate is 99% (functions stay
100%) to absorb a cargo-llvm-cov phantom-line artifact over async generic
functions — see the `Makefile`.

## Roadmap

- Persist prices + news (collectors exist; only facts are wired into ingest).
- Ratios computed from stored facts; segment/ownership/guidance ingestion;
  per-ticker schedule overrides; scheduled Parquet exports.

See `CLAUDE.md` / `AGENTS.md` for contributor + AI-agent conventions.
