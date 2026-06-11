# API Reference

All endpoints use JSON. Auth endpoints live under `/auth/`; data endpoints under `/api/`. The frontend dev proxy and nginx both forward both prefixes to the backend unchanged.

## Authentication

All `/api/` endpoints require a `Authorization: Bearer <token>` header. Tokens are issued by `/auth/login` and `/auth/signup`.

---

## Auth endpoints

### `POST /auth/signup`

Create a new user account.

**Request body:**
```json
{ "email": "user@example.com", "password": "secret" }
```

**Response `201`:**
```json
{ "token": "abc123..." }
```

**Errors:** `409` email already registered.

---

### `POST /auth/login`

Authenticate and get a session token.

**Request body:**
```json
{ "email": "user@example.com", "password": "secret" }
```

**Response `200`:**
```json
{ "token": "abc123..." }
```

**Errors:** `401` bad credentials. Throttled per email after 5 failures within a 15-minute window (`429`); a successful login clears the counter.

---

### `POST /auth/logout`

Invalidate the current session token.

**Headers:** `Authorization: Bearer <token>`

**Response:** `204`

---

### `GET /auth/me`

Return the authenticated user's email.

**Response `200`:**
```json
{ "email": "user@example.com" }
```

---

## Watchlist

### `GET /api/watchlist`

List the current user's watchlist companies.

