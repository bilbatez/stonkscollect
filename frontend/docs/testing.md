# Frontend Testing & Quality Gates

## Stacks

- **Vitest** + **@testing-library/react** + **user-event** (jsdom) — unit/component.
- **Playwright** — end-to-end, route-mocked (no real backend).

## Running

```
cd frontend
npm run test:run     # vitest once
npm run coverage     # vitest + v8 coverage (gates at 100%)
npm run build        # tsc -b + vite build (type-checks tests too)
npm run lint         # eslint
npm run e2e          # playwright
```

## Coverage gate

- **100%** statements / branches / functions / lines.
- Excluded: `src/main.tsx`, `src/charts/**` (ECharts canvas glue), `src/types.ts`,
  `*.test.*`, `src/test/**`.
- Vitest config inlines `@mui`, `@emotion`, and `react-transition-group`
  (`test.server.deps.inline`) because MUI's `.mjs` does an extensionless directory
  import the native ESM resolver rejects.

## Testing conventions

- Query by **role / accessible label / visible text**, never CSS classes or DOM
  shape — so the MUI markup and the TanStack/dnd-kit grid can change underneath
  without breaking tests, as long as accessible names stay stable.
- The API module is mocked (`vi.mock('../api')`); `PriceChart` is mocked to a stub
  `data-testid="price-chart"`.
- Hard-to-drive glue is extracted into pure helpers and unit-tested directly:
  - **dnd-kit reorder** — `applyReorder`/`idOf`/`makeDragEndHandler` tested as pure
    functions (jsdom can't perform a real drag).
  - **per-column sort accessors** — exercised by clicking each sortable header in
    the grid tests so the `sortValue` closures run.
- `tsconfig.app.json` includes `vitest/globals` + `@testing-library/jest-dom` in
  `types` so `tsc -b` type-checks `*.test.tsx` (don't remove or the build breaks).

## End-to-end (`e2e/`)

- `smoke.spec.ts` — app loads, shows the title.
- `dashboard.spec.ts` — route-mocked: log in → All Stocks lists a company → open it
  → statements + Graham scorecard render. Mocks `/auth/*`, `/api/companies?...`
  (the paginated directory), and the per-company endpoints. Selectors are
  role/text-based (resilient to markup changes).
