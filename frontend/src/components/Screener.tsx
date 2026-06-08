import { useEffect, useState } from 'react'
import {
  Box,
  Button,
  Checkbox,
  Chip,
  FormControlLabel,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TablePagination,
  TableRow,
  Typography,
} from '@mui/material'
import { screen } from '../api'
import type { ScreenRow } from '../types'

const PAGE_SIZE = 25

const pct = (x: number | null) => (x === null ? '—' : `${(x * 100).toFixed(0)}%`)
const num = (x: number | null) => (x === null ? '—' : x.toFixed(2))

/** Graham screener: all companies ranked by score, with optional filters. */
export function Screener({ onSelect }: { onSelect: (ticker: string) => void }) {
  const [rows, setRows] = useState<ScreenRow[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(0)
  const [defensive, setDefensive] = useState(false)
  const [netNet, setNetNet] = useState(false)

  useEffect(() => {
    void screen({ defensive, net_net: netNet, limit: PAGE_SIZE, offset: page * PAGE_SIZE }).then(
      (p) => {
        setRows(p.rows)
        setTotal(p.total)
      },
    )
  }, [defensive, netNet, page])

  return (
    <Box>
      <Typography variant="h5" component="h2" gutterBottom>
        Screener
      </Typography>
      <Typography variant="body2" color="text.secondary" gutterBottom>
        Companies ranked by Graham defensive score (0–8): how many of Benjamin Graham's
        criteria each one meets. Filter to the strictest picks.
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
      {rows.length === 0 ? (
        <Typography color="text.secondary">No matches.</Typography>
      ) : (
        <>
          <TableContainer component={Paper} variant="outlined">
            <Table size="small">
              <TableHead>
                <TableRow>
                  <TableCell>Ticker</TableCell>
                  <TableCell align="right">Score</TableCell>
                  <TableCell align="right">Graham #</TableCell>
                  <TableCell align="right">Margin of safety</TableCell>
                  <TableCell align="right">Net-net</TableCell>
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
                    <TableCell align="right">
                      <Chip size="small" label={`${r.score.score}/8`} />
                    </TableCell>
                    <TableCell align="right">{num(r.score.graham_number)}</TableCell>
                    <TableCell align="right">{pct(r.score.margin_of_safety)}</TableCell>
                    <TableCell align="right">{r.score.net_net ? '✓' : '—'}</TableCell>
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
        </>
      )}
    </Box>
  )
}
