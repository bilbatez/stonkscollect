# Frontend Architecture

## App shell (`App.tsx`)

```
App
├─ ThemeProvider (MUI, dark-first) + CssBaseline
├─ token === null  →  <AuthForm/>            (login / signup gate)
└─ token present   →  <Dashboard/>
     ├─ AppBar:  StonksCollect | Home | Compare | Screener | ThemeToggle | Log out   (icon buttons)
     └─ Container
          page === 'home'   → Tabs[ All Stocks | Watchlist ]
                               └─ detail.kind === 'company' ? <CompanyView/> + "← Back to list"
                                  : loading ? <Skeleton/> : error ? <Alert+Retry/>
                                  : tab 0 ? <AllStocks/> : <Watchlist/>
          page === 'compare' → <Compare/>     (skeleton while loading)
          page === 'screen'  → <Screener/>
```

### Navigation model

Two pieces of state on `Dashboard`:

- `page: 'home' | 'compare' | 'screen'` — the top-level view (AppBar buttons).
- `detail: { kind: 'none' | 'loading' | 'error' | 'company' }` — the in-home
  company panel.

The **All Stocks / Watchlist tabs stay mounted**; selecting a stock loads it into
`detail` *under* the tabs (no full view switch), so you can go back via "← Back to
list" or by clicking a tab (which clears `detail`). This is why opening a company
no longer hides the navigation.

### Theme

`createTheme` (memoized on the `light`/`dark` toggle) with a finance dark-first
palette: teal primary, green/red semantics, `#0e1116` surfaces. Default mode is
**dark**; `ThemeToggle` flips it, and the choice is mirrored onto
`document.documentElement.dataset.theme`.

## Data fetching

- The API client (`api.ts`) is plain `fetch` with a bearer token attached from
  `localStorage`. No global store/React-Query — components fetch in `useEffect`
  and hold local state.
- `loadCompanyData(ticker)` fans out 7 requests in parallel
  (`company/prices/facts/ratios/news/discrepancies/graham`) and assembles a
  `CompanyData`.
- `AllStocks` and `Screener` are **server-paginated** (limit/offset) and refetch
  on page/filter/search change.

## Period & grouping

`Ratio`/`FinancialFact` carry a lowercase `period_type` (`annual`/`quarterly`).
`RatiosPanel` and `StatementTable` filter client-side by an Annual/Quarterly
`PeriodToggle` and render grouped, date-columned pivots (ratios grouped by
category, statements by section). Values are humanized via `format.ts`.

> Casing matters: the backend serializes enums lowercase specifically so these
> client-side filters match. See backend `data-model.md`.

## Build/proxy

Vite dev server proxies `/api` and `/auth` to `VITE_API_TARGET` (default
`http://localhost:8080`), unchanged (no path rewrite). In production the frontend
nginx does the same. ECharts/`charts/` is lazy-loaded and excluded from coverage.
