# Frontend Features

The frontend is a React 19 SPA served by Vite in development and nginx in production. All components use MUI v9 with a dark-first theme (teal primary `#14b8a6`).

## Pages

### Home

The landing page after login. Above the tabs sit a **market summary** strip
(`MarketSummary` — S&P 500 / Nasdaq / Dow cards via `/api/markets/summary`) and a
**trending strip** (`TrendingStrip` — the day's top gainer/loser chips via
`/api/movers`). Contains two tabs:

**All Stocks tab** — Paginated, sortable, filterable directory of all ~8,000+ companies with their Graham scores. Implemented as a `DataGrid` (TanStack Table + DnD Kit) with per-column filters and drag-reorderable columns.

- Columns: Ticker, Name, Industry, Score, Graham #, Margin of Safety.
- Search by ticker or company name.
- Sort by any column ascending/descending.
- "Watch" button on each row adds to watchlist.
- Click a company row to open the company detail panel below the tabs.

**Watchlist tab** — The current user's personal watchlist. Add tickers manually via the input field; remove via the × button. Click a company to open it.

### Company Detail

Opens inline within the Home page (tabs remain visible). Sections:

1. **Header** — Company name, ticker, freshness badge (green/yellow/red by price-data age).
2. **Quote header** (`QuoteHeader`) — last close, colored day change, and a row of **trailing-return chips** (1D / 5D / 1M / 6M / YTD / 1Y / 5Y).
3. **Profile chips** — Sector, industry, exchange; description; links (SEC filings, website, Wikipedia, Yahoo Finance).
4. **Metrics summary** — 8 quick-view cards: P/E, P/B, ROE, Net margin, D/E, Current ratio, Graham #, Margin of safety.
5. **Key statistics** (`KeyStatsPanel`) — 16-card grid (market cap, shares, float, day/52-week range, volume, EPS, P/E, P/B, dividend rate/yield, payout, FCF, BVPS, employees) above a **52-week range gauge** (`WeekRangeBar`).
6. **Price chart** — ECharts candlestick/line with a **volume sub-chart**, optional **SMA 50/200 overlays**, and **range presets** (1M/6M/YTD/1Y/5Y/MAX).
7. **Graham Number chart** — close price (solid) vs. Graham Number (dashed) with a green "buying zone" shaded where price < Graham Number.
8. **Income chart** — Grouped bar chart: Revenue / Gross Profit / Net Income per period (annual/quarterly toggle).
9. **Statements table** — income / balance / cash flow with YoY growth badges, a **TTM column** on the quarterly view (last-4-quarter sums for flows, latest quarter for balance items), and CSV export.
10. **Dividends** (`DividendPanel`) — annual dividend-per-share history.
11. **Graham Scorecard** — all 8 criteria with pass/fail status + detail; Defensive / Net-net badges.
12. **Ratios panel** — Table of all computed ratios (annual/quarterly toggle) + CSV.
13. **Peers panel** — Same-sector companies ranked by Graham score.
14. **Holders** (`HoldersPanel`) — insider holders from SEC Form 4 filings (holder, type, shares, as-of); empty until a `collect` run has fetched Form 4 data.
15. **Notes** — Private note per company (persisted server-side, per user).
16. **News** — Recent headlines from Yahoo RSS and Finnhub.
17. **Discrepancies** — Cross-source value conflicts flagged by the reconciler.

### Screener

`/api/screen` powered view. Rank Graham defensive passers with filters:

- Minimum Graham score (0–8 slider).
- Defensive only / net-net only checkboxes.
- Sector filter.
- **Advanced filters**: Min P/E, Max P/E, Min ROE, Max D/E, Min net margin.
- Paginated, sortable grid of results.

### Compare

Side-by-side comparison of multiple companies (debounced autocomplete add/remove).
Renders a **normalized % overlay chart** (`CompareChart` — each ticker rebased to
its first common date) above a metric comparison table.

### Movers

Three tables — top gainers, losers, and most-active — by latest daily move, with
clickable tickers (`/api/movers`).

### Sectors

