import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, test } from 'vitest'
import { DataGrid } from './DataGrid'
import { applyReorder, idOf, makeDragEndHandler, type GridColumn } from './dataGridUtils'
import type { DragEndEvent } from '@dnd-kit/core'

interface Row {
  name: string
  num: number
}

const columns: GridColumn<Row>[] = [
  { id: 'name', header: 'Name', sortValue: (r) => r.name, filter: true, cell: (r) => r.name },
  { id: 'num', header: 'Num', sortValue: (r) => r.num, cell: (r) => String(r.num) },
  // no sortValue + no filter -> exercises the non-sortable, non-filterable branches
  { id: 'act', header: 'Action', cell: (r) => <button type="button">go {r.name}</button> },
]

const rows: Row[] = [
  { name: 'Beta', num: 2 },
  { name: 'Alpha', num: 10 },
]

function nameOrder(): string[] {
  return screen.getAllByText(/^(Alpha|Beta)$/).map((e) => e.textContent ?? '')
}

test('applyReorder moves an id and no-ops on equal/missing ids', () => {
  expect(applyReorder(['a', 'b', 'c'], 'a', 'c')).toEqual(['b', 'c', 'a'])
  expect(applyReorder(['a', 'b'], 'a', 'a')).toEqual(['a', 'b']) // equal
  expect(applyReorder(['a', 'b'], 'x', 'b')).toEqual(['a', 'b']) // missing
})

test('idOf returns the id or empty string', () => {
  expect(idOf({ id: 'x' })).toBe('x')
  expect(idOf({ id: 7 })).toBe('7')
  expect(idOf(null)).toBe('')
})

test('renders rows; non-sortable/non-filterable columns have no controls', () => {
  render(<DataGrid columns={columns} rows={rows} getRowId={(r) => r.name} />)
  expect(screen.getByText('Alpha')).toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'go Beta' })).toBeInTheDocument()
  // filterable column has a filter box; non-filterable ones don't
  expect(screen.getByLabelText('filter name')).toBeInTheDocument()
  expect(screen.queryByLabelText('filter num')).not.toBeInTheDocument()
  expect(screen.queryByLabelText('filter act')).not.toBeInTheDocument()
})

test('sorting toggles ascending then descending', async () => {
  render(<DataGrid columns={columns} rows={rows} getRowId={(r) => r.name} />)
  expect(nameOrder()).toEqual(['Beta', 'Alpha']) // input order
  await userEvent.click(screen.getByText('Name'))
  expect(nameOrder()).toEqual(['Alpha', 'Beta']) // asc
  await userEvent.click(screen.getByText('Name'))
  expect(nameOrder()).toEqual(['Beta', 'Alpha']) // desc
})

test('per-column filter narrows the rows', async () => {
  render(<DataGrid columns={columns} rows={rows} getRowId={(r) => r.name} />)
  await userEvent.type(screen.getByLabelText('filter name'), 'alp')
  expect(screen.getByText('Alpha')).toBeInTheDocument()
  expect(screen.queryByText('Beta')).not.toBeInTheDocument()
})

test('drag-end handler reorders columns (and no-ops without a target)', () => {
  let order = ['name', 'num', 'act']
  const setOrder = (u: unknown) => {
    order = (u as (o: string[]) => string[])(order)
  }
  const onDragEnd = makeDragEndHandler(setOrder as never)
  onDragEnd({ active: { id: 'name' }, over: { id: 'act' } } as DragEndEvent)
  expect(order).toEqual(['num', 'act', 'name'])
  onDragEnd({ active: { id: 'name' }, over: null } as DragEndEvent) // dropped nowhere
  expect(order).toEqual(['num', 'act', 'name']) // unchanged
})

test('the grid renders a drag handle per column', () => {
  render(<DataGrid columns={columns} rows={rows} getRowId={(r) => r.name} />)
  expect(screen.getByLabelText('reorder name')).toBeInTheDocument()
  expect(screen.getByLabelText('reorder act')).toBeInTheDocument()
})

test('empty state, default and custom', () => {
  const { rerender } = render(<DataGrid columns={columns} rows={[]} getRowId={(r) => r.name} />)
  expect(screen.getByText('No data.')).toBeInTheDocument()
  rerender(<DataGrid columns={columns} rows={[]} getRowId={(r) => r.name} empty="Nothing here." />)
  expect(screen.getByText('Nothing here.')).toBeInTheDocument()
})
