# REST API

axum router built by `app(Arc<Store>)` in `lib.rs`; handlers in `api.rs`. All
responses are JSON. Wrapped with `tower-http` middleware: 64 KiB body limit
(→ 413), 30 s timeout (→ 408), request tracing.

## Auth

- **Password hashing**: argon2 (`auth.rs`).
- **Sessions**: opaque random bearer token; the DB stores a sha256 hash with a
  30-day expiry. Clients send `Authorization: Bearer <token>`.
- **AuthUser extractor** (`api.rs`): resolves the bearer token → `user_id` for
  protected routes; missing/invalid/expired → `401`.
- **Brute-force throttle** (`net::LoginThrottle`, held on `Store`): 5 failed
  logins per email within 15 min → `429`; cleared on success.

## Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/health` | — | liveness `{status:"ok"}` |
| POST | `/auth/signup` | — | `{email,password}` → `201 {token}` (409 if taken) |
| POST | `/auth/login` | — | `{email,password}` → `{token}` (401 bad, 429 throttled) |
| POST | `/auth/logout` | bearer | invalidate the session → 204 |
| GET | `/auth/me` | ✓ | `{email}` |
| GET | `/api/companies?q=&limit=&offset=` | ✓ | paginated directory → `{rows:[{company,score}], total}` |
| GET | `/api/companies/:ticker` | ✓ | company record |
| GET | `/api/companies/:ticker/prices?from=&to=&limit=` | ✓ | OHLCV points |
| GET | `/api/companies/:ticker/facts?from=&to=&limit=` | ✓ | financial facts (all period types) |
| GET | `/api/companies/:ticker/ratios` | ✓ | ratios (all period types) |
| GET | `/api/companies/:ticker/news` | ✓ | headlines |
| GET | `/api/companies/:ticker/discrepancies` | ✓ | flagged mismatches |
| GET | `/api/companies/:ticker/graham` | ✓ | live full Graham assessment (criteria + numbers) |
| GET | `/api/companies/:ticker/summary` | ✓ | `{company, ratios, graham}` in one round trip |
| GET | `/api/screen?defensive=&net_net=&min_score=&limit=&offset=` | ✓ | ranked screener → `{rows:[{company,score}], total}` |
| GET | `/api/watchlist` / POST `/api/watchlist` / DELETE `/api/watchlist/:ticker` | ✓ | per-user watchlist |
| GET | `/api/runs` | ✓ | recent collection runs (observability) |

Pagination responses use `{ rows, total }`; ratios/facts return all period types
and the **frontend** filters by Annual/Quarterly client-side.

## Errors

- `404` unknown ticker; `401` auth; `409` duplicate signup; `429` login throttle;
  `413`/`408` middleware; `500` internal.
- Internal errors are **sanitized**: `internal()` logs the real `StoreError`
  server-side via `tracing::error!` and returns an opaque `"internal error"` body
  — no store/SQL detail leaks to clients.

## Notes

- The dashboard's frontend nginx + Vite dev proxy forward both `/api/` **and**
  `/auth/` to the backend **unchanged** (the backend serves both prefixes
  literally; never strip the prefix).
