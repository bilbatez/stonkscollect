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
  `@emotion`, dark-first theme, all components MUI — no bespoke CSS); reusable
  `DataGrid` (`@tanstack/react-table` + `@dnd-kit` — sort, per-column filter,
  drag column-reorder) for the All Stocks / Screener / Discrepancy grids; ECharts
  (lazy, candlestick/line); Vitest + Playwright.
- **Storage:** SQLite single file on mounted volume (`./data/stonks.db`) + scheduled Parquet
  export. Backup = copy the `.db` file.
- **Containers:** separate Dockerfiles for backend + frontend; `docker-compose.yml` wires them
  with a shared `./data` volume. Frontend nginx + Vite dev proxy both forward
  `/api/` **and** `/auth/` to the backend **unchanged** (backend serves both
  prefixes literally — never strip the prefix). Dev proxy target = `VITE_API_TARGET`
  (`frontend/.env`, see `.env.example`).

## Directory structure

```
backend/          Rust crate — lib (all logic) + thin bin (bootstrap, coverage-excluded)
  src/lib.rs        app(Arc<Store>) router + body-limit/timeout/trace layers
  src/main.rs       CLI: serve | bootstrap | collect | enrich | seed-admin; NO logic (coverage-excluded)
  src/config.rs     env-driven Config (pure parse(getter))
  src/domain.rs     typed models + value objects
  src/store/        SQLite (WAL) persistence, split by aggregate: mod (struct/pool/policy/helpers + tests), companies, records (prices/facts/news/ratios/discrepancies), runs, accounts (users/sessions/watch/notes), analytics (graham/screen/sectors/peers + Parquet export)
  src/collectors/   mod (shared traits + parse_json/nonempty/ISO_DATE helpers), edgar (facts+ProfileSource), fmp, yahoo (keyless prices + ProfileSource via assetProfile), news (rss+finnhub, sha256 dedup), scrape — Fact/Price/News/ProfileSource traits
  src/http.rs       reqwest client w/ retry+rate-limit (coverage-excluded glue)
  src/net.rs        RetryPolicy + RateLimiter + LoginThrottle (pure, time-injected)
  src/reconcile.rs  canonical selection + discrepancy flagging (pure)
  src/ratios.rs     derived ratios (per period_type: annual/quarterly) incl. P/E, P/B, FCF, payout (pure)
  src/graham.rs     Graham defensive scorecard, Graham Number, NCAV (pure)
  src/pipeline/     mod (CollectProgress/NoProgress/CollectSummary + tests), collect (per-company + batch collect, recompute_metrics), enrich (profiles/users/bootstrap), orchestrate (collect_all/tickers, ingest, persist_facts)
  src/scheduler.rs  Tier cron exprs + next_after + best-effort run_tracked
  src/auth.rs       argon2 password hashing + session tokens (pure)
  src/api.rs        axum REST handlers + AuthUser extractor; login brute-force
                    throttle (Store-held), sanitized 500s (no store/SQL leak)
  migrations/       SQL (companies, facts, prices(OHLC), ratios, graham_scores, users…)
  tests/            integration tests + fixtures/
frontend/         React + Vite SPA
  src/            api client, format utils, constants, types
  src/hooks/      reusable hooks (usePaginatedFetch — loading/error/abort)
  src/components/ grouped: auth/, layout/, pages/, panels/, shared/; *.test.tsx co-located or at components/ root for cross-cutting suites
  src/charts/     echarts canvas wrappers + bindChartResize (lazy-loaded, coverage-excluded)
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
- **Financial Modeling Prep** (FMP_API_KEY) — prices/OHLC, income facts. **Finnhub** (FINNHUB_API_KEY) — company news. **Yahoo Finance** chart API — keyless daily prices (no key needed; needs a non-empty User-Agent, our contact UA works). Keyless: EDGAR + Yahoo. (Stooq was tried but now serves a JS anti-bot challenge.)
- HTML scrape fallback (gap-fill + cross-check; respect robots.txt, rate-limit, cache).
- News: keyless per-company **Yahoo headline RSS** (`YahooNewsCollector`) + Finnhub (key); title + description only, deduped. Collected per company inside `collect_*` (like prices), so `make collect` populates news.

Conflicts: store every source's value; EDGAR canonical; flag discrepancies above threshold.

## Gotchas

- **`docker compose` plugin not installed locally** — compose files are correct but `make up`/`build`
  need it (`brew install docker-compose`). Backend/frontend dev + tests work without Docker.
- Frontend `tsconfig.app.json` `types` includes `vitest/globals` + `@testing-library/jest-dom`
  so `tsc -b` type-checks `*.test.tsx`. Don't remove or `npm run build` breaks.
- **MUI + Vitest:** `vite.config.ts` `test.server.deps.inline` must keep `/@mui/`,
  `/@emotion/`, `react-transition-group` — MUI's `.mjs` does an extensionless
  directory import the native ESM resolver rejects; inlining lets Vite transform it.
- **Enum JSON is lowercased** (`PeriodType`/`StatementKind` use `#[serde(rename_all="lowercase")]`) so API JSON (`annual`/`income`) matches the DB tokens and the frontend's lowercase filters. Don't remove — the period/statement toggles filter on these exact strings.
- **MUI v9 dropped system shorthand props** (`alignItems`/`justifyContent`/`flexWrap`/
  `fontWeight`/`textAlign`) from components — pass them via `sx`, not as top-level
  props (`tsc -b` errors otherwise). `Stack` keeps `direction`/`spacing` only.
