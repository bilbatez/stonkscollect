# Collectors

Collectors are responsible for fetching raw data from external sources and returning normalized domain objects. All collectors are behind trait interfaces and use an injected `HttpClient` so they can be tested with recorded fixtures without live network calls.

## Trait interfaces

**`FactSource`** — provides financial facts (income statement, balance sheet, cash flow):
```rust
#[async_trait(?Send)]
pub trait FactSource {
    fn name(&self) -> &'static str;
    async fn fetch_facts(
        &self,
        company_id: i64,
        target: &SourceTarget,
        now: DateTime<Utc>,
    ) -> Result<Vec<FinancialFact>, CollectorError>;
}
```

**`PriceSource`** — provides daily OHLCV prices:
```rust
#[async_trait(?Send)]
pub trait PriceSource {
    fn name(&self) -> &'static str;
    async fn fetch_prices(
        &self,
        company_id: i64,
        target: &SourceTarget,
        since: NaiveDate,
    ) -> Result<Vec<PricePoint>, CollectorError>;
}
```

**`NewsSource`** — provides news items:
```rust
#[async_trait(?Send)]
pub trait NewsSource {
    async fn fetch_news(
        &self,
        company_id: i64,
        target: &SourceTarget,
    ) -> Result<Vec<NewsItem>, CollectorError>;
}
```

**`ProfileSource`** — provides company profile metadata (sector, description, website):
```rust
#[async_trait(?Send)]
pub trait ProfileSource {
    fn name(&self) -> &'static str;
    async fn fetch_profile(
        &self,
        target: &SourceTarget,
    ) -> Result<Option<CompanyProfile>, CollectorError>;
}
```

---

## Collectors

### `EdgarCollector` (`collectors/edgar.rs`)

**Implements:** `FactSource`, `ProfileSource`

**Source:** SEC EDGAR `data.sec.gov` companyfacts API — the canonical source for US GAAP fundamentals.

**What it collects:**
- 35+ XBRL us-gaap concepts mapped to normalized line items across income statement, balance sheet, and cash flow statement.
- Company profile metadata from SEC submissions API (exchange, SIC sector description, website).
- Ticker → CIK mapping from SEC's `company_tickers.json`.

**Deduplication:** Multiple filings for the same (line item, period type, period end) — e.g. a 10-K/A amendment — keep only the most recent `filed` date.

**Key method:** `collect_facts(company_id, cik, now)` — fetches `https://data.sec.gov/api/xbrl/companyfacts/CIK{cik:0>10}.json` and parses all known CONCEPTS entries.

---

### `FmpCollector` (`collectors/fmp.rs`)

**Implements:** `FactSource`, `PriceSource`

**Source:** Financial Modeling Prep (requires `FMP_API_KEY`).

**What it collects:**
- Historical daily OHLCV prices.
- Income statement line items (cross-checked against EDGAR).

---

### `YahooCollector` (`collectors/yahoo.rs`)

**Implements:** `PriceSource`, `ProfileSource`

**Source:** Yahoo Finance chart API (keyless, uses a non-empty User-Agent).

**What it collects:**
- Historical daily prices (OHLCV) going back up to 5 years.
- Company metadata: sector, industry, description, website via the `assetProfile` module.

---

### `NewsCollector` (`collectors/news.rs`)

**Implements:** `NewsSource`

Two sub-collectors:
- **`YahooNewsCollector`** — keyless Yahoo Finance RSS feed per company ticker. Collects title + description.
- **`FinnhubNewsCollector`** — Finnhub `/company-news` API (requires `FINNHUB_API_KEY`).

Items are deduplicated by a SHA-256 hash of the URL.

---

### `ScrapeCollector` (`collectors/scrape.rs`)

**Implements:** `FactSource`, `PriceSource`

HTML scrape fallback for gap-filling and cross-checking. Respects `robots.txt`, applies rate-limiting, and caches responses. Used when primary sources are unavailable or for specific data points not covered by EDGAR/FMP.

---

## Collection pipeline (`pipeline.rs`)

The top-level orchestration:

1. **`collect_ticker(ticker, sources, store, now)`** — collect all data for one company:
   - Fetches from all registered `FactSource`s in parallel.
   - Calls `reconcile::select_canonical()` to pick the best value per (line item, period, source).
   - Persists facts and flags discrepancies.
   - Fetches prices from all `PriceSource`s.
   - Fetches news from all `NewsSource`s.
   - Calls `recompute_metrics()` to refresh ratios and Graham score.

2. **`collect_all(store, sources, now)`** — runs `collect_ticker` for all companies due for collection (never collected, or last collected before the tier cutoff).

3. **`recompute_metrics(company_id, store, now)`** — re-derives `ratios` and `graham_scores` from current stored facts. Called after every collection.

---

## Reconciliation (`reconcile.rs`)

Pure function: `select_canonical(facts_by_source) -> (Vec<FinancialFact>, Vec<Discrepancy>)`.

- EDGAR is the canonical source. If EDGAR has a value for a (line item, period), it wins.
- For non-EDGAR sources, if the difference vs. EDGAR exceeds a threshold (default 5%), a `Discrepancy` is created.
- The returned `Vec<FinancialFact>` contains one canonical value per (line item, period type, period end).

---

## HTTP client (`net.rs`, `http.rs`)

- `RetryPolicy` — configurable exponential backoff with jitter.
- `RateLimiter` — token-bucket rate limiting per host.
- `LoginThrottle` — brute-force login protection (5 failures per 10 min per IP, time-injected for testability).
- `http.rs` — wraps Reqwest; applies retry + rate-limit; coverage-excluded (I/O glue).

---

## Scheduler (`scheduler.rs`)

Tiered collection runs in the background when `serve` mode is active:

| Tier | Schedule | Scope |
|---|---|---|
| Tier 1 | Every 1 hour | Watchlisted companies |
| Tier 2 | Every 4 hours | S&P 500 |
| Tier 3 | Every 24 hours | Full universe |

`run_tracked()` records each run in `collection_runs` with start time, finish time, status, and any error message.
