# Configuration

All runtime config is environment-driven, parsed once by `Config::parse` (a pure
function over a key getter). `main.rs` loads a `.env` file (via `dotenvy`,
searching cwd upward) before reading real env vars (real env wins). See
`.env.example` at the repo root.

| Variable | Default | Purpose |
|----------|---------|---------|
| `DATABASE_URL` | `sqlite:///data/stonks.db` | SQLite file (relative paths resolve from the run dir; `/data/...` inside containers) |
| `PORT` | `8080` | REST API port (`serve`) |
| `USER_AGENT` | `stonkscollect (contact@example.com)` | **SEC requires** a descriptive UA with a real contact; also satisfies Yahoo (rejects empty UA). Set to `you@domain.com` |
| `TICKERS` | — | comma-separated default tickers for `collect`/scheduler when not `--all` |
| `COLLECT_ALL` | `false` | collect the whole bootstrapped universe (overrides `TICKERS`) |
| `REQUEST_DELAY_MS` | `150` | **per-host** request spacing (≈ 6 req/s/host). Lowering this is the biggest speedup lever |
| `COLLECT_CONCURRENCY` | `8` | companies fetched in parallel; with per-host limiters this parallelizes across EDGAR/Yahoo/vendors |
| `COLLECT_MAX_AGE_HRS` | unset | incremental skip: don't recollect companies fresher than N hours; unset = always |
| `RECONCILE_THRESHOLD` | `0.05` | relative cross-source diff above which a discrepancy is flagged (5%) |
| `GRAHAM_MIN_REVENUE` | `500000000` | Graham "adequate size" revenue floor (USD) |
| `FMP_API_KEY` | — | enables FMP price/fact source |
| `FINNHUB_API_KEY` | — | enables Finnhub news source |

Invalid numeric values fall back to defaults; `COLLECT_CONCURRENCY=0` is rejected
(→ default). Truthy values for `COLLECT_ALL`: `1`, `true`, `yes` (case-insensitive).

## Tuning a big `--all` run

- Raise `COLLECT_CONCURRENCY` (e.g. 16–32) and lower `REQUEST_DELAY_MS` (e.g. 120).
- Keep `REQUEST_DELAY_MS ≥ ~100` for the EDGAR host (SEC fair-access ≈ 10 req/s).
- Yahoo is unofficial — too aggressive earns 429s; raise the delay if prices fail.
- Set `COLLECT_MAX_AGE_HRS` (e.g. 168) so re-runs resume instead of re-fetching.
