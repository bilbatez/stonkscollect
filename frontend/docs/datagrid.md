# DataGrid

A reusable, sortable / per-column-filterable / drag-column-reorderable grid,
rendered with MUI Table primitives. Built on **TanStack Table v8** (sorting +
filtering models) and **dnd-kit** (header drag reorder). MIT-licensed (no MUI Pro).

Files:
- `components/DataGrid.tsx` — the component (+ internal `HeaderCell`).
- `components/dataGridUtils.ts` — pure helpers + the `GridColumn` type (kept
  separate so they're unit-testable and so the component file only exports a
  component, satisfying the react-refresh lint rule).

## API

```ts
interface GridColumn<T> {
  id: string
  header: string
  cell: (row: T) => ReactNode      // how to render the cell
  sortValue?: (row: T) => string | number  // present ⇒ sortable + the filter basis
  filter?: boolean                 // show a per-column text filter
}

<DataGrid
  columns={GridColumn<T>[]}
  rows={T[]}
  getRowId={(row) => string}
  empty="No data."                 // empty-state message
/>
```

## Features

- **Sort** — click a column header (`TableSortLabel`); cycles asc → desc. Only
  columns with `sortValue` are sortable.
- **Filter** — a `TextField` under each `filter: true` header; case-insensitive
  substring (`includesString`) over that column's `sortValue`.
- **Reorder** — each header has a drag handle (`aria-label="reorder <id>"`); drag
  to reorder columns (dnd-kit `SortableContext`, horizontal).

State (sorting / column filters / column order) is local to the grid.

## Pure helpers (`dataGridUtils.ts`)

- `applyReorder(order, activeId, overId)` — move a column id; no-op on equal/missing.
- `idOf(item)` — dnd-kit id → string (`''` when absent).
- `makeDragEndHandler(setOrder)` — builds the dnd-kit `onDragEnd` callback. Extracted
  precisely so it can be unit-tested directly: dnd-kit can't dispatch a real drag in
  jsdom, so the reorder logic is verified via these pure functions rather than a
  flaky simulated drag.

## Scope & limitations

- Applied to the flat row grids: **AllStocks**, **Screener**, **DiscrepancyPanel**.
- Sort/filter/reorder are **client-side over the rows passed in**. AllStocks and
  Screener are server-paginated (25/page) and keep the server-side search, so
  grid sort/filter act on the **current page**, not the whole 10k universe.
  Global sort/filter would require server-side params on `/api/companies` and
  `/api/screen` — a possible future enhancement.
- The date-pivot panels (Ratios/Statements/Compare) deliberately don't use
  DataGrid; they're grouped-by-date matrices.