**Response `200`:** Array of [Company](#company) objects.

---

### `POST /api/watchlist`

Add a ticker to the watchlist.

**Request body:**
```json
{ "ticker": "AAPL" }
```

**Response:** `204`

---

### `DELETE /api/watchlist/:ticker`

Remove a ticker from the watchlist.

**Response:** `204`

---

## Companies

### `GET /api/companies`

Paginated directory of all companies with their latest Graham score.

**Query params:**

| Param | Type | Description |
|---|---|---|
| `q` | string | Search in ticker and company name |
| `sort_by` | string | `ticker` \| `name` \| `industry` \| `score` |
| `sort_dir` | string | `asc` \| `desc` |
| `limit` | integer | Page size (server default 50; the web UI sends 25) |
| `offset` | integer | Pagination offset (default 0) |

**Response `200`:**
```json
{
  "rows": [{ "company": { ...Company }, "score": { ...GrahamScore } | null }],
  "total": 4200
}
```

---

### `GET /api/companies/:ticker`

Fetch a single company by ticker.

**Response `200`:** [Company](#company) object. `404` if not found.

---

### `GET /api/companies/:ticker/summary`

One round trip for the dashboard header: the company plus its ratios and persisted Graham score.

**Response `200`:**
```json
{
  "company": { ...Company },
  "ratios": [ { ...Ratio } ],
  "graham": { ...GrahamScore } | null
}
```

---

### `GET /api/companies/:ticker/prices`

Historical daily OHLCV prices, oldest-first.

**Query params (all optional):** `from=YYYY-MM-DD`, `to=YYYY-MM-DD`, `limit=N`.

**Response `200`:** Array of [PricePoint](#pricepoint).

---

### `GET /api/companies/:ticker/facts`

All stored financial facts (income, balance, cash flow) for a company.

**Query params (all optional):** `from=YYYY-MM-DD`, `to=YYYY-MM-DD`, `limit=N`.

**Response `200`:** Array of [FinancialFact](#financialfact).

---

### `GET /api/companies/:ticker/ratios`

Derived financial ratios (annual + quarterly), newest-first within each period type.

**Response `200`:** Array of [Ratio](#ratio).

---

### `GET /api/companies/:ticker/news`

Recent news items, newest-first.

**Response `200`:** Array of [NewsItem](#newsitem).

---

### `GET /api/companies/:ticker/discrepancies`

Cross-source value conflicts flagged above the discrepancy threshold.

**Response `200`:** Array of [Discrepancy](#discrepancy).

---

### `GET /api/companies/:ticker/graham`

Graham defensive-investor assessment (live, from stored facts).

**Response `200`:** [GrahamAssessment](#grahamassessment).

---

### `GET /api/companies/:ticker/peers`

Companies in the same sector, ranked by Graham score. Excludes the queried company.

**Query params:** none — the result is capped server-side at 20 peers.

**Response `200`:** Array of `{ "company": Company, "score": GrahamScore | null }`.

---

### `GET /api/companies/:ticker/note`

Retrieve the current user's private note for this company.

**Response `200`:**
```json
{ "body": "My analysis..." }
```
`body` is `null` if no note exists.

---

### `PUT /api/companies/:ticker/note`

Create or replace the note.

**Request body:**
```json
{ "body": "My analysis..." }
```

**Response:** `204`

---

### `DELETE /api/companies/:ticker/note`

Delete the note.

**Response:** `204`

---

## Screener

### `GET /api/screen`

Screen companies by Graham criteria and optional ratio filters. Returns ranked results.

**Query params:**

| Param | Type | Default | Description |
|---|---|---|---|
| `defensive` | bool | `false` | Only companies passing all 7 Graham criteria |
| `net_net` | bool | `false` | Only NCAV net-nets |
| `min_score` | integer | `0` | Minimum Graham score (0–7) |
| `sector` | string | — | Exact sector match |
| `min_pe` | float | — | Min P/E (latest annual) |
| `max_pe` | float | — | Max P/E (latest annual) |
| `min_roe` | float | — | Min ROE (latest annual) |
| `max_de` | float | — | Max debt/equity (latest annual) |
| `min_margin` | float | — | Min net margin (latest annual) |
| `sort_by` | string | score desc | `ticker` \| `graham_number` \| `margin_of_safety` |
| `sort_dir` | string | `asc` | `asc` \| `desc` |
| `limit` | integer | 50 | Page size |
| `offset` | integer | 0 | Pagination offset |

**Response `200`:**
```json
{
  "rows": [{ "company": { ...Company }, "score": { ...GrahamScore } }],
  "total": 42
}
```

---

## Sectors

### `GET /api/sectors`

Sector-level aggregates, ordered by average Graham score descending.

**Response `200`:** Array of [SectorStats](#sectorstats).

---

## Collection runs

### `GET /api/runs`

Recent collection run history (status, errors, timing).

**Response `200`:** Array of [CollectionRun](#collectionrun).

---

## Response shapes

### Company
```json
{
  "id": 1,
  "cik": "0000320193",
  "ticker": "AAPL",
  "name": "Apple Inc.",
  "exchange": "NASDAQ",
  "sector": "Technology",
  "industry": "Consumer Electronics",
  "description": "Apple designs...",
  "website": "https://www.apple.com"
}
```

### PricePoint
```json
{
  "company_id": 1,
  "date": "2024-01-15",
  "open": 185.0,
  "high": 186.5,
  "low": 183.2,
  "close": 185.9,
  "volume": 52000000,
  "source": "fmp"
}
```

### FinancialFact
```json
{
  "company_id": 1,
  "statement": "income",
  "line_item": "Revenue",
  "period_type": "annual",
  "period_end": "2023-09-30",
  "value": 383285000000.0,
  "source": "edgar",
  "fetched_at": "2024-01-15T10:00:00Z"
}
```

### Ratio
```json
{
  "company_id": 1,
  "period_end": "2023-09-30",
  "period_type": "annual",
  "metric": "pe",
  "value": 28.5,
  "computed_at": "2024-01-15T10:00:00Z"
}
```

### GrahamScore
```json
{
  "company_id": 1,
  "score": 4,
  "passes_defensive": false,
  "graham_number": 52.3,
  "ncav_per_share": -12.5,
  "margin_of_safety": 0.15,
  "net_net": false,
  "computed_at": "2024-01-15T10:00:00Z"
}
```

### GrahamAssessment
```json
{
  "criteria": [
    { "name": "Current ratio >= 2", "passed": true, "detail": "current ratio 2.5" },
    { "name": "No deficit last 10y", "passed": false, "detail": "deficit in 2020" }
  ],
  "score": 4,
  "graham_number": 52.3,
  "ncav_per_share": null,
  "margin_of_safety": 0.15,
  "net_net": false,
  "passes_defensive": false
}
```

### NewsItem
```json
{
  "company_id": 1,
  "title": "Apple Reports Q4 Results",
  "description": "Revenue beat expectations...",
  "url": "https://...",
  "source": "yahoo",
  "published_at": "2024-01-15T08:00:00Z"
}
```

### Discrepancy
```json
{
  "company_id": 1,
  "field": "Revenue",
  "period": "2023-09-30",
  "source_a": "edgar",
  "value_a": 383285000000.0,
  "source_b": "fmp",
  "value_b": 383500000000.0,
  "pct_diff": 0.056,
  "flagged_at": "2024-01-15T10:00:00Z"
}
```

### SectorStats
```json
{
  "sector": "Technology",
  "company_count": 312,
  "avg_score": 3.8,
  "pct_defensive": 0.12,
  "top_ticker": "MSFT"
}
```

### CollectionRun
```json
{
  "id": 1,
  "source": "edgar",
  "scope": "AAPL",
  "started_at": "2024-01-15T10:00:00Z",
  "finished_at": "2024-01-15T10:00:05Z",
  "status": "ok",
  "error": null
}
```
