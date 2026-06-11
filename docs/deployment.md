# Deployment

## Environment variables

Backend config is parsed in `backend/src/config.rs`; missing keys fall back to the
defaults below. A `.env` file (loaded via `dotenvy`, real env vars win) is optional.

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `sqlite:///data/stonks.db` | SQLite connection string |
| `PORT` | `8080` | HTTP listen port (binds `0.0.0.0:PORT`) |
| `USER_AGENT` | `stonkscollect (contact@example.com)` | UA sent to data sources (EDGAR/Yahoo require non-empty) |
| `FMP_API_KEY` | — | Financial Modeling Prep key (prices + income facts); optional |
| `FINNHUB_API_KEY` | — | Finnhub key (news); optional |
| `TICKERS` | — | Comma-separated tickers for the `collect`/`serve` loop |
| `COLLECT_ALL` | `false` | Collect the whole bootstrapped universe (overrides `TICKERS`) |
| `REQUEST_DELAY_MS` | `150` | Per-host politeness spacing between requests |
| `COLLECT_MAX_AGE_HRS` | — | Skip companies collected within this many hours |
| `COLLECT_CONCURRENCY` | `8` | Max companies fetched concurrently in bulk collection |
| `RECONCILE_THRESHOLD` | `0.05` | Relative gap above which cross-source values are flagged |
| `GRAHAM_MIN_REVENUE` | `500000000` | Graham "adequate size" revenue floor |
| `RUST_LOG` | `info` | Tracing filter (`tracing_subscriber` EnvFilter) |

**Frontend** (`frontend/.env`, see `frontend/.env.example`):

| Variable | Description |
|---|---|
| `VITE_API_TARGET` | Dev proxy target, e.g. `http://localhost:8080` |

---

## CLI commands

All run via `cargo run --bin stonkscollect_backend -- <command>`:

| Command | Description |
|---|---|
| `bootstrap` | Download SEC's full ticker → CIK directory (~10k companies). Run once on first deploy. |
| `collect --ticker AAPL,MSFT` | Collect facts, prices, and news for specific tickers; recompute ratios + Graham. |
| `collect --all` | Collect for all companies. Runs incrementally (skips recently collected). |
| `enrich` | Update company profiles (sector, description, website) from SEC/Yahoo. |
| `seed-admin` | Dev only: seed a hardcoded `admin@admin.com` / `admin` login (idempotent, insecure — never use in prod). |
| `serve` | Start the REST API server with background tiered collection loop. |

---

## Docker Compose

```bash
# Start all services
docker compose up -d

# Stop
docker compose down

# View logs
docker compose logs -f backend
```

`docker-compose.yml` wires two services:
- `backend` — Rust binary; mounts `./data` volume for the SQLite file.
- `frontend` — nginx serving the built React SPA; proxies `/api/` and `/auth/` to the backend.

The `./data` directory is the single source of truth. **Backup = copy `./data/stonks.db`**.

---

## Development setup

```bash
# Backend
cd backend
cargo run -- serve           # Hot reload not available; use cargo-watch
cargo test                   # Unit + integration tests
cargo clippy -- -D warnings  # Linter

# Frontend
cd frontend
npm run dev          # Vite dev server with HMR at localhost:5173
npm test             # Vitest watch mode
npm run coverage     # Coverage report
npm run build        # Production build
npm run e2e          # Playwright end-to-end tests

# Combined via Makefile
make test    # backend + frontend tests
make cov     # coverage gates (backend 100% functions / frontend 100%)
make lint    # clippy + eslint
make demo    # bootstrap + collect a handful of tickers
make e2e     # Playwright
```

---

## Makefile targets

| Target | Description |
|---|---|
| `make test` | `cargo test` + `vitest run` |
| `make cov` | Backend `cargo llvm-cov` (100% fn / ≥99% lines) + frontend coverage (100%) |
| `make lint` | `cargo clippy -D warnings` + `eslint` |
| `make demo` | Quick local data: bootstrap + collect a few well-known tickers |
| `make e2e` | Playwright end-to-end tests against the running frontend |
| `make up` | `docker compose up -d` |
| `make down` | `docker compose down` |

---

## Backup and restore

**Backup:**
```bash
cp ./data/stonks.db ./data/stonks.db.bak
```

**Restore:**
```bash
cp ./data/stonks.db.bak ./data/stonks.db
```

The Parquet export (`./data/parquet/`) is a read-only archive of historical prices and facts. It is not used by the app at runtime — it exists for external analysis with DuckDB, pandas, etc.

---

## Notes on Docker locally

`docker compose` plugin may not be installed by default on macOS:

```bash
brew install docker-compose
```

Backend and frontend development + tests work without Docker.
