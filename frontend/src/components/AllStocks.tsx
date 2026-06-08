import { useEffect, useState } from 'react'
import {
  Box,
  Button,
  Chip,
  Paper,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TablePagination,
  TableRow,
  TextField,
} from '@mui/material'
import { listCompanies } from '../api'
import type { CompanyRow } from '../types'

const PAGE_SIZE = 25

interface Props {
  onSelect: (ticker: string) => void
  onAdd: (ticker: string) => void
}

/** Paginated, searchable directory of every company with its Graham score. */
export function AllStocks({ onSelect, onAdd }: Props) {
  const [rows, setRows] = useState<CompanyRow[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(0)
  const [q, setQ] = useState('')

  useEffect(() => {
    void listCompanies(q, PAGE_SIZE, page * PAGE_SIZE).then((p) => {
      setRows(p.rows)
      setTotal(p.total)
    })
  }, [q, page])

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
      <TableContainer component={Paper} variant="outlined">
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>Ticker</TableCell>
              <TableCell>Name</TableCell>
              <TableCell align="right">Graham score</TableCell>
              <TableCell align="right">Watchlist</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {rows.map((r) => (
              <TableRow key={r.company.ticker} hover>
                <TableCell>
                  <Button size="small" onClick={() => onSelect(r.company.ticker)}>
                    {r.company.ticker}
                  </Button>
                </TableCell>
                <TableCell>{r.company.name}</TableCell>
                <TableCell align="right">
                  {r.score === null ? (
                    '—'
                  ) : (
                    <Chip size="small" label={`${r.score.score}/8`} />
                  )}
                </TableCell>
                <TableCell align="right">
                  <Button
                    size="small"
                    aria-label={`watch ${r.company.ticker}`}
                    onClick={() => onAdd(r.company.ticker)}
                  >
                    Watch
                  </Button>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </TableContainer>
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
