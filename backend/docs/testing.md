# Testing & Quality Gates

## Workflow

Strict TDD: write a failing test → watch it fail (RED) → minimal code (GREEN) →
refactor. No production code without a failing test first. Determinism is enforced
by injecting the clock, HTTP, and DB (see [architecture.md](architecture.md));
**no live-network calls** in unit/integration tests — real source responses are
captured as fixtures under `tests/fixtures/`.

## Running

```
cd backend
cargo test                         # unit + integration
cargo clippy --all-targets -- -D warnings
cargo llvm-cov --ignore-filename-regex '(main|http)\.rs' \
      --fail-under-lines 99 --fail-under-functions 100
```

## Coverage gates

- **Functions 100% / lines ≥ 99%** on logic modules.
- Excluded as untestable I/O glue: `main.rs` (thin CLI bootstrap) and `http.rs`
  (real reqwest loop). Everything else — including `api.rs`, `store.rs`,
  `pipeline.rs`, all collectors' parsing — is covered.
- The ≥99% lines floor (vs 100%) absorbs a cargo-llvm-cov phantom-missed-line
  artifact over async generic functions, proven executed via `--text`.

## Test seams / helpers (`testutil.rs`)

- `FakeHttp::new(fixture)` — an `HttpClient` returning a fixed body and recording
  the requested URL. Used to test every collector offline.
- `temp_store()` — a fresh, migrated SQLite temp DB (auto-cleaned).
- `fixed_now()` — a constant timestamp for deterministic time-dependent logic.

## Patterns worth knowing

- Pure modules (`reconcile`, `ratios`, `graham`, `net`, `auth`, `config`,
  `domain`) are tested directly with crafted inputs covering each branch + guard.
- Collectors are tested against captured JSON/CSV/XML fixtures.
- `store`/`api`/`pipeline` use `temp_store` + `FakeHttp`; API handler tests drive
  the real axum router via `tower::ServiceExt::oneshot`.
- Hard-to-trigger glue (e.g. dnd-kit drag on the frontend, the reqwest loop here)
  is extracted into pure, directly-tested helpers rather than fought through the
  framework.
