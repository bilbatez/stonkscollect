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
import { downloadCsv, formatMetric, formatPeriodDate, metricGroup, metricGroups, metricLabel } from '../format'
import type { Period, Ratio } from '../types'
import { PeriodToggle } from './PeriodToggle'

/** Derived ratios, grouped by category (rows) across periods (date columns),
 *  with an Annual/Quarterly toggle and human labels + formatting. */
export function RatiosPanel({ ratios }: { ratios: Ratio[] }) {
  const [period, setPeriod] = useState<Period>('annual')
  if (ratios.length === 0) {
    return <Typography color="text.secondary">No ratio data.</Typography>
  }

  const rows = ratios.filter((r) => r.period_type === period)
  const periods = [...new Set(rows.map((r) => r.period_end))].sort().reverse()
  // metric -> (period_end -> value)
  const byMetric = new Map<string, Map<string, number>>()
  for (const r of rows) {
    if (!byMetric.has(r.metric)) {
      byMetric.set(r.metric, new Map())
    }
    byMetric.get(r.metric)!.set(r.period_end, r.value)
  }
  const groupsInOrder = [...metricGroups, 'Other']
  const metricsOf = (g: string) =>
    [...byMetric.keys()].filter((m) => metricGroup(m) === g).sort()

  const body: ReactNode[] = []
  for (const g of groupsInOrder) {
    const ms = metricsOf(g)
    if (ms.length === 0) {
      continue
    }
    body.push(
      <TableRow key={`group-${g}`}>
        <TableCell colSpan={periods.length + 1} sx={{ fontWeight: 600, bgcolor: 'action.hover' }}>
          {g}
        </TableCell>
      </TableRow>,
    )
    for (const m of ms) {
      body.push(
        <TableRow key={m} hover>
          <TableCell>{metricLabel(m)}</TableCell>
          {periods.map((p) => {
            const v = byMetric.get(m)!.get(p)
            return (
              <TableCell key={p} align="right">
                {v === undefined ? '—' : formatMetric(m, v)}
              </TableCell>
            )
          })}
        </TableRow>,
      )
    }
  }

  function handleExport() {
    const headers = ['Metric', ...periods.map(formatPeriodDate)]
    const rows: (string | number | null)[][] = []
    for (const g of groupsInOrder) {
      for (const m of metricsOf(g)) {
        rows.push([metricLabel(m), ...periods.map((p) => byMetric.get(m)!.get(p) ?? null)])
      }
    }
    downloadCsv(`${period}-ratios.csv`, headers, rows)
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
          No {period} ratio data.
        </Typography>
      ) : (
        <TableContainer component={Paper} variant="outlined" sx={{ mt: 1 }}>
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>Metric</TableCell>
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
