import { useEffect, useState } from 'react'
import {
  Box,
  Button,
  Checkbox,
  Chip,
  FormControlLabel,
  Stack,
  TablePagination,
  Typography,
} from '@mui/material'
import { screen } from '../api'
import type { ScreenRow } from '../types'
import { DataGrid } from './DataGrid'
import type { GridColumn } from './dataGridUtils'

const PAGE_SIZE = 25

const pct = (x: number | null) => (x === null ? '—' : `${(x * 100).toFixed(0)}%`)
const num = (x: number | null) => (x === null ? '—' : x.toFixed(2))

/** Graham screener: all companies ranked by score, with filters. The result
 *  page is a sortable / filterable / column-reorderable grid. */
export function Screener({ onSelect }: { onSelect: (ticker: string) => void }) {
  const [rows, setRows] = useState<ScreenRow[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(0)
  const [defensive, setDefensive] = useState(false)
  const [netNet, setNetNet] = useState(false)
  const [sortBy, setSortBy] = useState<string | null>(null)
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('asc')

  useEffect(() => {
    void screen({
      defensive,
      net_net: netNet,
      sort_by: sortBy ?? undefined,
      sort_dir: sortBy ? sortDir : undefined,
      limit: PAGE_SIZE,
      offset: page * PAGE_SIZE,
    }).then((p) => {
      setRows(p.rows)
      setTotal(p.total)
    })
  }, [defensive, netNet, page, sortBy, sortDir])

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
    { id: 'graham', header: 'Graham #', sortValue: (r) => r.score.graham_number ?? -1, cell: (r) => num(r.score.graham_number) },
    { id: 'margin', header: 'Margin of safety', sortValue: (r) => r.score.margin_of_safety ?? -1e9, cell: (r) => pct(r.score.margin_of_safety) },
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
      <Stack direction="row" spacing={2} sx={{ my: 1 }}>
        <FormControlLabel
          control={
            <Checkbox
              checked={defensive}
              onChange={(e) => {
                setPage(0)
                setDefensive(e.target.checked)
              }}
            />
          }
          label="Defensive only"
        />
        <FormControlLabel
          control={
            <Checkbox
              checked={netNet}
              onChange={(e) => {
                setPage(0)
                setNetNet(e.target.checked)
              }}
            />
          }
          label="Net-net"
        />
      </Stack>
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
