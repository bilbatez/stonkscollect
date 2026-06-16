import {
  Paper,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography,
} from '@mui/material'
import { formatNum, formatPeriodDate } from '../../format'
import type { FinancialFact } from '../../types'

/** Annual dividend-per-share history (newest first) from EDGAR facts. */
export function DividendPanel({ facts }: { facts: FinancialFact[] }) {
  const divs = facts
    .filter((f) => f.line_item === 'DividendPerShare' && f.period_type === 'annual')
    .sort((a, b) => b.period_end.localeCompare(a.period_end))
  if (divs.length === 0) {
    return <Typography color="text.secondary">No dividend history.</Typography>
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Period</TableCell>
            <TableCell align="right">Dividend / share</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {divs.map((d) => (
            <TableRow key={d.period_end} hover>
              <TableCell>{formatPeriodDate(d.period_end)}</TableCell>
              <TableCell align="right">{formatNum(d.value)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  )
}
