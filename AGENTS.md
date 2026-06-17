# AGENTS.md

Conventions for AI coding agents (and humans) working in this repo. For the
full project map see `README.md` and `CLAUDE.md`; for the full-stack feature
catalog see `FEATURES.md`; for what's not built see `docs/roadmap.md`.

## Golden rules

1. **Strict TDD.** Write a failing test, watch it fail, then write the minimal
   code to pass. No production code without a failing test first.
2. **Coverage gates must stay green.** Backend: functions **100%**, lines
   **≥99%** (`cargo llvm-cov`). Frontend: **100%** statements/branches/functions/
   lines (`vitest`). I/O glue is excluded by config, never silently skipped.
3. **No `unwrap()`/`expect()` in production paths** (tests may; `expect` is OK
   only for compile-time invariants like static selectors). Errors are typed
   (`thiserror`).
4. **Match the surrounding style.** Small single-purpose functions; collectors
   behind `HttpClient`/`FactSource`; pure logic (reconcile) free of I/O.

## Where things go

| You want to… | Edit |
|--------------|------|
| add a fundamentals source | implement `FactSource` in `backend/src/collectors/`, register in the ingest call |
| change DB shape | add a migration in `backend/migrations/` + store methods + tests |
| add an API route | `backend/src/api.rs` (+ handler test via `tower::oneshot`) |
| add a dashboard panel | `frontend/src/components/` (+ vitest test) |
| add a chart | `frontend/src/charts/` (coverage-excluded; mock it in component tests) |
| share a test helper | `backend/src/testutil.rs` |

## Commands

```bash
# Backend (run from backend/)
cargo test
cargo clippy --all-targets -- -D warnings
cargo llvm-cov --ignore-filename-regex '(main|http)\.rs' \
  --fail-under-lines 99 --fail-under-functions 100

# Frontend (run from frontend/)
npm run test:run
npm run coverage          # enforces 100% thresholds
npm run lint
npm run e2e               # Playwright

# Or from repo root
make test | make cov | make lint | make e2e
```

## Gotchas

- **cwd:** `cargo` commands need to run inside `backend/`; git commands run from
  the repo root (which resets the shell cwd).
- **Coverage of async generic fns:** `cargo llvm-cov` reports a few phantom
  "missed lines" that `cargo llvm-cov --text` proves execute. Don't contort code
  to chase them — that's why the lines floor is 99, not 100. Functions stays 100.
- **Derives count toward function coverage.** If you add `Serialize`/`Clone`/
  `Debug`, exercise it in a test or coverage drops.
- **`docker compose`** isn't required for backend/frontend dev or tests.
- **Commits:** TDD per change; keep gates green before committing.
