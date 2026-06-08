import { Paper, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from '@mui/material'
import { formatCurrency } from '../format'
import type { FinancialFact } from '../types'

/** Pivot facts into a line-item × period table (periods newest-first). */
export function StatementTable({ facts }: { facts: FinancialFact[] }) {
  if (facts.length === 0) {
    return <Typography color="text.secondary">No statement data.</Typography>
  }

  const periods = [...new Set(facts.map((f) => f.period_end))].sort().reverse()
  const byItem = new Map<string, Map<string, number>>()
  for (const f of facts) {
    if (!byItem.has(f.line_item)) {
      byItem.set(f.line_item, new Map())
    }
    byItem.get(f.line_item)!.set(f.period_end, f.value)
  }

  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Line item</TableCell>
            {periods.map((p) => (
              <TableCell key={p} align="right">
                {p}
              </TableCell>
            ))}
          </TableRow>
        </TableHead>
        <TableBody>
          {[...byItem.entries()].map(([item, values]) => (
            <TableRow key={item} hover>
              <TableCell>{item}</TableCell>
              {periods.map((p) => {
                const v = values.get(p)
                return (
                  <TableCell key={p} align="right">
                    {v === undefined ? '—' : formatCurrency(v)}
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
