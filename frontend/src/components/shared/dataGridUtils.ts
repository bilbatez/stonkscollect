import type { Dispatch, ReactNode, SetStateAction } from 'react'
import type { DragEndEvent } from '@dnd-kit/core'

/** A column definition for `DataGrid`. `sortValue` (when given) makes the column
 *  sortable + the basis for its text filter. */
export interface GridColumn<T> {
  id: string
  header: string
  cell: (row: T) => ReactNode
  sortValue?: (row: T) => string | number
  filter?: boolean
}

/** Move `activeId` to `overId`'s position. No-op if either id is absent or equal. */
export function applyReorder(order: string[], activeId: string, overId: string): string[] {
  const from = order.indexOf(activeId)
  const to = order.indexOf(overId)
  if (from === -1 || to === -1 || from === to) {
    return order
  }
  const next = order.slice()
  next.splice(to, 0, next.splice(from, 1)[0])
  return next
}

/** Apply a TanStack-style updater: call it if it's a function, else return it directly. */
export function applyUpdater<T>(updater: T | ((prev: T) => T), prev: T): T {
  return typeof updater === 'function'
    ? (updater as (prev: T) => T)(prev)
    : updater
}

/** dnd-kit id (string|number|null) → string ('' when absent). */
export function idOf(item: { id: string | number } | null | undefined): string {
  return item ? String(item.id) : ''
}

/** Build the dnd-kit drag-end handler that reorders columns. Extracted so the
 *  reorder wiring is unit-testable (dnd-kit can't dispatch real drags in jsdom). */
export function makeDragEndHandler(setOrder: Dispatch<SetStateAction<string[]>>) {
  return (e: DragEndEvent) => setOrder((o) => applyReorder(o, idOf(e.active), idOf(e.over)))
}
