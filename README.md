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

```bash
# Backend (API on :8080)
cd backend
DATABASE_URL="sqlite://../data/stonks.db" cargo run

# Frontend (dev server on :5173, proxies /api -> :8080)
cd frontend
npm install
npm run dev

# Everything in containers
docker compose up --build      # frontend on :3000, shared ./data volume
```

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

- **Live collection driver:** wire `scheduler::run_tracked` per `Tier` →
  collectors → `pipeline::ingest` in the binary. Needs config (ticker→CIK map,
  API keys, feed URLs). All building blocks exist and are tested.
- Ratios computed from stored facts; segment/ownership/guidance ingestion;
  per-ticker schedule overrides.

See `CLAUDE.md` / `AGENTS.md` for contributor + AI-agent conventions.
