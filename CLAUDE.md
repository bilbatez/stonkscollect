# StonksCollect

Collects US-equity fundamental + price + news data from multiple reliable sources,
cross-checks them (SEC EDGAR canonical), stores history locally, and serves a dashboard
with graphs for fundamental analysis. Not realtime — latest-and-stored.

Full design: `/Users/bilbatez/.claude/plans/purring-humming-walrus.md`.

## Tech stack

- **Backend:** Rust 1.91, `axum` 0.7, `tower`/`tower-http` (body-limit + timeout +
  trace middleware), `tokio`, `sqlx` (SQLite), `reqwest`, `scraper`, `cron`,
  `arrow`/`parquet`, `argon2`+`sha2` (auth), `futures` (parallel collect),
  `clap` (CLI), `dotenvy`, `serde`, `thiserror`, `tracing`.
- **Frontend:** React 19 + Vite 8 + TypeScript; **MUI v9** (`@mui/material` +
  `@emotion`, dark-first theme, all components MUI — no bespoke CSS); ECharts
  (lazy, candlestick/line); Vitest + Playwright.
- **Storage:** SQLite single file on mounted volume (`./data/stonks.db`) + scheduled Parquet
  export. Backup = copy the `.db` file.
- **Containers:** separate Dockerfiles for backend + frontend; `docker-compose.yml` wires them
  with a shared `./data` volume. Frontend nginx proxies `/api/` → `backend:8080`.

## Directory structure

```
backend/          Rust crate — lib (all logic) + thin bin (bootstrap, coverage-excluded)
  src/lib.rs        app(Arc<Store>) router + body-limit/timeout/trace layers
  src/main.rs       CLI: serve | bootstrap | collect; NO logic (coverage-excluded)
  src/config.rs     env-driven Config (pure parse(getter))
  src/domain.rs     typed models + value objects
  src/store.rs      SQLite (WAL) CRUD, range queries, OHLC, Parquet export
  src/collectors/   edgar, fmp, news (rss+finnhub), scrape — Fact/Price/NewsSource traits
  src/http.rs       reqwest client w/ retry+rate-limit (coverage-excluded glue)
  src/net.rs        RetryPolicy + RateLimiter + LoginThrottle (pure, time-injected)
  src/reconcile.rs  canonical selection + discrepancy flagging (pure)
  src/ratios.rs     derived ratios incl. P/E, P/B, FCF, payout (pure)
  src/graham.rs     Graham defensive scorecard, Graham Number, NCAV (pure)
  src/pipeline.rs   ingest, collect_all/tickers (parallel, incremental, CollectProgress sink), recompute_metrics
  src/scheduler.rs  Tier cron exprs + next_after + best-effort run_tracked
  src/auth.rs       argon2 password hashing + session tokens (pure)
  src/api.rs        axum REST handlers + AuthUser extractor; login brute-force
                    throttle (Store-held), sanitized 500s (no store/SQL leak)
  migrations/       SQL (companies, facts, prices(OHLC), ratios, graham_scores, users…)
  tests/            integration tests + fixtures/
frontend/         React + Vite SPA
  src/            api client, format utils, components/ + co-located *.test.tsx
  src/charts/     echarts canvas wrappers (lazy-loaded, coverage-excluded)
  e2e/            Playwright specs (smoke + route-mocked dashboard)
data/             mounted volume: stonks.db + parquet/ exports + backups (gitignored)
Makefile          dev tasks
docker-compose.yml
```

**CLI:** `bootstrap` (SEC ticker/CIK universe), `collect [--ticker | --all]`
(facts+prices+news → reconcile → persist → recompute ratios+Graham), `serve`
(REST API + background tiered collection loop via `tokio::select!`; graceful
shutdown on SIGTERM). Multi-user: signup/login (argon2 + bearer sessions),
per-user watchlists; `/api/screen` ranks Graham defensive passers.

