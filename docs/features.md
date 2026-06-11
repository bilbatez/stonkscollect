# Frontend Features

The frontend is a React 19 SPA served by Vite in development and nginx in production. All components use MUI v9 with a dark-first theme (teal primary `#14b8a6`).

## Pages

### Home

The landing page after login. Contains two tabs:

**All Stocks tab** — Paginated, sortable, filterable directory of all ~8,000+ companies with their Graham scores. Implemented as a `DataGrid` (TanStack Table + DnD Kit) with per-column filters and drag-reorderable columns.

- Columns: Ticker, Name, Industry, Score, Graham #, Margin of Safety.
- Search by ticker or company name.
- Sort by any column ascending/descending.
- "Watch" button on each row adds to watchlist.
- Click a company row to open the company detail panel below the tabs.

**Watchlist tab** — The current user's personal watchlist. Add tickers manually via the input field; remove via the × button. Click a company to open it.

### Company Detail

Opens inline within the Home page (tabs remain visible). Sections:

1. **Header** — Company name, ticker, freshness badge (green/yellow/red based on price data age).
2. **Profile chips** — Sector, industry, exchange.
3. **Description** — SEC/Yahoo company description.
4. **Links** — SEC filings, company website, Wikipedia, Yahoo Finance.
5. **Metrics summary** — 8 quick-view cards: P/E, P/B, ROE, Net margin, D/E, Current ratio, Graham #, Margin of safety.
6. **Price chart** — Interactive ECharts candlestick/line chart of historical close prices.
7. **Graham Number chart** — Dual-line ECharts chart: close price (solid) vs. Graham Number (dashed), with a translucent green "buying zone" shaded where price < Graham Number.
8. **Income chart** — Grouped bar chart: Revenue / Gross Profit / Net Income per period. Toggle between annual and quarterly.
9. **Statements table** — Full income / balance / cash flow data, with year-over-year growth badges (green +% / red -%).
10. **Graham Scorecard** — All 7 criteria shown with pass/fail status and detail text.
11. **Ratios panel** — Table of all computed ratios. Toggle annual/quarterly.
12. **Peers panel** — Companies in the same sector, ranked by Graham score.
13. **Notes** — Private Markdown note per company (persisted server-side, per user).
14. **News** — Recent headlines from Yahoo RSS and Finnhub.
15. **Discrepancies** — Cross-source value conflicts flagged by the reconciler.

### Screener

`/api/screen` powered view. Rank Graham defensive passers with filters:

- Minimum Graham score (0–7 slider).
- Defensive only / net-net only checkboxes.
- Sector filter.
- **Advanced filters**: Min P/E, Max P/E, Min ROE, Max D/E, Min net margin.
- Paginated, sortable grid of results.

### Compare

Side-by-side comparison of up to 4 companies. Add tickers via the input field. Renders a condensed metrics summary for each.

### Sectors

Sector-level overview table: sector name, company count, average Graham score, % passing defensive criteria, top-ranked ticker (clickable to open company detail).

## Theming

Dark mode by default. Toggle in the navbar switches to light mode. The choice is persisted to `localStorage` (`stonks_theme`) and restored on the next load.

The MUI theme is configured with:
- `primary.main: #14b8a6` (teal)
- `success.main: #22c55e` (green)
- `error.main: #ef4444` (red)
- Dark background: `#0e1116` / paper `#161b22`

## Component structure

```
src/
  App.tsx               Auth routing + theme state (~50 lines)
  components/
    Dashboard.tsx         Nav bar + page routing + watchlist state
    CompanyView.tsx        Full company detail with all sections
    AllStocks.tsx          Paginated company directory
    Screener.tsx           Graham screener with filters
    Watchlist.tsx          User watchlist management
    CompareView.tsx        Side-by-side company comparison
    SectorOverview.tsx     Sector aggregates table
    MetricsSummary.tsx     8-card quick metrics bar
    GrahamScorecard.tsx    7-criteria scorecard
    StatementTable.tsx     Financial statements with YoY growth
    RatiosPanel.tsx        Computed ratios table
    PeersPanel.tsx         Sector peers list
    NewsFeed.tsx           News headlines
    NotePanel.tsx          Personal note editor
    DiscrepancyPanel.tsx   Cross-source conflicts
    FreshnessBadge.tsx     Price data freshness indicator
    PeriodToggle.tsx       Annual/quarterly toggle
    DataGrid.tsx           Reusable sortable/filterable grid
    ThemeToggle.tsx        Dark/light mode switch
    AuthForm.tsx           Login/signup form
    Skeleton.tsx           Loading placeholder
  charts/               (lazy-loaded, coverage-excluded)
    PriceChart.tsx
    IncomeChart.tsx
    RatioChart.tsx
    GrahamChart.tsx
  api.ts                All HTTP calls + token management
  format.ts             Number formatting, labels, growth calc
  chartData.ts          Pure data transform functions for charts
  constants.ts          PAGE_SIZE, GRAHAM_FORMULA_MULTIPLE
  types.ts              TypeScript interfaces
```

## Key frontend patterns

- **API layer** (`api.ts`) — all `fetch()` calls in one file; attaches Bearer token; throws on non-OK responses.
- **Lazy charts** — all ECharts components are `React.lazy()` wrapped in `<Suspense>`. Avoids bundling the large ECharts library in the initial JS chunk.
- **100% coverage** — every component and utility is covered by Vitest tests. Charts are excluded via `vite.config.ts` `coverage.exclude`.
- **MUI v9 compliance** — no system shorthand props (`alignItems`, `fontWeight`, etc.); all go via `sx`. `slotProps` instead of deprecated `inputProps`.