Sector-level overview table: sector name, company count, average Graham score
(cells **heat-colored** by score), % passing defensive criteria, top-ranked ticker
(clickable to open company detail).

## Theming

Dark mode by default. Toggle in the navbar switches to light mode. The choice is persisted to `localStorage` (`stonks_theme`) and restored on the next load.

The MUI theme is configured with:
- `primary.main: #14b8a6` (teal)
- `success.main: #22c55e` (green)
- `error.main: #ef4444` (red)
- Dark background: `#0e1116` / paper `#161b22`

## Component structure

Components are grouped by role (`auth/`, `layout/`, `pages/`, `panels/`,
`shared/`); `*.test.tsx` are co-located or at `components/` root for cross-cutting
suites.

```
src/
  App.tsx                  Auth routing + theme state
  components/
    auth/AuthForm.tsx        Login/signup form
    layout/
      Dashboard.tsx           Nav + page routing + watchlist state + market/trending strips
      Watchlist.tsx           User watchlist management
    pages/
      AllStocks.tsx           Paginated company directory
      CompanyView.tsx         Full company detail (all sections)
      CompareView.tsx         Compare table + normalized overlay chart
      Screener.tsx            Graham screener with filters
      MoversView.tsx          Gainers / losers / most-active
      SectorOverview.tsx      Sector aggregates + heatmap
    panels/
      QuoteHeader.tsx         Last close, day change, period-return chips
      KeyStatsPanel.tsx       16-card key statistics
      MetricsSummary.tsx      8-card quick metrics bar
      MarketSummary.tsx       Index summary cards (Home)
      TrendingStrip.tsx       Top gainers/losers chips (Home)
      GrahamScorecard.tsx     8-criteria scorecard
      StatementTable.tsx      Statements + YoY growth + TTM column
      DividendPanel.tsx       Annual dividend-per-share history
      RatiosPanel.tsx         Computed ratios table
      PeersPanel.tsx          Sector peers list
      HoldersPanel.tsx        Form 4 insider holders
      NewsFeed.tsx            News headlines
      NotePanel.tsx           Personal note editor
      DiscrepancyPanel.tsx    Cross-source conflicts
    shared/
      DataGrid.tsx            Reusable sortable/filterable grid
      Compare.tsx             Metric comparison table
      RangeToggle.tsx         1M/6M/YTD/1Y/5Y/MAX presets
      PeriodToggle.tsx        Annual/quarterly toggle
      WeekRangeBar.tsx        52-week range gauge
      FreshnessBadge.tsx      Price-data freshness indicator
      ThemeToggle.tsx         Dark/light mode switch
      Skeleton.tsx            Loading placeholder
  charts/                  (lazy-loaded, coverage-excluded — keep logic in helpers)
    PriceChart.tsx           Candlestick + volume + SMA overlays
    IncomeChart.tsx
    RatioChart.tsx
    GrahamChart.tsx
    CompareChart.tsx         Normalized multi-ticker overlay
  hooks/usePaginatedFetch.ts Loading/error/abort fetch hook
  api.ts                   All HTTP calls + token management
  quote.ts                 computeQuote / computeKeyStats / computePeriodReturns
  chartData.ts             movingAverage / pricesForRange / normalizedReturns / ttmColumn / chart transforms
  format.ts                Number formatting, labels, growth calc, scoreHeatColor
  constants.ts             PAGE_SIZE, CHART_HEIGHT, GRAHAM_FORMULA_MULTIPLE
  types.ts                 TypeScript interfaces
```

## Key frontend patterns

- **API layer** (`api.ts`) — all `fetch()` calls in one file; attaches Bearer token; throws on non-OK responses.
- **Lazy charts** — all ECharts components are `React.lazy()` wrapped in `<Suspense>`. Avoids bundling the large ECharts library in the initial JS chunk.
- **100% coverage** — every component and utility is covered by Vitest tests. Charts are excluded via `vite.config.ts` `coverage.exclude`.
- **MUI v9 compliance** — no system shorthand props (`alignItems`, `fontWeight`, etc.); all go via `sx`. `slotProps` instead of deprecated `inputProps`.
