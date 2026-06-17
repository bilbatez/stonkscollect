# frontend/src — module map

React 19 + Vite + TypeScript SPA, MUI v9 (dark-first). Canonical context:
[`/CLAUDE.md`](../../CLAUDE.md), [`/FEATURES.md`](../../FEATURES.md),
[Frontend Features](../../docs/features.md).

| Path | Responsibility |
|---|---|
| `App.tsx` / `main.tsx` | auth routing + theme state; React entry (`main.tsx` coverage-excluded) |
| `api.ts` | all `fetch()` calls + bearer-token management |
| `types.ts` | TypeScript interfaces (mirror `backend/src/domain.rs`) |
| `quote.ts` | pure derivation: `computeQuote`, `computeKeyStats`, `computePeriodReturns`, `dedupeDaily` |
| `chartData.ts` | pure transforms: `movingAverage`, `pricesForRange`, `normalizedReturns`, `ttmColumn`, income/ratio/graham chart data |
| `format.ts` | number/date formatting, labels, growth, `scoreHeatColor`, CSV helpers |
| `constants.ts` | `PAGE_SIZE`, `CHART_HEIGHT`, `GRAHAM_FORMULA_MULTIPLE`, … |
| `hooks/` | `usePaginatedFetch` (loading/error/abort) |
| `components/auth/` | `AuthForm` |
| `components/layout/` | `Dashboard` (nav + routing + market/trending strips), `Watchlist` |
| `components/pages/` | `AllStocks`, `CompanyView`, `CompareView`, `Screener`, `MoversView`, `SectorOverview` |
| `components/panels/` | quote/key-stats/metrics, statements (+TTM), ratios, dividends, graham, peers, holders, news, notes, discrepancies, market summary, trending strip |
| `components/shared/` | `DataGrid`, `Compare`, `RangeToggle`, `PeriodToggle`, `WeekRangeBar`, `FreshnessBadge`, `ThemeToggle`, `Skeleton` |
| `charts/` | lazy ECharts wrappers (`PriceChart`, `IncomeChart`, `RatioChart`, `GrahamChart`, `CompareChart`) — **coverage-excluded; keep logic in `quote.ts`/`chartData.ts`** |
| `e2e/` | Playwright specs (under `frontend/e2e/`, separate from Vitest `src/**`) |

Conventions: 100% Vitest coverage (charts excluded), strict TDD, MUI props via
`sx`. See [`/.cursorrules`](../../.cursorrules) and [`/AGENTS.md`](../../AGENTS.md).
