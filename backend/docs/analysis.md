# Analysis: Ratios & Graham Scorecard

Both modules are **pure** (no I/O), computed from stored `FinancialFact`s (+ the
latest price) and persisted by `pipeline::recompute_metrics`.

## Derived ratios (`ratios.rs`)

`compute(company_id, facts, prices, now)` groups facts by `(period_end,
period_type)` — so annual and Q4 (same end date) never collide — and emits a
`Ratio` per metric whose inputs are present (and denominators non-zero):

| Metric key | Formula |
|------------|---------|
| `net_margin` | NetIncome / Revenue |
| `gross_margin` | GrossProfit / Revenue |
| `operating_margin` | OperatingIncome / Revenue |
| `roe` | NetIncome / StockholdersEquity |
| `debt_to_equity` | TotalLiabilities / StockholdersEquity |
| `current_ratio` | CurrentAssets / CurrentLiabilities |
| `working_capital` | CurrentAssets − CurrentLiabilities |
| `book_value_per_share` | StockholdersEquity / SharesOutstanding |
| `free_cash_flow` | OperatingCashFlow − CapEx |
| `fcf_margin` | FreeCashFlow / Revenue |
| `payout_ratio` | DividendPerShare / EPS |
| `pe` | price(period_end) / EPS  (when EPS > 0) |
| `pb` | price(period_end) / BVPS  (when BVPS > 0) |

`pe`/`pb` use the close on/just before the period end (`price_at`). Each ratio is
stamped with the `period_type` of its source facts, so the dashboard's
Annual/Quarterly toggle filters cleanly.

## Graham defensive-investor scorecard (`graham.rs`)

`assess(facts, latest_price, min_revenue)` follows *The Intelligent Investor*,
adapted to the annual history EDGAR provides (windows degrade to available years;
missing inputs report **"insufficient data"** and fail rather than silently pass).
Only annual facts are used; balance-sheet items come from the latest annual year.

### The 8 criteria

1. **Adequate size** — Revenue ≥ `min_revenue` (`GRAHAM_MIN_REVENUE`, default $500M).
2. **Current ratio ≥ 2** — CurrentAssets / CurrentLiabilities.
3. **Debt ≤ working capital** — LongTermDebt ≤ (CurrentAssets − CurrentLiabilities).
4. **Earnings stability** — NetIncome > 0 across the available years (≥ 3).
5. **EPS growth ≥ 33%** — over the window (3-yr-average endpoints).
6. **P/E ≤ 15** — price / 3-yr-average EPS.
7. **P/B ≤ 1.5 or P/E·P/B ≤ 22.5**.
8. **Dividend record** — dividends paid in every available year.

`score` = criteria passed (0–8); `passes_defensive` = all 8 pass.

### Valuation outputs

- **Graham Number** = √(22.5 · EPS · BVPS), when both > 0.
- **NCAV / share** = (CurrentAssets − TotalLiabilities) / shares.
- **Margin of safety** = GrahamNumber / price − 1.
- **net-net** = price < ⅔ · NCAV/share.

### Why P/E, P/B, margin, net-net can be blank

They all require a **price**. If the `prices` table is empty for a company (no
price source ran), those criteria are "insufficient data", `margin_of_safety` is
null, `net_net` is false, and the max achievable score is 6/8 — so nothing can
pass "defensive". The fix is collecting prices (Yahoo, keyless); the dashboard
shows a "needs price data" hint in that case. Constants: `PE_MAX=15`, `PB_MAX=1.5`,
`PE_PB_MAX=22.5`, `CURRENT_RATIO_MIN=2`, `EPS_GROWTH_MIN=0.33`, `NET_NET_FRACTION=⅔`.

## Screening

`store.screen(defensive_only, net_net_only, min_score, limit, offset)` ranks
`graham_scores` by score (JOINed to companies), with optional filters and
pagination; returns the page plus the total match count. Served at `/api/screen`.
