import { Paper, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from '@mui/material'
import type { Ratio } from '../types'

/** Table of derived ratios. */
export function RatiosPanel({ ratios }: { ratios: Ratio[] }) {
  if (ratios.length === 0) {
    return <Typography color="text.secondary">No ratio data.</Typography>
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Metric</TableCell>
            <TableCell>Period</TableCell>
            <TableCell align="right">Value</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {ratios.map((r) => (
            <TableRow key={`${r.metric}-${r.period_end}`} hover>
              <TableCell>{r.metric}</TableCell>
              <TableCell>{r.period_end}</TableCell>
              <TableCell align="right">{r.value}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  )
}