- vitest `include` is `src/**` only; Playwright `testDir` is `e2e/` — kept separate on purpose.
- DB path uses POSIX `/data/...` inside containers; SQLite file must sit on the `./data` volume.

<!-- rtk-instructions v2 -->
# RTK (Rust Token Killer) - Token-Optimized Commands

## Golden Rule

**Always prefix commands with `rtk`**. If RTK has a dedicated filter, it uses it. If not, it passes through unchanged. This means RTK is always safe to use.

**Important**: Even in command chains with `&&`, use `rtk`:
```bash
# ❌ Wrong
git add . && git commit -m "msg" && git push

# ✅ Correct
rtk git add . && rtk git commit -m "msg" && rtk git push
```

## RTK Commands by Workflow

### Build & Compile (80-90% savings)
```bash
rtk cargo build         # Cargo build output
rtk cargo check         # Cargo check output
rtk cargo clippy        # Clippy warnings grouped by file (80%)
rtk tsc                 # TypeScript errors grouped by file/code (83%)
rtk lint                # ESLint/Biome violations grouped (84%)
rtk prettier --check    # Files needing format only (70%)
rtk next build          # Next.js build with route metrics (87%)
```

### Test (60-99% savings)
```bash
rtk cargo test          # Cargo test failures only (90%)
rtk go test             # Go test failures only (90%)
rtk jest                # Jest failures only (99.5%)
rtk vitest              # Vitest failures only (99.5%)
rtk playwright test     # Playwright failures only (94%)
rtk pytest              # Python test failures only (90%)
rtk rake test           # Ruby test failures only (90%)
rtk rspec               # RSpec test failures only (60%)
rtk test <cmd>          # Generic test wrapper - failures only
```

### Git (59-80% savings)
```bash
rtk git status          # Compact status
rtk git log             # Compact log (works with all git flags)
rtk git diff            # Compact diff (80%)
rtk git show            # Compact show (80%)
rtk git add             # Ultra-compact confirmations (59%)
rtk git commit          # Ultra-compact confirmations (59%)
rtk git push            # Ultra-compact confirmations
rtk git pull            # Ultra-compact confirmations
rtk git branch          # Compact branch list
rtk git fetch           # Compact fetch
rtk git stash           # Compact stash
rtk git worktree        # Compact worktree
```

Note: Git passthrough works for ALL subcommands, even those not explicitly listed.

### GitHub (26-87% savings)
```bash
rtk gh pr view <num>    # Compact PR view (87%)
rtk gh pr checks        # Compact PR checks (79%)
rtk gh run list         # Compact workflow runs (82%)
rtk gh issue list       # Compact issue list (80%)
rtk gh api              # Compact API responses (26%)
```

### JavaScript/TypeScript Tooling (70-90% savings)
```bash
rtk pnpm list           # Compact dependency tree (70%)
rtk pnpm outdated       # Compact outdated packages (80%)
rtk pnpm install        # Compact install output (90%)
rtk npm run <script>    # Compact npm script output
rtk npx <cmd>           # Compact npx command output
rtk prisma              # Prisma without ASCII art (88%)
```

### Files & Search (60-75% savings)
```bash
rtk ls <path>           # Tree format, compact (65%)
rtk read <file>         # Code reading with filtering (60%)
rtk grep <pattern>      # Search grouped by file (75%). Format flags (-c, -l, -L, -o, -Z) run raw.
rtk find <pattern>      # Find grouped by directory (70%)
```

### Analysis & Debug (70-90% savings)
```bash
rtk err <cmd>           # Filter errors only from any command
rtk log <file>          # Deduplicated logs with counts
rtk json <file>         # JSON structure without values
rtk deps                # Dependency overview
rtk env                 # Environment variables compact
rtk summary <cmd>       # Smart summary of command output
rtk diff                # Ultra-compact diffs
```

### Infrastructure (85% savings)
```bash
rtk docker ps           # Compact container list
rtk docker images       # Compact image list
rtk docker logs <c>     # Deduplicated logs
rtk kubectl get         # Compact resource list
rtk kubectl logs        # Deduplicated pod logs
```

### Network (65-70% savings)
```bash
rtk curl <url>          # Compact HTTP responses (70%)
rtk wget <url>          # Compact download output (65%)
```

### Meta Commands
```bash
rtk gain                # View token savings statistics
rtk gain --history      # View command history with savings
rtk discover            # Analyze Claude Code sessions for missed RTK usage
rtk proxy <cmd>         # Run command without filtering (for debugging)
rtk init                # Add RTK instructions to CLAUDE.md
rtk init --global       # Add RTK to ~/.claude/CLAUDE.md
```

## Token Savings Overview

| Category | Commands | Typical Savings |
|----------|----------|-----------------|
| Tests | vitest, playwright, cargo test | 90-99% |
| Build | next, tsc, lint, prettier | 70-87% |
| Git | status, log, diff, add, commit | 59-80% |
| GitHub | gh pr, gh run, gh issue | 26-87% |
| Package Managers | pnpm, npm, npx | 70-90% |
| Files | ls, read, grep, find | 60-75% |
| Infrastructure | docker, kubectl | 85% |
| Network | curl, wget | 65-70% |

Overall average: **60-90% token reduction** on common development operations.
<!-- /rtk-instructions -->