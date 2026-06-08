import { Paper, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from '@mui/material'

export interface CompareRow {
  ticker: string
  metrics: Record<string, number>
}

/** Compare a set of ratio metrics across multiple tickers. */
export function Compare({ rows }: { rows: CompareRow[] }) {
  if (rows.length === 0) {
    return <Typography color="text.secondary">Nothing to compare.</Typography>
  }
  // Union of metric names across rows, sorted for stable columns.
  const metrics = [...new Set(rows.flatMap((r) => Object.keys(r.metrics)))].sort()
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Ticker</TableCell>
            {metrics.map((m) => (
              <TableCell key={m} align="right">
                {m}
              </TableCell>
            ))}
          </TableRow>
        </TableHead>
        <TableBody>
          {rows.map((r) => (
            <TableRow key={r.ticker} hover>
              <TableCell>{r.ticker}</TableCell>
              {metrics.map((m) => {
                const v = r.metrics[m]
                return (
                  <TableCell key={m} align="right">
                    {v === undefined ? '—' : v.toFixed(2)}
                  </TableCell>
                )
              })}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  )
}
