# Data & API Layer

## API client (`api.ts`)

Thin `fetch` wrapper. The bearer token lives in `localStorage` (`stonks_token`).

- `getToken`/`setToken`/`clearToken` — token storage.
- `authedFetch(path, init)` — attaches `Authorization: Bearer <token>` when present.
- `getJson<T>` / `postJson<T>` — JSON helpers; throw on non-2xx (`request failed: <status>`).

### Functions

| Function | Calls |
|----------|-------|
| `signup(email,password)` / `login(email,password)` | `POST /auth/{signup,login}` → stores + returns token |
| `logout()` | `POST /auth/logout`, clears token |
| `listCompanies(q, limit, offset)` | `GET /api/companies?...` → `Page<CompanyRow>` |
| `screen(filters)` | `GET /api/screen?defensive&net_net&min_score&limit&offset` → `Page<ScreenRow>` |
| `getWatchlist` / `addWatch` / `removeWatch` | `/api/watchlist[...]` |
| `loadCompanyData(ticker)` | parallel `company/prices/facts/ratios/news/discrepancies/graham` → `CompanyData` |

Requests are relative (`/api/...`, `/auth/...`); the dev proxy / nginx forward
them to the backend.

## Types (`types.ts`)

Mirror the backend JSON: `Company`, `PricePoint`, `FinancialFact`, `Ratio` (with
`period_type: 'annual' | 'quarterly'`), `NewsItem`, `Discrepancy`,
`GrahamAssessment`/`GrahamScore`, `CompanyData`, plus pagination helpers
`Page<T>`, `CompanyRow`, `ScreenRow`, and `ScreenFilters`.

> `period_type`/`statement` arrive **lowercase** — the backend renames its enums
> for exactly this. Client filters compare against `'annual'`/`'income'`.

## Format & label registry (`format.ts`)

Turns machine values/keys into human-readable output. Pure, fully unit-tested.

- `formatCurrency(value)` — `$383.3B` / `$97.0M` / `$950` / `-$2.0M`.
- `freshness(iso, nowMs)` — `fresh` (<2 days) / `stale` / `unknown` (null).
- `metricMeta` — per ratio key: `{ label, group, kind }`
  (e.g. `roe → {"Return on equity","Profitability","percent"}`,
  `current_ratio → {…,"ratio"}`, `book_value_per_share → {…,"currency"}`).
- `metricLabel(key)` / `metricGroup(key)` — human label + group, with a titleized
  fallback for unknown keys.
- `formatMetric(key, value)` — formats by kind: `percent` `12.6%`, `ratio` `2.69×`,
  `currency` via `formatCurrency`, else 2-dp plain.
- `lineItemLabel` + `statementItemLabel(key)` — statement line items
  (`NetIncome → "Net income"`, `StockholdersEquity → "Shareholders' equity"`, …).
- `statementLabel(kind)` — section names (`income → "Income statement"`, etc.).

These power the readable ratios/statements/compare displays and grouping.
