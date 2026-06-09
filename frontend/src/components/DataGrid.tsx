import { useMemo, useState } from 'react'
import {
  Box,
  Paper,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TableSortLabel,
  TextField,
  Typography,
} from '@mui/material'
import DragIndicatorIcon from '@mui/icons-material/DragIndicator'
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
} from '@dnd-kit/core'
import {
  SortableContext,
  horizontalListSortingStrategy,
  sortableKeyboardCoordinates,
  useSortable,
} from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import {
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getSortedRowModel,
  useReactTable,
  type ColumnDef,
  type ColumnFiltersState,
  type Header,
  type SortingState,
} from '@tanstack/react-table'
import { makeDragEndHandler, type GridColumn } from './dataGridUtils'

/** Sortable, per-column-filterable, drag-reorderable grid (TanStack + dnd-kit,
 *  MUI-rendered). Client-side over the rows it's given. */
export function DataGrid<T>({
  columns,
  rows,
  getRowId,
  empty = 'No data.',
}: {
  columns: GridColumn<T>[]
  rows: T[]
  getRowId: (row: T) => string
  empty?: string
}) {
  const [sorting, setSorting] = useState<SortingState>([])
  const [filters, setFilters] = useState<ColumnFiltersState>([])
  const [order, setOrder] = useState<string[]>(columns.map((c) => c.id))

  const colDefs = useMemo<ColumnDef<T>[]>(
    () =>
      columns.map((c) => ({
        id: c.id,
        header: c.header,
        accessorFn: (row: T) => (c.sortValue ? c.sortValue(row) : ''),
        cell: (info) => c.cell(info.row.original),
        enableSorting: c.sortValue !== undefined,
        enableColumnFilter: c.filter === true,
        filterFn: 'includesString',
      })),
    [columns],
  )

  // TanStack returns fresh fns each render; we don't pass them to memoized
  // children, so the React Compiler skip is safe here.
  // eslint-disable-next-line react-hooks/incompatible-library
  const table = useReactTable({
    data: rows,
    columns: colDefs,
    state: { sorting, columnFilters: filters, columnOrder: order },
    onSortingChange: setSorting,
    onColumnFiltersChange: setFilters,
    onColumnOrderChange: setOrder,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getRowId,
  })

  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  const handleDragEnd = makeDragEndHandler(setOrder)

  if (rows.length === 0) {
    return <Typography color="text.secondary">{empty}</Typography>
  }

  const headers = table.getHeaderGroups()[0].headers
  return (
    <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
      <TableContainer component={Paper} variant="outlined">
        <Table size="small">
          <TableHead>
            <TableRow>
              <SortableContext items={order} strategy={horizontalListSortingStrategy}>
                {headers.map((h) => (
                  <HeaderCell key={h.column.id} header={h} />
                ))}
              </SortableContext>
            </TableRow>
          </TableHead>
          <TableBody>
            {table.getRowModel().rows.map((r) => (
              <TableRow key={r.id} hover>
                {r.getVisibleCells().map((cell) => (
                  <TableCell key={cell.id}>
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </TableCell>
                ))}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </TableContainer>
    </DndContext>
  )
}

function HeaderCell<T>({ header }: { header: Header<T, unknown> }) {
  const col = header.column
  const { attributes, listeners, setNodeRef, transform, transition } = useSortable({ id: col.id })
  const style = { transform: CSS.Translate.toString(transform), transition }
  const label = col.columnDef.header as string

  return (
    <TableCell ref={setNodeRef} style={style} sx={{ verticalAlign: 'top' }}>
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
        <Box
          {...attributes}
          {...listeners}
          aria-label={`reorder ${col.id}`}
          sx={{ cursor: 'grab', display: 'flex' }}
        >
          <DragIndicatorIcon fontSize="small" color="disabled" />
        </Box>
        {col.getCanSort() ? (
          <TableSortLabel
            active={col.getIsSorted() !== false}
            direction={col.getIsSorted() === 'desc' ? 'desc' : 'asc'}
            onClick={col.getToggleSortingHandler()}
          >
            {label}
          </TableSortLabel>
        ) : (
          <span>{label}</span>
        )}
      </Box>
      {col.getCanFilter() && (
        <TextField
          variant="standard"
          placeholder="filter"
          value={(col.getFilterValue() as string) ?? ''}
          onChange={(e) => col.setFilterValue(e.target.value)}
          slotProps={{ htmlInput: { 'aria-label': `filter ${col.id}` } }}
          sx={{ mt: 0.5 }}
        />
      )}
    </TableCell>
  )
}
