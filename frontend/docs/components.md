# Components

All components are MUI-based and presentational unless noted. Accessible names
(button text, `aria-label`s, heading levels, `role="alert"`/`role="status"`) are
intentionally stable — tests query by role/label/text.

| Component | Props | Behavior |
|-----------|-------|----------|
| **AuthForm** | `onAuth(token)` | Login/signup toggle. `TextField`s (`aria-label` email/password), submit `Button`, `Alert` on error, `Link` to switch mode. Calls `login`/`signup`, reports the token. |
| **AllStocks** | `onSelect(ticker)`, `onAdd(ticker)` | Server-paginated directory. Search box (`aria-label="search stocks"`) → server `q`; `TablePagination`; rows rendered via `DataGrid` (Ticker / Name / Graham score `n/8` / Watch). Sortable + per-column filter + drag-reorder over the page. |
| **Watchlist** | `items`, `onSelect`, `onAdd`, `onRemove` | Sidebar/tab list. Add input (`aria-label="add ticker"`, trims/upper-cases, ignores blanks), per-row select + remove `IconButton` (`aria-label="remove <T>"`), empty state. |
| **Screener** | `onSelect(ticker)` | Self-fetching ranked grid (`screen()`), filters: "Defensive only" + "Net-net" checkboxes (off by default → shows all), `TablePagination`. `DataGrid` columns: Ticker / Score / Graham # / Margin of safety / Net-net. |
| **Compare** | `rows: CompareRow[]` | Matrix of metric (columns, human-labeled) × ticker (rows), values via `formatMetric`; empty → "Nothing to compare." Fed by `App.compare()` (annual ratios only, `Promise.allSettled` so a failed load can't hang it). |
| **CompanyView** (in App) | `data`, `loadedAt` | Card: name + `FreshnessBadge`; sections Price (lazy `PriceChart`), Statements, Graham scorecard, Ratios, News, Discrepancies. |
| **RatiosPanel** | `ratios` | Annual/Quarterly `PeriodToggle`; metrics grouped by category (Profitability/Liquidity/Leverage/Valuation/Per-share) as rows × period date columns; values via `formatMetric`; empty/"no <period> data" states. |
| **StatementTable** | `facts` | Annual/Quarterly toggle; line items grouped by statement section (Income/Balance/Cash flow), human labels, period date columns, `formatCurrency`, dashes for gaps. |
| **GrahamScorecard** | `assessment` | `Card` with score chip `n/m`, defensive/net-net chips, criteria list (check/cancel icons + detail; shows "needs price data" for price-starved P/E·P/B), Graham Number + margin of safety. |
| **NewsFeed** | `news` | List of headline `Link`s (open in new tab) + source `Chip` + optional description; empty state. |
| **DiscrepancyPanel** | `discrepancies` | `DataGrid` (Field / Period / Sources / Diff %), sortable + filterable + reorderable; empty state. |
| **FreshnessBadge** | `status` | `Chip` — Fresh (success) / Stale (warning) / Unknown (default). |
| **ThemeToggle** | `theme`, `onToggle` | `Button` showing the *opposite* theme with a sun/moon icon (keeps visible text for a11y/tests). |
| **PeriodToggle** | `period`, `onChange` | `ToggleButtonGroup` Annual/Quarterly; ignores a re-click on the active option (no null). Shared by Ratios + Statements. |
| **Skeleton** | `label?` | `role="status"` box with `CircularProgress` + visible label. Loading placeholder. |
| **PriceChart** (`charts/`) | `prices` | ECharts canvas, candlestick when OHLC present else close line. Lazy-loaded, **coverage-excluded** (canvas glue). |

## Reusable grid

`AllStocks`, `Screener`, and `DiscrepancyPanel` render through the shared
**`DataGrid`** — see [datagrid.md](datagrid.md). The date-pivot panels
(`RatiosPanel`, `StatementTable`, `Compare`) are intentionally grouped-by-date
matrices, not row grids, so they don't use `DataGrid`.
