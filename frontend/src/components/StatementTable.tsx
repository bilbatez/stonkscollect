import { useState, type ReactNode } from 'react'
import {
  Box,
  Button,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography,
} from '@mui/material'
import { calcGrowth, downloadCsv, formatCurrency, formatPeriodDate, statementItemLabel, statementLabel } from '../format'
import type { FinancialFact, Period } from '../types'
import { PeriodToggle } from './PeriodToggle'

const SECTION_ORDER = ['income', 'balance', 'cashflow']

/** Statement facts grouped by section (Income / Balance / Cash flow), line items
 *  (rows) × periods (date columns), with an Annual/Quarterly toggle. */
export function StatementTable({ facts }: { facts: FinancialFact[] }) {
  const [period, setPeriod] = useState<Period>('annual')
  if (facts.length === 0) {
    return <Typography color="text.secondary">No statement data.</Typography>
  }

  const rows = facts.filter((f) => f.period_type === period)
  const periods = [...new Set(rows.map((f) => f.period_end))].sort().reverse()
  // statement -> line_item -> (period_end -> value)
  const bySection = new Map<string, Map<string, Map<string, number>>>()
  for (const f of rows) {
    if (!bySection.has(f.statement)) {
      bySection.set(f.statement, new Map())
    }
    const items = bySection.get(f.statement)!
    if (!items.has(f.line_item)) {
      items.set(f.line_item, new Map())
    }
    items.get(f.line_item)!.set(f.period_end, f.value)
  }
  const sections = [
    ...SECTION_ORDER.filter((s) => bySection.has(s)),
    ...[...bySection.keys()].filter((s) => !SECTION_ORDER.includes(s)),
  ]

  const body: ReactNode[] = []
  for (const section of sections) {
    body.push(
      <TableRow key={`s-${section}`}>
        <TableCell colSpan={periods.length + 1} sx={{ fontWeight: 600, bgcolor: 'action.hover' }}>
          {statementLabel(section)}
        </TableCell>
      </TableRow>,
    )
    for (const [item, values] of bySection.get(section)!) {
      body.push(
        <TableRow key={`${section}-${item}`} hover>
          <TableCell>{statementItemLabel(item)}</TableCell>
          {periods.map((p, i) => {
            const v = values.get(p)
            const prior = i < periods.length - 1 ? values.get(periods[i + 1]) : undefined
            const growth = v !== undefined && prior !== undefined ? calcGrowth(v, prior) : null
            return (
              <TableCell key={p} align="right">
                {v === undefined ? '—' : formatCurrency(v)}
                {growth && (
                  <Typography variant="caption" color={growth.positive ? 'success.main' : 'error.main'} sx={{ display: 'block' }}>
                    {growth.pct}
                  </Typography>
                )}
              </TableCell>
            )
          })}
        </TableRow>,
      )
    }
  }

  function handleExport() {
    const headers = ['Line item', ...periods.map(formatPeriodDate)]
    const rows: (string | number | null)[][] = []
    for (const section of sections) {
      for (const [item, values] of bySection.get(section)!) {
        rows.push([statementItemLabel(item), ...periods.map((p) => values.get(p) ?? null)])
      }
    }
    downloadCsv(`${period}-statements.csv`, headers, rows)
  }

  return (
    <Box>
      <Stack direction="row" sx={{ alignItems: 'center', justifyContent: 'space-between' }}>
        <PeriodToggle period={period} onChange={setPeriod} />
        <Button size="small" variant="outlined" onClick={handleExport}>
          Export CSV
        </Button>
      </Stack>
      {periods.length === 0 ? (
        <Typography color="text.secondary" sx={{ mt: 1 }}>
          No {period} statement data.
        </Typography>
      ) : (
        <TableContainer component={Paper} variant="outlined" sx={{ mt: 1 }}>
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>Line item</TableCell>
                {periods.map((p) => (
                  <TableCell key={p} align="right">
                    {formatPeriodDate(p)}
                  </TableCell>
                ))}
              </TableRow>
            </TableHead>
            <TableBody>{body}</TableBody>
          </Table>
        </TableContainer>
      )}
    </Box>
  )
}
