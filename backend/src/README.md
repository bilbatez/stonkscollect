# backend/src — module map

Rust crate: all logic in the lib tree; `main.rs` is a thin, coverage-excluded
binary. Canonical context: [`/CLAUDE.md`](../../CLAUDE.md),
[`/FEATURES.md`](../../FEATURES.md), [`/docs/`](../../docs/).

| Module | Responsibility |
|---|---|
| `lib.rs` | `app(Arc<Store>)` axum router + body-limit/timeout/trace layers |
| `main.rs` | CLI: `serve` / `bootstrap` / `collect` / `enrich` / `seed-admin` — glue only, no logic (coverage-excluded) |
| `config.rs` | env-driven `Config` (pure `parse(getter)`) |
| `domain.rs` | typed models + value objects (`Company`, `FinancialFact`, `PricePoint`, `Ratio`, `GrahamScore`, `MoverRow`, `OwnershipHolding`, …) |
| `store/` | SQLite (WAL) persistence, split by aggregate: `mod` (struct/pool/policy/helpers), `companies` (+ `upsert_index`), `records` (prices/facts/news/ratios/discrepancies/ownership), `runs`, `accounts` (users/sessions/watch/notes), `analytics` (graham/screen/sectors/peers + `day_changes`/`index_changes` + Parquet export) |
| `collectors/` | source adapters behind `Fact/Price/News/Profile/HolderSource` traits + injected `HttpClient`: `edgar`, `edgar_ownership` (Form 4), `fmp`, `yahoo`, `news`, `scrape`; shared `parse_json`/`nonempty`/`ISO_DATE` |
| `reconcile.rs` | canonical selection (EDGAR wins) + discrepancy flagging (pure) |
| `ratios.rs` | derived ratios per period type (pure) |
| `graham.rs` | 8-criteria defensive scorecard, Graham Number, NCAV (pure) |
| `pipeline/` | `collect` (per-company/batch + `recompute_metrics`), `enrich` (profiles/users/bootstrap + `seed_indices`), `orchestrate` (collect_all/tickers, ingest, persist) |
| `scheduler.rs` | tiered cron exprs + `next_after` + best-effort `run_tracked` |
| `auth.rs` | argon2 hashing + session tokens (pure) |
| `net.rs` | `RetryPolicy` + `RateLimiter` + `LoginThrottle` (pure, time-injected) |
| `http.rs` | reqwest client w/ retry+rate-limit (coverage-excluded glue) |

Deeper: [Architecture](../../docs/architecture.md) ·
[Collectors](../../docs/collectors.md) · [Data Models](../../docs/data-models.md) ·
[API Reference](../../docs/api-reference.md).
