import {
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
import { formatNum, formatPct, scoreHeatColor } from '../../format'
import type { SectorStats } from '../../types'

export function SectorOverview({
  sectors,
  onSelect,
}: {
  sectors: SectorStats[]
  onSelect: (ticker: string) => void
}) {
  if (sectors.length === 0) {
    return <Typography color="text.secondary">No sector data available.</Typography>
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Sector</TableCell>
            <TableCell align="right">Companies</TableCell>
            <TableCell align="right">Avg score</TableCell>
            <TableCell align="right">% Defensive</TableCell>
            <TableCell align="right">Top pick</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {sectors.map((s) => (
            <TableRow key={s.sector} hover>
              <TableCell>{s.sector}</TableCell>
              <TableCell align="right">{s.company_count}</TableCell>
              <TableCell align="right" sx={{ bgcolor: scoreHeatColor(s.avg_score) }}>
                {formatNum(s.avg_score)}
              </TableCell>
              <TableCell align="right">{formatPct(s.pct_defensive)}</TableCell>
              <TableCell align="right">
                {s.top_ticker ? (
                  <Button size="small" onClick={() => onSelect(s.top_ticker!)}>
                    {s.top_ticker}
                  </Button>
                ) : (
                  '—'
                )}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  )
}
