import { Paper, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from '@mui/material'
import { formatCurrency } from '../format'
import type { Discrepancy } from '../types'

/** Cross-source mismatches flagged by the reconcile layer. */
export function DiscrepancyPanel({ discrepancies }: { discrepancies: Discrepancy[] }) {
  if (discrepancies.length === 0) {
    return <Typography color="text.secondary">No discrepancies flagged.</Typography>
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Field</TableCell>
            <TableCell>Period</TableCell>
            <TableCell>Sources</TableCell>
            <TableCell align="right">Diff</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {discrepancies.map((d, i) => (
            <TableRow key={`${d.field}-${d.period ?? 'na'}-${i}`} hover>
              <TableCell>{d.field}</TableCell>
              <TableCell>{d.period ?? '—'}</TableCell>
              <TableCell>
                {d.source_a} {formatCurrency(d.value_a)} vs {d.source_b} {formatCurrency(d.value_b)}
              </TableCell>
              <TableCell align="right">{(d.pct_diff * 100).toFixed(1)}%</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  )
}