**Remaining:** segment/ownership/guidance ingestion (not in EDGAR companyfacts —
needs a paid feed); HTTP conditional GET (ETag) on top of the freshness skip.

## Run / test / build

From repo root via Makefile:

- `make test` — backend `cargo test` + frontend `vitest run`
- `make cov` — coverage gates: backend `cargo-llvm-cov` **functions 100% / lines ≥99%** (main/http excluded); frontend `vitest` **100%** (charts excluded)
- `make demo` — bootstrap + collect a few tickers (quick local data)
- `make lint` — `cargo clippy -D warnings` + `eslint`
- `make e2e` — Playwright (frontend)
- `make up` / `make down` — docker compose

Direct:
- Backend: `cd backend && cargo test | cargo run | cargo clippy --all-targets -- -D warnings`
- Backend coverage: `cargo llvm-cov --ignore-filename-regex '(main|http)\.rs' --fail-under-lines 99 --fail-under-functions 100`
- Frontend: `cd frontend && npm run dev | test:run | coverage | build | e2e`

## Coding conventions

- **Strict TDD (mandatory):** write failing test → watch it fail (RED) → minimal code (GREEN) →
  refactor. No production code without a failing test first.
- **Coverage gate** on logic modules: backend functions 100% / lines ≥99% (the
  ≥99 floor absorbs a cargo-llvm-cov phantom-line artifact over async generic fns,
  proven executed by `--text`); frontend 100%. I/O glue (`main.rs`, `http.rs`,
  `frontend/src/{main.tsx,charts/}`) is explicitly excluded — never silently skip logic.
- **Clean code:** small single-purpose functions; collectors behind `HttpClient` +
  `FactSource`/`PriceSource`/`NewsSource` traits;
  reconcile logic pure (no I/O); domain models free of I/O; inject HTTP/DB/clock so they swap
  with fakes in tests; typed errors (`thiserror`), no `unwrap()` in production paths.
- **Determinism:** inject clock + ids; record real source responses as fixtures. No live-network
  calls in unit/integration tests.
- All logic lives in `backend/src/lib.rs` tree; `main.rs` stays a thin wrapper.

## Data sources (US only)

- **SEC EDGAR** `data.sec.gov` companyfacts/companyconcept — canonical fundamentals.
- **Financial Modeling Prep** (FMP_API_KEY) — prices/OHLC, income facts. **Finnhub** (FINNHUB_API_KEY) — company news. Keyless: EDGAR only.
- HTML scrape fallback (gap-fill + cross-check; respect robots.txt, rate-limit, cache).
- News: RSS (Reuters/AP/CNBC/MarketWatch/Yahoo) + Finnhub; title + description only, deduped.

Conflicts: store every source's value; EDGAR canonical; flag discrepancies above threshold.

## Gotchas

- **`docker compose` plugin not installed locally** — compose files are correct but `make up`/`build`
  need it (`brew install docker-compose`). Backend/frontend dev + tests work without Docker.
- Frontend `tsconfig.app.json` `types` includes `vitest/globals` + `@testing-library/jest-dom`
  so `tsc -b` type-checks `*.test.tsx`. Don't remove or `npm run build` breaks.
- **MUI + Vitest:** `vite.config.ts` `test.server.deps.inline` must keep `/@mui/`,
  `/@emotion/`, `react-transition-group` — MUI's `.mjs` does an extensionless
  directory import the native ESM resolver rejects; inlining lets Vite transform it.
- **MUI v9 dropped system shorthand props** (`alignItems`/`justifyContent`/`flexWrap`/
  `fontWeight`/`textAlign`) from components — pass them via `sx`, not as top-level
  props (`tsc -b` errors otherwise). `Stack` keeps `direction`/`spacing` only.
- vitest `include` is `src/**` only; Playwright `testDir` is `e2e/` — kept separate on purpose.
- DB path uses POSIX `/data/...` inside containers; SQLite file must sit on the `./data` volume.
