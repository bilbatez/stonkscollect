# Data Model

SQLite (single file on the mounted `./data` volume, WAL mode). Schema is built by
sequential migrations in `backend/migrations/`, applied automatically on
`Store::connect`. Backup = copy the `.db` file.

## Migrations

| File | Adds |
|------|------|
| `0001_init.sql` | core tables: users, companies, filings, financial_facts, ratios, prices, segments, shares_outstanding, ownership, guidance, news, discrepancies, collection_runs |
| `0002_company_state.sql` | `company_state` (last_collected_at) for the incremental freshness skip |
| `0003_auth.sql` | password hashing columns + `sessions` |
| `0004_graham.sql` | `graham_scores` (+ score index) |
| `0005_ohlc.sql` | `open/high/low` columns on `prices` |
| `0006_ratios_period.sql` | rebuild `ratios` with `period_type` in the unique key (annual vs Q4 no longer collide) |
| `0007_company_profile.sql` | `companies.description` + `companies.website` (profile enrichment) |

## Key tables

### companies
`id, cik, ticker (unique), name, exchange, sector, industry, description, website`.
Identity (`cik/ticker/name`) is populated by `bootstrap`; the profile fields
(`sector/industry/exchange/description/website`) are filled by the `enrich` pass
(EDGAR submissions + Yahoo assetProfile). `description`/`website` added in
`0007_company_profile.sql`.

### financial_facts
One reported line-item value per `(company_id, statement, line_item, period_type,
period_end, source)` (unique). `statement ∈ {income, balance, cashflow}`,
`period_type ∈ {annual, quarterly}`. EDGAR is canonical; other sources coexist for
cross-checking. Indexed on `(company_id, period_end)`.

### prices
Daily OHLCV per `(company_id, date, source)` (unique). `open/high/low/volume`
nullable (older rows / sources may have close only). Indexed on `(company_id, date)`.

### ratios
Derived metrics per `(company_id, period_end, period_type, metric)` (unique).
`metric` is a snake_case key (`net_margin`, `roe`, `pe`, `book_value_per_share`,
…). Indexed on `(company_id, period_type)`.

### graham_scores
One row per company: `score (0–8), passes_defensive, graham_number,
ncav_per_share, margin_of_safety, net_net, computed_at`. Indexed on `score` for the
screener.

### news
`title, description, url, source, published_at, dedup_hash (unique)`. The dedup
hash is a stable hash of the normalized headline, so the same story from multiple
sources collapses to one row.

### discrepancies
Flagged cross-source mismatches: `field, period, source_a/value_a, source_b/value_b,
pct_diff, flagged_at`.

### users / sessions
`users(email unique, password_hash)`; `sessions(token_hash, user_id, expires_at)`.
Sessions store a **sha256 hash** of an opaque bearer token (30-day expiry), never
the raw token. Watchlists are per-user (a join table).

### collection_runs
Observability: `source, scope, started_at, finished_at, status (running|ok|error),
error`. Written best-effort by the scheduler's `run_tracked`.

## Domain types (`domain.rs`)

Pure data structures mirroring the rows, serialized to JSON for the API. Notable:

- `PeriodType` / `StatementKind` are enums with `#[serde(rename_all="lowercase")]`
  → JSON emits `"annual"`/`"income"` (matching the DB tokens **and** the
  frontend's lowercase filters — do not change without updating both).
- `FinancialFact`, `PricePoint`, `Ratio` (carries `period_type`), `NewsItem`,
  `Discrepancy`, `GrahamScore`, `Company`, `CollectionRun`.
- Value objects expose `as_str()` (DB token) + `parse()` (token → enum).

## Parquet export

`Store::export_prices_parquet` writes a company's price history to a Parquet file
(via `arrow`/`parquet`) for portable archiving alongside the SQLite file.
