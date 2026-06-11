import { useState } from 'react'
import {
  Alert,
  Box,
  Button,
  Checkbox,
  Chip,
  FormControlLabel,
  LinearProgress,
  Stack,
  TablePagination,
  TextField,
  Typography,
} from '@mui/material'
import { screen } from '../../api'
import {
  FILTER_FIELD_WIDTH,
  FILTER_FIELD_WIDTH_WIDE,
  PAGE_SIZE,
  SECTOR_FIELD_WIDTH,
} from '../../constants'
import { formatNum, formatPct } from '../../format'
import { usePaginatedFetch } from '../../hooks/usePaginatedFetch'
import type { ScreenRow } from '../../types'
import { DataGrid } from '../shared/DataGrid'
import type { GridColumn } from '../shared/dataGridUtils'

/** The screener's filter controls. Text inputs hold raw strings (`''` = unset);
 *  they're coerced to numbers only when building the query. */
interface Filters {
  defensive: boolean
  netNet: boolean
  sector: string
  minPe: string
  maxPe: string
  minRoe: string
  maxDe: string
  minMargin: string
}

const EMPTY_FILTERS: Filters = {
  defensive: false,
  netNet: false,
  sector: '',
  minPe: '',
  maxPe: '',
  minRoe: '',
  maxDe: '',
  minMargin: '',
}

/** `''` → `undefined`, else the parsed number — for optional numeric filters. */
const num = (s: string): number | undefined => (s !== '' ? Number(s) : undefined)

/** Graham screener: all companies ranked by score, with filters. The result
 *  page is a sortable / filterable / column-reorderable grid. */
export function Screener({ onSelect }: { onSelect: (ticker: string) => void }) {
  const [page, setPage] = useState(0)
  const [filters, setFilters] = useState<Filters>(EMPTY_FILTERS)
  const [sortBy, setSortBy] = useState<string | null>(null)
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('asc')

  // Any filter change returns to the first page.
  const update = (patch: Partial<Filters>) => {
    setPage(0)
    setFilters((f) => ({ ...f, ...patch }))
  }

  const { rows, total, loading, error } = usePaginatedFetch<ScreenRow>(
    () =>
      screen({
        defensive: filters.defensive,
        net_net: filters.netNet,
        sector: filters.sector || undefined,
        min_pe: num(filters.minPe),
        max_pe: num(filters.maxPe),
        min_roe: num(filters.minRoe),
        max_de: num(filters.maxDe),
        min_margin: num(filters.minMargin),
        sort_by: sortBy ?? undefined,
        sort_dir: sortBy ? sortDir : undefined,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      }),
    [filters, page, sortBy, sortDir],
  )

  const columns: GridColumn<ScreenRow>[] = [
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
    {
      id: 'score',
      header: 'Score',
      sortValue: (r) => r.score.score,
      cell: (r) => <Chip size="small" label={`${r.score.score}/8`} />,
    },
    { id: 'graham', header: 'Graham #', sortValue: (r) => r.score.graham_number ?? -1, cell: (r) => formatNum(r.score.graham_number) },
    { id: 'margin', header: 'Margin of safety', sortValue: (r) => r.score.margin_of_safety ?? -1e9, cell: (r) => formatPct(r.score.margin_of_safety) },
    { id: 'netnet', header: 'Net-net', sortValue: (r) => (r.score.net_net ? 1 : 0), cell: (r) => (r.score.net_net ? '✓' : '—') },
  ]

  return (
    <Box>
      <Typography variant="h5" component="h2" gutterBottom>
        Screener
      </Typography>
      <Typography variant="body2" color="text.secondary" gutterBottom>
        Companies ranked by Graham defensive score (0–8): how many of Benjamin Graham's
        criteria each one meets. Sort, filter, and reorder columns; filter to the strictest picks.
      </Typography>
      <Stack direction="row" spacing={2} sx={{ my: 1, alignItems: 'center', flexWrap: 'wrap' }}>
        <FormControlLabel
          control={
            <Checkbox
              checked={filters.defensive}
              onChange={(e) => update({ defensive: e.target.checked })}
            />
          }
          label="Defensive only"
        />
        <FormControlLabel
          control={
            <Checkbox
              checked={filters.netNet}
              onChange={(e) => update({ netNet: e.target.checked })}
            />
          }
          label="Net-net"
        />
        <TextField
          size="small"
          label="Sector"
          value={filters.sector}
          onChange={(e) => update({ sector: e.target.value })}
          sx={{ width: SECTOR_FIELD_WIDTH }}
        />
      </Stack>
      <Stack direction="row" spacing={1} sx={{ my: 1, flexWrap: 'wrap' }}>
        <TextField size="small" label="Min P/E" type="number" value={filters.minPe} onChange={(e) => update({ minPe: e.target.value })} sx={{ width: FILTER_FIELD_WIDTH }} />
        <TextField size="small" label="Max P/E" type="number" value={filters.maxPe} onChange={(e) => update({ maxPe: e.target.value })} sx={{ width: FILTER_FIELD_WIDTH }} />
        <TextField size="small" label="Min ROE" type="number" value={filters.minRoe} onChange={(e) => update({ minRoe: e.target.value })} slotProps={{ htmlInput: { step: '0.01' } }} sx={{ width: FILTER_FIELD_WIDTH }} />
        <TextField size="small" label="Max D/E" type="number" value={filters.maxDe} onChange={(e) => update({ maxDe: e.target.value })} slotProps={{ htmlInput: { step: '0.01' } }} sx={{ width: FILTER_FIELD_WIDTH }} />
        <TextField size="small" label="Min margin" type="number" value={filters.minMargin} onChange={(e) => update({ minMargin: e.target.value })} slotProps={{ htmlInput: { step: '0.01' } }} sx={{ width: FILTER_FIELD_WIDTH_WIDE }} />
      </Stack>
      {error && <Alert severity="error" sx={{ mb: 2 }}>{error}</Alert>}
      {loading && <LinearProgress sx={{ mb: 1 }} />}
      <DataGrid
        columns={columns}
        rows={rows}
        getRowId={(r) => r.company.ticker}
        empty="No matches."
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
