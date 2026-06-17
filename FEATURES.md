# StonksCollect — Feature Catalog

The single full-stack map of **what the app does** and **where each feature
lives**. Every row links a user-facing capability to its backend route, the
store method behind it, the frontend component that renders it, and the domain
type that flows between them. Start here, then drill into the deeper docs:
[Architecture](docs/architecture.md) · [API Reference](docs/api-reference.md) ·
[Collectors](docs/collectors.md) · [Data Models](docs/data-models.md) ·
[Financial Data](docs/financials.md) · [Frontend Features](docs/features.md) ·
[Deployment](docs/deployment.md) · [Roadmap / what's missing](docs/roadmap.md).

**One-line product:** collect US-equity fundamentals + prices + news from
multiple sources, cross-check them (SEC EDGAR canonical), store history locally,
and serve a Yahoo-Finance-style dashboard for fundamental analysis. Not realtime
— latest-and-stored.

Paths: backend under `backend/src/`, frontend under `frontend/src/`. All `/api/`
endpoints require `Authorization: Bearer <token>`.

---

## 1. Accounts & auth
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| Signup / login (Argon2id, 30-day bearer sessions) | `POST /auth/signup`, `POST /auth/login` → `api.rs`; `auth.rs` (hash + tokens); brute-force throttle in `net.rs` `LoginThrottle` | `components/auth/AuthForm.tsx`; token in `localStorage` via `api.ts` | — |
| Current user / logout | `GET /auth/me`, `POST /auth/logout` | `App.tsx` auth dispatch | — |

## 2. Company directory & search
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| Paginated, searchable, sortable directory (excludes index pseudo-companies) | `GET /api/companies` → `store::list_companies` (`store/analytics.rs`) | `components/pages/AllStocks.tsx` + `shared/DataGrid.tsx` | `Company`, `GrahamScore` |
| Single company | `GET /api/companies/:ticker` → `store::get_company` | `components/pages/CompanyView.tsx` | `Company` |
| One-roundtrip detail header | `GET /api/companies/:ticker/summary` | `CompanyView.tsx` | `CompanySummary` |
| Autocomplete ticker search | reuses `GET /api/companies?q=` | `pages/CompareView.tsx` | `CompanyRow` |

## 3. Company detail — quote & key stats
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| Quote header (last close, day change, as-of) | derived in frontend from `/prices` | `panels/QuoteHeader.tsx`; `quote.ts` `computeQuote` | `Quote` |
| **Period-return chips** (1D/5D/1M/6M/YTD/1Y/5Y) | derived | `QuoteHeader.tsx`; `quote.ts` `computePeriodReturns` | `PeriodReturn` |
| 16-card key statistics | derived | `panels/KeyStatsPanel.tsx`; `quote.ts` `computeKeyStats` | `KeyStats` |
| **52-week range gauge** | derived | `shared/WeekRangeBar.tsx` | — |
| 8-card metrics summary | from ratios + Graham | `panels/MetricsSummary.tsx` | `Ratio`, `GrahamScore` |
| Data-freshness badge | client clock vs latest price date | `shared/FreshnessBadge.tsx` | — |

## 4. Price charts (ECharts, lazy)
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| Candlestick / line price chart + **volume sub-chart** + **SMA 50/200 toggle** | `GET /api/companies/:ticker/prices` (OHLCV, oldest-first) | `charts/PriceChart.tsx`; `chartData.ts` `movingAverage` | `PricePoint` |
| **Range presets** 1M/6M/YTD/1Y/5Y/MAX | client filter | `shared/RangeToggle.tsx`; `chartData.ts` `pricesForRange` | — |
| Graham Number overlay (price vs Graham #, buying-zone shading) | derived from facts + ratios | `charts/GrahamChart.tsx`; `chartData.ts` `grahamChartData` | — |

## 5. Fundamentals
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| Financial statements (income/balance/cashflow) + YoY growth + **TTM column** (quarterly) + CSV | `GET /api/companies/:ticker/facts` | `panels/StatementTable.tsx`; `chartData.ts` `ttmColumn` | `FinancialFact` |
| Derived ratios table (annual/quarterly) + CSV | `GET /api/companies/:ticker/ratios` → `ratios.rs` | `panels/RatiosPanel.tsx` | `Ratio` |
| Income trend bar chart | from facts | `charts/IncomeChart.tsx`; `chartData.ts` `incomeChartData` | — |
| Ratio trend line chart | from ratios | `charts/RatioChart.tsx`; `chartData.ts` `ratioChartData` | — |
| **Dividend history** (annual DPS) | from facts | `panels/DividendPanel.tsx` | `FinancialFact` |

Ratios computed (`ratios.rs`): `pe`, `pb`, `roe`, `net_margin`, `gross_margin`,
`operating_margin`, `debt_to_equity`, `current_ratio`, `book_value_per_share`,
`payout_ratio`, `working_capital`, `free_cash_flow`, `fcf_margin`. See
[financials.md](docs/financials.md).

## 6. Valuation — Graham (8 defensive criteria)
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| Defensive scorecard (8 criteria, score 0–8), Graham Number, NCAV, margin-of-safety, net-net | `GET /api/companies/:ticker/graham` → `graham.rs` (pure) | `panels/GrahamScorecard.tsx` | `GrahamAssessment` |
| Persisted score (cached, recomputed each collect) | `graham_scores` table | shown in grids | `GrahamScore` |
| Screener (Graham + ratio filters, ranked) | `GET /api/screen` → `store::screen` | `pages/Screener.tsx` + `DataGrid` | `ScreenRow` |

## 7. Compare
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| Multi-ticker metric table | per-ticker `summary`/`ratios` | `pages/CompareView.tsx` + `shared/Compare.tsx` | `Ratio` |
| **Normalized % overlay chart** (rebased to first common date) | `/prices` per ticker | `charts/CompareChart.tsx`; `chartData.ts` `normalizedReturns` | `PricePoint` |

## 8. Market dashboard
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| **Index summary cards** (S&P/Nasdaq/Dow) | `GET /api/markets/summary` → `store::index_changes`; seeded by `pipeline::seed_indices` (`is_index`) | `panels/MarketSummary.tsx` | `MoverRow` |
| Market movers (gainers/losers/most-active) | `GET /api/movers` → `store::day_changes` + `domain::select_movers` | `pages/MoversView.tsx` | `Movers`/`MoverRow` |
| **Home trending strip** (top gainers/losers chips) | reuses `/api/movers` | `panels/TrendingStrip.tsx` | `Movers` |
| Sector overview + **heatmap** | `GET /api/sectors` → `store::get_sectors` | `pages/SectorOverview.tsx`; `format.ts` `scoreHeatColor` | `SectorStats` |

## 9. Watchlist & notes
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| Watchlist add/remove/list | `GET/POST /api/watchlist`, `DELETE /api/watchlist/:ticker` | `layout/Watchlist.tsx` | `Company` |
| Watchlist with live quotes | `GET /api/watchlist/quotes` → `store::watch_quotes` | `layout/Watchlist.tsx` | `WatchQuote` |
| Private per-company note | `GET/PUT/DELETE /api/companies/:ticker/note` | `panels/NotePanel.tsx` | `Note` |

## 10. News
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| Per-company headlines (Yahoo RSS keyless + Finnhub key), SHA-256 deduped | `GET /api/companies/:ticker/news` → `collectors/news.rs` | `panels/NewsFeed.tsx` | `NewsItem` |

## 11. Ownership / holders
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| **Insider holders from EDGAR Form 4** (keyless): submissions feed → Form 4 XML → owners + latest shares | `GET /api/companies/:ticker/holders` → `store::get_ownership`; `collectors/edgar_ownership.rs` (`OwnershipCollector`/`HolderSource`), saved via `store::save_ownership` on the `collect` CLI | `panels/HoldersPanel.tsx` | `OwnershipHolding` |

> 13F institutional holdings are **not** built — see [roadmap.md](docs/roadmap.md).

## 12. Cross-source reconciliation
| Feature | Backend | Frontend | Type |
|---|---|---|---|
| Canonical selection (EDGAR wins) + discrepancy flagging above threshold | `reconcile.rs` (pure); `GET /api/companies/:ticker/discrepancies` | `panels/DiscrepancyPanel.tsx` + `DataGrid` | `Discrepancy` |
| Per-source collection errors | `GET /api/companies/:ticker/errors` → `source_errors` table | (consumed by tooling) | `SourceError` |

## 13. Collection pipeline, CLI & observability
| Feature | Backend | Notes |
|---|---|---|
| CLI: `bootstrap` / `collect [--ticker|--all]` / `serve` / `enrich` / `seed-admin` | `main.rs` (thin, coverage-excluded) | `bootstrap` also `seed_indices`; `collect` also best-effort Form 4 holders |
| Per-company + batch collect, recompute ratios+Graham | `pipeline/{collect,orchestrate}.rs` | facts+prices+news → reconcile → persist → metrics |
| Tiered background collection (serve loop) | `scheduler.rs` (cron tiers + `run_tracked`) | graceful shutdown on SIGTERM |
| Run history | `GET /api/runs` → `collection_runs` | `CollectionRun` |
| Conditional GET (ETag) + incremental fetch | `http_cache` table; `collectors/edgar.rs`, `yahoo.rs` | reduces redundant downloads |

## 14. Storage & export
| Feature | Backend | Notes |
|---|---|---|
| SQLite (WAL) single file | `./data/stonks.db`; `store/` split by aggregate | backup = copy the `.db` |
| Scheduled Parquet export | `store/analytics.rs` `export_prices_parquet` (Arrow/Parquet) | weekly tier |

---

## Data sources (US only)
- **SEC EDGAR** (keyless) — canonical fundamentals (companyfacts), profiles
  (submissions), **insider ownership** (Form 4 XML).
- **Yahoo Finance** chart API (keyless) — daily OHLCV for equities **and indices**
  (`^GSPC`/`^IXIC`/`^DJI`); `assetProfile` metadata; per-symbol news RSS.
- **Financial Modeling Prep** (`FMP_API_KEY`) — prices/OHLCV + income facts (cross-check).
- **Finnhub** (`FINNHUB_API_KEY`) — company news.
- **HTML scrape** — gap-fill fallback (robots.txt-aware, rate-limited, cached).

Keyless path (no API keys): EDGAR + Yahoo. Conflicts: store every source's value,
EDGAR canonical, flag discrepancies above threshold.

## Conventions (for any agent editing this repo)
Strict TDD (red→green→refactor); coverage gate **backend functions 100% / lines
≥99%** (main.rs/http.rs excluded) and **frontend 100%** (`src/charts/` excluded —
keep chart logic in pure `quote.ts`/`chartData.ts` helpers); inject HTTP/DB/clock,
no live network in tests; prefix shell commands with `rtk`. See `CLAUDE.md` and
`AGENTS.md`.
