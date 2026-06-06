# StonksCollect

Collects US-equity fundamental + price + news data from multiple reliable sources,
cross-checks them (SEC EDGAR canonical), stores history locally, and serves a dashboard
with graphs for fundamental analysis. Not realtime — latest-and-stored.

Full design: `/Users/bilbatez/.claude/plans/purring-humming-walrus.md`.

## Tech stack

- **Backend:** Rust 1.91, `axum` 0.7, `tokio`, `serde`, `thiserror`, `tracing`.
  Planned: `sqlx` (SQLite), `reqwest`, `scraper`, `tokio-cron-scheduler`, parquet export.
- **Frontend:** React 19 + Vite 8 + TypeScript. Planned charts: ECharts + Lightweight-Charts,
  TanStack/AG Grid for statement tables.
- **Storage:** SQLite single file on mounted volume (`./data/stonks.db`) + scheduled Parquet
  export. Backup = copy the `.db` file.
- **Containers:** separate Dockerfiles for backend + frontend; `docker-compose.yml` wires them
  with a shared `./data` volume. Frontend nginx proxies `/api/` → `backend:8080`.

## Directory structure

```
backend/          Rust crate — lib (all logic) + thin bin (bootstrap, coverage-excluded)
  src/lib.rs      app() router + handlers
  src/main.rs     binary entrypoint (bind + serve); NO logic
  tests/          integration tests (e.g. health.rs)
frontend/         React + Vite SPA
  src/            components, hooks, utils + co-located *.test.tsx (vitest)
  e2e/            Playwright specs (run against built/preview or running stack)
data/             mounted volume: stonks.db + parquet/ exports + backups (gitignored)
Makefile          dev tasks
docker-compose.yml
```

Planned backend modules: `domain/`, `store/`, `collectors/`, `reconcile/`, `scheduler/`, `api/`.

## Run / test / build

From repo root via Makefile:

- `make test` — backend `cargo test` + frontend `vitest run`
- `make cov` — coverage gates (backend `cargo-llvm-cov`, frontend `vitest --coverage`); **100% required**
- `make lint` — `cargo clippy -D warnings` + `eslint`
- `make e2e` — Playwright (frontend)
- `make up` / `make down` — docker compose

Direct:
- Backend: `cd backend && cargo test | cargo run | cargo clippy --all-targets -- -D warnings`
- Backend coverage: `cargo llvm-cov --ignore-filename-regex 'main\.rs' --fail-under-lines 100 --fail-under-functions 100`
- Frontend: `cd frontend && npm run dev | test:run | coverage | build | e2e`

## Coding conventions

- **Strict TDD (mandatory):** write failing test → watch it fail (RED) → minimal code (GREEN) →
  refactor. No production code without a failing test first.
- **100% coverage gate** on logic modules. Pure bootstrap glue (`backend/src/main.rs`,
  `frontend/src/main.tsx`) is explicitly excluded — never silently skip real logic.
- **Clean code:** small single-purpose functions; collectors behind a `Collector` trait;
  reconcile logic pure (no I/O); domain models free of I/O; inject HTTP/DB/clock so they swap
  with fakes in tests; typed errors (`thiserror`), no `unwrap()` in production paths.
- **Determinism:** inject clock + ids; record real source responses as fixtures. No live-network
  calls in unit/integration tests.
- All logic lives in `backend/src/lib.rs` tree; `main.rs` stays a thin wrapper.

## Data sources (US only)

- **SEC EDGAR** `data.sec.gov` companyfacts/companyconcept — canonical fundamentals.
- One financial API (FMP / Finnhub / Alpha Vantage — TBD at Phase 4) — ratios, prices, segments.
- HTML scrape fallback (gap-fill + cross-check; respect robots.txt, rate-limit, cache).
- News: RSS (Reuters/AP/CNBC/MarketWatch/Yahoo) + Finnhub; title + description only, deduped.

Conflicts: store every source's value; EDGAR canonical; flag discrepancies above threshold.

## Gotchas

- **`docker compose` plugin not installed locally** — compose files are correct but `make up`/`build`
  need it (`brew install docker-compose`). Backend/frontend dev + tests work without Docker.
- Frontend `tsconfig.app.json` `types` includes `vitest/globals` + `@testing-library/jest-dom`
  so `tsc -b` type-checks `*.test.tsx`. Don't remove or `npm run build` breaks.
- vitest `include` is `src/**` only; Playwright `testDir` is `e2e/` — kept separate on purpose.
- DB path uses POSIX `/data/...` inside containers; SQLite file must sit on the `./data` volume.
