import { useState } from 'react'
import {
  Alert,
  Box,
  Button,
  Chip,
  FormControlLabel,
  LinearProgress,
  Stack,
  Switch,
  TablePagination,
  TextField,
} from '@mui/material'
import { listCompanies } from '../../api'
import { PAGE_SIZE } from '../../constants'
import { usePaginatedFetch } from '../../hooks/usePaginatedFetch'
import type { CompanyRow } from '../../types'
import { DataGrid } from '../shared/DataGrid'
import type { GridColumn } from '../shared/dataGridUtils'

interface Props {
  onSelect: (ticker: string) => void
  onAdd: (ticker: string) => void
}

/** Paginated directory of every company with its Graham score. The page is a
 *  sortable / filterable / column-reorderable grid. Sorting triggers a server
 *  re-fetch so the full dataset is sorted, not just the visible page. */
export function AllStocks({ onSelect, onAdd }: Props) {
  const [page, setPage] = useState(0)
  const [q, setQ] = useState('')
  const [filters, setFilters] = useState<Record<string, string>>({})
  const [sortBy, setSortBy] = useState<string | null>(null)
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('asc')
  const [showDelisted, setShowDelisted] = useState(false)

  const { rows, total, loading, error } = usePaginatedFetch<CompanyRow>(
    () => listCompanies(q, filters, sortBy, sortDir, PAGE_SIZE, page * PAGE_SIZE, showDelisted),
    [q, filters, page, sortBy, sortDir, showDelisted],
  )

  const columns: GridColumn<CompanyRow>[] = [
    {
      id: 'ticker',
      header: 'Ticker',
      sortValue: (r) => r.company.ticker,
      filter: true,
      cell: (r) => (
        <>
          <Button size="small" onClick={() => onSelect(r.company.ticker)}>
            {r.company.ticker}
          </Button>
          {r.company.status === 'delisted' && (
            <Chip size="small" color="warning" variant="outlined" label="Delisted" sx={{ ml: 0.5 }} />
          )}
        </>
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
      <Stack direction="row" spacing={2} sx={{ mb: 2, alignItems: 'center' }}>
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
        />
        <FormControlLabel
          control={
            <Switch
              checked={showDelisted}
              onChange={(e) => {
                setPage(0)
                setShowDelisted(e.target.checked)
              }}
              slotProps={{ input: { 'aria-label': 'show delisted' } }}
            />
          }
          label="Show delisted"
          sx={{ flexShrink: 0, whiteSpace: 'nowrap' }}
        />
      </Stack>
      {error && <Alert severity="error" sx={{ mb: 2 }}>{error}</Alert>}
      {loading && <LinearProgress sx={{ mb: 1 }} />}
      <DataGrid
        columns={columns}
        rows={rows}
        getRowId={(r) => r.company.ticker}
        empty="No companies."
        onSortChange={(col, dir) => { setPage(0); setSortBy(col); setSortDir(dir) }}
        onFilterChange={(f) => { setPage(0); setFilters(f) }}
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
