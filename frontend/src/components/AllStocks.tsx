import { useEffect, useState } from 'react'
import { Box, Button, Chip, TablePagination, TextField } from '@mui/material'
import { listCompanies } from '../api'
import { PAGE_SIZE } from '../constants'
import type { CompanyRow } from '../types'
import { DataGrid } from './DataGrid'
import type { GridColumn } from './dataGridUtils'

interface Props {
  onSelect: (ticker: string) => void
  onAdd: (ticker: string) => void
}

/** Paginated directory of every company with its Graham score. The page is a
 *  sortable / filterable / column-reorderable grid. Sorting triggers a server
 *  re-fetch so the full dataset is sorted, not just the visible page. */
export function AllStocks({ onSelect, onAdd }: Props) {
  const [rows, setRows] = useState<CompanyRow[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(0)
  const [q, setQ] = useState('')
  const [sortBy, setSortBy] = useState<string | null>(null)
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('asc')

  useEffect(() => {
    void listCompanies(q, sortBy, sortDir, PAGE_SIZE, page * PAGE_SIZE).then((p) => {
      setRows(p.rows)
      setTotal(p.total)
    })
  }, [q, page, sortBy, sortDir])

  const columns: GridColumn<CompanyRow>[] = [
    {
      id: 'ticker',
      header: 'Ticker',
      sortValue: (r) => r.company.ticker,
      filter: true,
      cell: (r) => (
        <Button size="small" onClick={() => onSelect(r.company.ticker)}>
          {r.company.ticker}
        </Button>
      ),
    },
    { id: 'name', header: 'Name', sortValue: (r) => r.company.name, filter: true, cell: (r) => r.company.name },
    {
      id: 'industry',
      header: 'Industry',
      sortValue: (r) => r.company.industry ?? '',
      filter: true,
      cell: (r) => r.company.industry ?? '—',
    },
    {
      id: 'score',
      header: 'Graham score',
      sortValue: (r) => (r.score ? r.score.score : -1),
      cell: (r) => (r.score === null ? '—' : <Chip size="small" label={`${r.score.score}/8`} />),
    },
    {
      id: 'watch',
      header: 'Watchlist',
      cell: (r) => (
        <Button size="small" aria-label={`watch ${r.company.ticker}`} onClick={() => onAdd(r.company.ticker)}>
          Watch
        </Button>
      ),
    },
  ]

  return (
    <Box>
      <TextField
        size="small"
        fullWidth
        placeholder="Search ticker or name"
        value={q}
        onChange={(e) => {
          setPage(0)
          setQ(e.target.value)
        }}
        slotProps={{ htmlInput: { 'aria-label': 'search stocks' } }}
        sx={{ mb: 2 }}
      />
      <DataGrid
        columns={columns}
        rows={rows}
        getRowId={(r) => r.company.ticker}
        empty="No companies."
        onSortChange={(col, dir) => { setPage(0); setSortBy(col); setSortDir(dir) }}
      />
      <TablePagination
        component="div"
        count={total}
        page={page}
        rowsPerPage={PAGE_SIZE}
        rowsPerPageOptions={[PAGE_SIZE]}
        onPageChange={(_e, p) => setPage(p)}
      />
    </Box>
  )
}
