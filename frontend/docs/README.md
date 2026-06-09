# StonksCollect — Frontend Documentation

A React + TypeScript single-page dashboard for Benjamin-Graham-style fundamental
analysis. It talks to the backend REST API, renders prices, statements, ratios,
news, and a Graham scorecard per company, plus a paginated all-stocks directory, a
per-user watchlist, a screener, and a compare view.

## Documents

| Doc | What's inside |
|-----|---------------|
| [architecture.md](architecture.md) | App shell, navigation/state model, MUI theme, data fetching |
| [components.md](components.md) | Every component, its props and behavior |
| [data-and-api.md](data-and-api.md) | API client, auth/token handling, types, the human-label/format registry |
| [datagrid.md](datagrid.md) | The reusable sortable/filterable/reorderable grid |
| [testing.md](testing.md) | Vitest, Playwright, the 100% coverage gate |

## Tech stack

- **React 19 + Vite 8 + TypeScript**.
- **MUI v9** (`@mui/material` + `@emotion`, `@mui/icons-material`) — dark-first
  theme; **all** UI is MUI components (no bespoke CSS).
- **TanStack Table v8** + **dnd-kit** — the reusable `DataGrid` (sort, per-column
  filter, drag column-reorder).
- **ECharts** — lazy-loaded price chart (candlestick when OHLC present, else line).
- **Vitest** + Testing Library (unit) and **Playwright** (e2e).

## Layout

```
frontend/
  src/
    App.tsx          shell: AppBar, ThemeProvider, tabs (All Stocks | Watchlist),
                     company detail, Compare, Screener; auth gate
    api.ts           REST client + bearer-token storage
    types.ts         API response types
    format.ts        currency/percent/ratio formatters + metric & line-item label registries
    components/      AuthForm, AllStocks, Watchlist, Screener, Compare, DataGrid,
                     RatiosPanel, StatementTable, GrahamScorecard, NewsFeed,
                     DiscrepancyPanel, FreshnessBadge, ThemeToggle, PeriodToggle, Skeleton
    charts/          PriceChart (ECharts, lazy, coverage-excluded)
    test/            vitest setup
  e2e/               Playwright specs (route-mocked)
```

## Dev

```
cd frontend
npm install
cp .env.example .env      # VITE_API_TARGET points the dev proxy at the backend
npm run dev               # Vite dev server; proxies /api and /auth to the backend
```

Log in with the dev account (`admin@admin.com` / `admin`, seeded by the backend's
`seed-admin`).
