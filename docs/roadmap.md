# Roadmap — what's missing

Canonical list of capabilities **not yet built**, so an agent doesn't assume they
exist. Implemented features are catalogued in [`FEATURES.md`](../FEATURES.md).

## Product gaps

### Data not ingested
- **13F institutional holdings.** Insider Form 4 ownership ships
  (`collectors/edgar_ownership.rs`), but 13F is **filer-centric** (one institution
  reports many issuers), so "who holds ticker X?" requires inverting EDGAR
  full-text search across thousands of filers — a separate collector. The
  `ownership` table already supports `kind = 'institutional'`.
- **Segment & forward-guidance data.** The `segments` and `guidance` tables exist
  (`0001_init.sql`) but are **empty** — these aren't in EDGAR companyfacts and
  need either a paid feed or bespoke filing parsing.

### Out of scope (paid / non-goal)
- **Analyst ratings, price targets, forward estimates** — paid vendor data.
- **Options chains, ESG/sustainability** — out of the fundamental-analysis scope.
- **Real-time quotes / intraday** — by design the app is *latest-and-stored*, not
  realtime.
- **Non-US equities, crypto, FX** — US equities only.

### Wiring gaps (built, not fully integrated)
- **Form 4 holders on the background scheduler.** Insider ownership is collected
  only on the `collect` CLI path; it is **not yet wired into the `serve`
  background tier** (`scheduler.rs`). A live server won't refresh holders until a
  manual `collect` runs. Adding a weekly holders tier is the natural follow-up.
- **Yahoo-style financials reformat & earnings-date timeline.** Considered during
  the chart/fundamentals work but not built — statements use the existing layout,
  and there is no true earnings-*date* timeline (only period-end dates; filing
  dates from the `filings` table are not surfaced).

## Documentation gaps — status
Closed by the docs sync that introduced this file (kept as a drift checklist):
- [x] CLAUDE.md "Remaining" no longer claims ownership "needs a paid feed".
- [x] `docs/api-reference.md` covers `/api/movers`, `/api/markets/summary`,
  `/api/companies/:ticker/holders`, `/api/watchlist/quotes`,
  `/api/companies/:ticker/errors`.
- [x] `docs/features.md` covers MarketSummary, TrendingStrip, DividendPanel,
  HoldersPanel, CompareChart, WeekRangeBar, SMA/volume, TTM, period-return chips,
  sector heatmap, grouped component layout.
- [x] `docs/collectors.md` documents `edgar_ownership.rs` / `HolderSource`.
- [x] `docs/data-models.md` documents `OwnershipHolding`, `MoverRow`/`Movers`,
  `WatchQuote`, `SectorStats`, `is_index`, `employees`.
- [x] Graham criteria count corrected to **8** (score 0–8) across docs.

When new features land, update [`FEATURES.md`](../FEATURES.md), the relevant
`docs/*` page, and this roadmap together.
