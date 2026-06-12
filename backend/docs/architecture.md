# Backend Architecture

## Layers

```
            ┌──────────────────────────────────────────────┐
   CLI ─────►  main.rs  (serve | bootstrap | collect | …)   │  thin glue, no logic
            └───────────────┬──────────────────────────────┘
                            │ builds Config + Store + collectors
            ┌───────────────▼──────────────────────────────┐
   HTTP ────►  api.rs  (axum handlers, AuthUser extractor)  │
            │  lib.rs  (router + tower-http middleware)      │
            └───────────────┬──────────────────────────────┘
                            │
            ┌───────────────▼──────────────────────────────┐
            │  pipeline.rs  (orchestration)                 │
            │   ingest → reconcile → persist → recompute     │
            └──┬───────────┬──────────────┬──────────┬──────┘
               │           │              │          │
        collectors/    reconcile.rs   ratios.rs   graham.rs   ← pure domain logic
        (I/O via            (pure)       (pure)      (pure)
         HttpClient)
               │
            ┌──▼───────────────────────────────────────────┐
            │  store.rs  (SQLite/WAL)  ◄── domain.rs types   │
            └───────────────────────────────────────────────┘
```

Logic flows down; only `store.rs` and the collectors do I/O. The pure modules
(`domain`, `reconcile`, `ratios`, `graham`, `net`, `auth`, `config`) have no I/O
and are exhaustively unit-tested.

## The source-collector trait model (Strategy)

Collectors are decoupled from the pipeline by three traits in `collectors/mod.rs`:

```rust
trait FactSource  { fn name() -> &str; async fn fetch_facts(company_id, target, now)         -> Vec<FinancialFact>; }
trait PriceSource { fn name() -> &str; async fn fetch_prices(company_id, target, now, since) -> Vec<PricePoint>;    }
trait NewsSource  { fn name() -> &str; async fn fetch_news(company_id, target, now)          -> Vec<NewsItem>;      }
```

The pipeline takes them bundled as `CollectSources { facts, prices, news }`
(with the pass's knobs in `CollectOptions`) — it aggregates from a heterogeneous, open-ended set of sources without knowing
their concrete types. Adding a source = implement a trait + register it in
`main.rs`. Implementors:

| Source | Traits | Keyless? |
|--------|--------|----------|
| `EdgarCollector` | FactSource + ProfileSource | yes (needs a contact User-Agent) |
| `YahooCollector` | PriceSource | yes |
| `YahooNewsCollector` | NewsSource | yes |
| `FmpCollector` | FactSource + PriceSource + ProfileSource | needs `FMP_API_KEY` |
| `FinnhubCollector` | NewsSource | needs `FINNHUB_API_KEY` |
| `ScrapeCollector` | FactSource | yes (targeted runs only) |

## Dependency-injection seams (for deterministic tests)

- **HTTP** — every collector holds an `HttpClient` (trait, `get_text(url)` plus
  `get_text_with_validators` for conditional GETs). The real impl is `http.rs`
  (`ReqwestClient`, coverage-excluded glue); tests use `testutil::FakeHttp` which
  returns a fixture string, records the URL, and has a 304 mode.
- **Clock** — `now: DateTime<Utc>` is passed in (never `Utc::now()` inside pure
  code), so time-dependent logic (rate limiter, retry, login throttle, freshness,
  Graham windows) is deterministic.
- **IDs/tokens** — `auth::new_token` is the only randomness; sessions store a
  sha256 hash, never the raw token.
- **DB** — `Store` wraps a `SqlitePool`; tests use `testutil::temp_store()` (a
  fresh migrated temp DB).

## Data flow: one collection pass

```
for each company (up to COLLECT_CONCURRENCY in parallel):
  1. ingest():   each FactSource.fetch_facts()  → supplement quiet sources from stored facts
                 → reconcile() → persist canonical facts + discrepancies + source_errors
  2. collect_prices_for():  each PriceSource     → save_prices()  (incremental: since last stored date − 7d)
  3. collect_news_for():    each NewsSource      → save_news()  (deduped by headline hash)
  4. recompute_metrics():   ratios::compute()  +  graham::assess()  → save
  5. mark_collected(now)    (for the incremental freshness skip)
```

`ingest` is best-effort per source: a failing source is recorded in the report
and the `source_errors` table (served at `/api/companies/:ticker/errors`), never
aborts the company; a failing company is counted in
`summary.failed`, never aborts the pass.

## Reconciliation

`reconcile.rs` groups multi-source facts by `(line_item, fiscal year)`, picks the
**canonical** value (EDGAR preferred), and flags any cross-source value differing
by more than `RECONCILE_THRESHOLD` (default 5%) as a `Discrepancy`. Pure, no I/O.

## HTTP server middleware (`lib.rs`)

`app(Arc<Store>)` builds the axum router and wraps it with `tower-http`:
- `RequestBodyLimitLayer` — 64 KiB cap (→ 413).
- `TimeoutLayer` — 30 s (→ 408).
- `TraceLayer` — request tracing.

State is `Arc<Store>` (also holds the process-local `LoginThrottle`). See
[api.md](api.md).

## Design rules

- Small, single-purpose functions; pure domain logic separated from I/O.
- Typed errors (`thiserror`): `StoreError`, `CollectorError`; no `unwrap()` on
  production paths (only static selectors / proven-safe constants).
- Strict TDD; coverage gates (see [testing.md](testing.md)).
- Determinism: inject clock + HTTP + DB; record real source responses as fixtures;
  no live-network calls in tests.
