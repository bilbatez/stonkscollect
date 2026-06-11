# StonksCollect

StonksCollect is a self-hosted Graham-style fundamental analysis platform for US equities. It collects data from multiple public sources, cross-checks them against SEC EDGAR as the canonical source of truth, stores history locally in SQLite, and serves a React dashboard for fundamental analysis.

## What it does

- **Collects** company facts (income statement, balance sheet, cash flow), prices, and news from SEC EDGAR, FMP, Yahoo Finance, and Finnhub.
- **Cross-checks** data across sources; flags discrepancies above configurable thresholds.
- **Computes** 14 derived financial ratios and a Benjamin Graham defensive-investor scorecard.
- **Serves** a React SPA with charts, screener, sector overview, peer comparison, notes, and CSV export.

## Quick start

```bash
# 1. Configure environment
cp backend/.env.example backend/.env
# Set FMP_API_KEY and FINNHUB_API_KEY (optional)

# 2. Bootstrap the SEC ticker universe
cargo run --bin stonkscollect_backend -- bootstrap

# 3. Collect a few tickers
cargo run --bin stonkscollect_backend -- collect --ticker AAPL,MSFT,KO

# 4. Serve the API
cargo run --bin stonkscollect_backend -- serve

# 5. Run the frontend (separate terminal)
cd frontend && npm run dev
```

Or with Docker Compose:

```bash
docker compose up
```

Open `http://localhost:5173` in your browser.

## Repository layout

```
backend/          Rust crate (lib + thin bin)
frontend/         React + Vite SPA
docs/             This documentation site
data/             SQLite database + Parquet exports (gitignored)
Makefile          Dev tasks (test, cov, demo, lint, e2e, up, down)
docker-compose.yml
```

## Serve these docs locally

```bash
# Python (no install required)
python3 -m http.server 3000 --directory docs
# or
npx serve docs
```

Open `http://localhost:3000`.
