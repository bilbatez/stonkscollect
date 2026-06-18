import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, test, vi } from 'vitest'
import { DataGrid } from './shared/DataGrid'
import { applyReorder, applyUpdater, idOf, makeDragEndHandler, type GridColumn } from './shared/dataGridUtils'
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

test('applyUpdater calls function or returns value directly', () => {
  expect(applyUpdater((n: number) => n + 1, 5)).toBe(6)
  expect(applyUpdater(42, 0)).toBe(42)
})

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

test('onSortChange fires with column id and direction, then null when cleared', async () => {
  const onSortChange = vi.fn()
  render(<DataGrid columns={columns} rows={rows} getRowId={(r) => r.name} onSortChange={onSortChange} />)
  await userEvent.click(screen.getByText('Name'))
  expect(onSortChange).toHaveBeenCalledWith('name', 'asc')
  await userEvent.click(screen.getByText('Name'))
  expect(onSortChange).toHaveBeenCalledWith('name', 'desc')
  // third click clears sort; first is undefined → col is null
  await userEvent.click(screen.getByText('Name'))
  expect(onSortChange).toHaveBeenCalledWith(null, 'asc')
})

test('onFilterChange fires with the non-empty column filter map; clearing drops the key', async () => {
  const onFilterChange = vi.fn()
  render(
    <DataGrid columns={columns} rows={rows} getRowId={(r) => r.name} onFilterChange={onFilterChange} />,
  )
  await userEvent.type(screen.getByLabelText('filter name'), 'al')
  expect(onFilterChange).toHaveBeenLastCalledWith({ name: 'al' })
  // clearing the input emits an empty map (the empty value is skipped)
  await userEvent.clear(screen.getByLabelText('filter name'))
  expect(onFilterChange).toHaveBeenLastCalledWith({})
})

test('with onFilterChange, client-side filtering is off (server is authoritative)', async () => {
  const onFilterChange = vi.fn()
  render(
    <DataGrid columns={columns} rows={rows} getRowId={(r) => r.name} onFilterChange={onFilterChange} />,
  )
  // Typing a value that matches neither row would client-side hide both rows,
  // but with onFilterChange the rows passed in stay rendered (server filters).
  await userEvent.type(screen.getByLabelText('filter name'), 'zzz')
  expect(screen.getByText('Alpha')).toBeInTheDocument()
  expect(screen.getByText('Beta')).toBeInTheDocument()
})

test('empty state, default and custom', () => {
  const { rerender } = render(<DataGrid columns={columns} rows={[]} getRowId={(r) => r.name} />)
  expect(screen.getByText('No data.')).toBeInTheDocument()
  rerender(<DataGrid columns={columns} rows={[]} getRowId={(r) => r.name} empty="Nothing here." />)
  expect(screen.getByText('Nothing here.')).toBeInTheDocument()
})
