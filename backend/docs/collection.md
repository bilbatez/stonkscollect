# Collection

## Data sources (US only)

| Source | Provides | Endpoint | Key |
|--------|----------|----------|-----|
| **SEC EDGAR** | canonical fundamentals (companyfacts), ticker→CIK universe | `data.sec.gov` / `sec.gov/files/company_tickers.json` | none (requires a contact `User-Agent`) |
| **Yahoo Finance** | daily OHLCV prices (~20y) | `query1.finance.yahoo.com/v8/finance/chart/<SYM>` | none |
| **Yahoo Finance** | per-company news headlines | `feeds.finance.yahoo.com/rss/2.0/headline?s=<SYM>` | none |
| **Financial Modeling Prep** | prices, income facts, company profile (employees) | FMP API | `FMP_API_KEY` |
| **Finnhub** | company news | Finnhub API | `FINNHUB_API_KEY` |
| **HTML scrape** | gap-fill / cross-check (targeted runs) | configured pages | none |
| **SEC EDGAR submissions** | company profile: industry (SIC), exchange, HQ | `data.sec.gov/submissions/CIK….json` | none |
| **Yahoo assetProfile** | prose business summary, website, sector/industry | `query1.finance.yahoo.com/v10/.../quoteSummary?modules=assetProfile` (cookie+crumb) | none |

EDGAR is canonical for fundamentals; every source's value is stored, conflicts
above `RECONCILE_THRESHOLD` are flagged (see reconciliation in
[architecture.md](architecture.md)).

### Source notes

- **EDGAR** (`collectors/edgar.rs`): parses `companyfacts` XBRL into normalized
  `FinancialFact`s; maps ~20 us-gaap concepts (Revenue, NetIncome, Assets,
  CurrentAssets/Liabilities, LongTermDebt, EPS, SharesOutstanding, dividends, cash
  flows, CapEx, …) plus the `dei` cover-page concepts
  (`EntityCommonStockSharesOutstanding` → the `shares_outstanding` table,
  `EntityPublicFloat`). Dedups by latest filing date (10-K/A supersedes 10-K).
  Falls back past the `USD` unit so per-share (`USD/shares`) concepts parse.
  History is whatever SEC has in XBRL (~2009+, older for some issuers).
  Uses **conditional GETs**: ETag/Last-Modified are stored per URL (`http_cache`)
  and replayed, so an unchanged companyfacts document answers 304 and skips the
  multi-megabyte download + re-parse.
- **Yahoo prices** (`collectors/yahoo.rs`): requests explicit `period1`/`period2`
  epochs with `interval=1d` — incremental: `period1` starts at the last stored
  price date minus a 7-day revision overlap, or ~20y back on first collect — **not** `range=max`, which Yahoo downsamples
  to monthly. `Quote` OHLCV arrays are `#[serde(default)]` so responses missing an
  array still parse; days with a null close are skipped. (Stooq was the original
  pick but began serving a JS anti-bot challenge — hence Yahoo.)
- **Yahoo news** (`collectors/news.rs`): per-symbol headline RSS, parsed by the
  shared `parse_rss`; keyless and company-specific.

## The pipeline (`pipeline.rs`)

Core functions:

- `ingest(store, fact_sources, company_id, target, threshold, now)` — fetch facts
  from all sources (best-effort), supplement sources that reported nothing (304,
  error, missing key) with their stored facts, `reconcile`, persist canonical
  facts + discrepancies, and record failures in `source_errors`. Returns an
  `IngestReport` (facts/discrepancies written, per-source errors).
- `collect_prices_for` / `collect_news_for` — fetch + persist for one company
  (best-effort; a failed source is logged at `debug`, not fatal).
- `recompute_metrics` — `recompute_ratios` (`ratios::compute`) + `recompute_graham`
  (`graham::assess` using stored facts + latest price).
- `collect_all(store, &CollectSources, &CollectOptions, progress)` — the whole
  universe (`COLLECT_ALL`) or the freshness-due subset, fetched up to
  `options.concurrency` companies at once via `futures::buffer_unordered`. Per
  company (`collect_company`): facts → prices → news → mark_collected →
  recompute. Reports progress via a `CollectProgress` sink.
- `collect_tickers(store, &CollectSources, tickers, &CollectOptions)` — same
  per-company work for an explicit ticker list.
- `export_all_parquet(store, dir)` — one Parquet file of prices per ticker
  (driven weekly by the Parquet tier).
- `bootstrap_companies` — upsert the SEC ticker→CIK universe.
- `ensure_user` — idempotent dev-login seed.

Ordering matters: **prices are collected before `recompute_graham`** so P/E, P/B,
margin-of-safety and net-net can be computed from the latest price.

## Incremental collection

`COLLECT_MAX_AGE_HRS` sets a cutoff; `collect_all` then processes only companies
whose `company_state.last_collected_at` is null or older than the cutoff, so a
re-run resumes instead of re-fetching everything. One-shot CLI `collect` always
collects (no cutoff).

Two more layers keep re-runs cheap:

- **Incremental prices** — each price source is asked only for bars newer than
  its last stored date (minus a 7-day revision overlap).
- **Conditional GETs (EDGAR)** — stored ETag/Last-Modified validators turn
  unchanged companyfacts/submissions fetches into 304s.

## Rate limiting & retry (`net.rs`)

- `RateLimiter` spaces requests at least `REQUEST_DELAY_MS` apart. **One limiter
  per host** (EDGAR, Yahoo, each vendor) — so hosts throttle independently and
  actually run in parallel under `COLLECT_CONCURRENCY`. Lowering `REQUEST_DELAY_MS`
  is the bigger throughput lever than raising concurrency.
- `RetryPolicy` — exponential backoff (capped), honors `Retry-After`, retries only
  transient failures (transport error, 429, 5xx).
- Both are pure (time injected); the real retry/throttle loop lives in `http.rs`.

## Scheduler (`scheduler.rs`)

`serve` runs a background loop driven by tiered cron expressions:

- **Fundamentals** tier → facts + prices + ratios + Graham (weekly).
- **Price** tier → prices only (daily, after US close), parallel.
- **News** tier → news only (every 6h), parallel.
- **Parquet** tier → weekly price export to `PARQUET_DIR`.

`next_tier(now)` returns the next `(tier, fire_time)`; the loop sleeps until then
and runs that tier's collection, wrapped in `run_tracked` which records a
`collection_runs` row (best-effort). Graceful shutdown on SIGTERM/Ctrl-C via
`tokio::select!`.

## CLI (`main.rs`)

| Command | Action |
|---------|--------|
| `bootstrap` | fetch SEC ticker→CIK directory, upsert companies |
| `collect [--ticker T]… [--all]` | one-shot collect (facts + prices + news + metrics), then exit; prints live progress on `--all` |
| `enrich [--ticker T]… [--all]` | fill company profiles (description, sector/industry, website, employees) from EDGAR submissions (industry/exchange, canonical) + FMP profile (employees, when keyed) + Yahoo assetProfile (prose/website/sector/employees). Best-effort, idempotent |
| `serve` | REST API on `PORT` + background tiered collection; graceful shutdown |
| `seed-admin` | dev only: ensure `admin@admin.com` / `admin` exists |

`make collect` runs `collect` with `ARGS` (defaults to `--all`). Note: pass
options via `ARGS="--ticker AAPL"`, not bare flags (`make` rejects `--all`).
