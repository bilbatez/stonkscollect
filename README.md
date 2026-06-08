# StonksCollect

Collects **US-equity fundamental, price, and news data** from multiple reliable
sources, **cross-checks** them (SEC EDGAR is canonical), stores history locally,
and serves a **dashboard with graphs** for fundamental analysis.

Not real-time — the goal is the *latest* data, stored and queryable, with
discrepancies between sources surfaced so you can trust the numbers.

Multi-user: accounts (argon2 + bearer-token sessions) with per-user watchlists;
market data is shared/global. The React dashboard has login/signup, a watchlist,
multi-ticker compare, dark mode, and charts.

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

## Get started

Prereqs: Rust toolchain, Node 20+, (optional) Docker with the compose plugin.

```bash
make setup          # one-time: creates .env, data/ dir, installs deps, builds backend
# edit .env -> set USER_AGENT to "yourapp your@email" (SEC requires a contact)

make bootstrap                     # load SEC ticker->CIK universe (~10k companies)
make collect ARGS="--ticker AAPL"  # collect one company...  (omit ARGS for --all)
make serve                         # REST API + scheduled collection on :8080
make dev-frontend                  # dashboard dev server (in another terminal)
```

`make help` lists everything. That's the whole setup.

<details><summary>Without make (raw CLI)</summary>

The backend is a CLI; config comes from `.env` (auto-loaded via `dotenvy`) or
real env vars (which override `.env`).

```bash
cd backend
cargo run -- bootstrap                          # ticker universe
cargo run -- collect --ticker AAPL --ticker MSFT
cargo run -- collect                            # uses TICKERS from .env
cargo run -- collect --all                      # entire US universe (EDGAR-only, throttled)
cargo run -- serve                              # API + background scheduler
cargo run -- --help
```
</details>

### Docker

```bash
docker compose up --build          # frontend :3000, backend :8080, shared ./data
# one-off collection inside the stack:
docker compose run --rm backend stonkscollect-backend collect --ticker AAPL
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

## Performance & concurrency

- **Batched writes.** Each company's reconciled facts + discrepancies are
  persisted in a *single* SQLite transaction (`Store::save_reconciled`), and the
  ~10k-company bootstrap is one transaction (`upsert_companies`) — instead of a
  commit per row. This is the dominant speedup for bulk collection.
- **WAL + small pool.** SQLite runs in WAL mode with `synchronous=NORMAL`, a
  5-connection pool, a 5s `busy_timeout`, and a 10s `acquire_timeout`. WAL lets
  the read-only API serve concurrently with the single writer.
- **No long-held locks / no deadlock.** Writes happen in one place (the
  scheduler loop / `collect_*`), one short transaction per company; the API is
  read-only. There is no lock ordering to invert, and `acquire_timeout` bounds
  any pool wait so nothing hangs indefinitely.
- **Responsive while collecting.** `serve` runs the API and the collection loop
  on the same task via `tokio::select!`; collection yields at every await
  (HTTP, DB, the inter-request throttle), so the API stays responsive even
  during a full-universe pass.
- **Polite + bounded.** Bulk collection is EDGAR-only and throttled
  (`REQUEST_DELAY_MS`); facts stream to the DB per company, so memory stays flat.
- **Lazy frontend.** ECharts is code-split out of the initial bundle (~196 kB
  initial vs ~1.3 MB before).

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

- **Candlestick charts** need OHLC; only daily *close* is stored today, so the
  price chart is a close line. Capturing OHLC would unlock candlesticks.
- HTTP conditional GET (ETag/If-Modified-Since) on top of the freshness skip.
- Segment/ownership/guidance ingestion; per-ticker schedule overrides;
  scheduled Parquet exports.

See `CLAUDE.md` / `AGENTS.md` for contributor + AI-agent conventions.
