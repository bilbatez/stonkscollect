import { useEffect, useState } from 'react'
import {
  Alert,
  Box,
  Button,
  Paper,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography,
} from '@mui/material'
import { getMovers } from '../../api'
import { formatCompact, formatNum, formatPct } from '../../format'
import { Skeleton } from '../shared/Skeleton'
import type { MoverRow, Movers } from '../../types'

function signed(formatted: string, positive: boolean): string {
  return positive ? `+${formatted}` : formatted
}

function MoverTable({
  title,
  rows,
  onSelect,
}: {
  title: string
  rows: MoverRow[]
  onSelect: (ticker: string) => void
}) {
  return (
    <Box sx={{ flex: '1 1 320px', minWidth: 320 }}>
      <Typography variant="subtitle1" component="h3" gutterBottom sx={{ fontWeight: 600 }}>
        {title}
      </Typography>
      {rows.length === 0 ? (
        <Typography color="text.secondary">No movers yet.</Typography>
      ) : (
        <TableContainer component={Paper} variant="outlined">
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>Ticker</TableCell>
                <TableCell align="right">Last</TableCell>
                <TableCell align="right">Change</TableCell>
                <TableCell align="right">%</TableCell>
                <TableCell align="right">Volume</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {rows.map((row) => {
                const up = row.change >= 0
                const moveColor = { color: up ? 'success.main' : 'error.main' }
                return (
                  <TableRow key={row.company.ticker} hover>
                    <TableCell>
                      <Button size="small" onClick={() => onSelect(row.company.ticker)}>
                        {row.company.ticker}
                      </Button>
                    </TableCell>
                    <TableCell align="right">{formatNum(row.last_close)}</TableCell>
                    <TableCell align="right" sx={moveColor}>
                      {signed(formatNum(row.change), up)}
                    </TableCell>
                    <TableCell align="right" sx={moveColor}>
                      {signed(formatPct(row.change_pct), up)}
                    </TableCell>
                    <TableCell align="right">{formatCompact(row.volume)}</TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        </TableContainer>
      )}
    </Box>
  )
}

/** Yahoo-style market movers: top gainers / losers / most active. */
export function MoversView({ onSelect }: { onSelect: (ticker: string) => void }) {
  const [movers, setMovers] = useState<Movers | null>(null)
  const [failed, setFailed] = useState(false)
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    let active = true
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setMovers(null)
    setFailed(false)
    getMovers()
      .then((m) => {
        if (active) setMovers(m)
      })
      .catch(() => {
        if (active) setFailed(true)
      })
    return () => {
      active = false
    }
  }, [attempt])

  if (failed) {
    return (
      <Alert
        severity="error"
        action={
          <Button color="inherit" size="small" onClick={() => setAttempt((n) => n + 1)}>
            Retry
          </Button>
        }
      >
        Failed to load movers.
      </Alert>
    )
  }
  if (!movers) {
    return <Skeleton label="Loading movers…" />
  }
  return (
    <Box sx={{ display: 'flex', gap: 2, flexWrap: 'wrap' }}>
      <MoverTable title="Top gainers" rows={movers.gainers} onSelect={onSelect} />
      <MoverTable title="Top losers" rows={movers.losers} onSelect={onSelect} />
      <MoverTable title="Most active" rows={movers.most_active} onSelect={onSelect} />
    </Box>
  )
}
