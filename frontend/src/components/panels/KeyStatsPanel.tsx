import { Box, Card, CardContent, Typography } from '@mui/material'
import { formatCompact, formatCurrency, formatNum, formatPct } from '../../format'
import type { KeyStats, Quote } from '../../quote'

interface StatRow {
  label: string
  value: string
}

const currency = (v: number | null) => (v === null ? '—' : formatCurrency(v))

function range(low: number | null, high: number | null): string {
  return low === null || high === null ? '—' : `${formatNum(low)} – ${formatNum(high)}`
}

/** Yahoo-style "key statistics" grid built from the derived quote + stats. */
export function KeyStatsPanel({ stats, quote }: { stats: KeyStats; quote: Quote | null }) {
  const rows: StatRow[] = [
    { label: 'Market cap', value: currency(stats.marketCap) },
    { label: 'Shares outstanding', value: formatCompact(stats.sharesOutstanding) },
    { label: 'Public float', value: currency(stats.publicFloat) },
    { label: 'Day range', value: quote ? range(quote.dayLow, quote.dayHigh) : '—' },
    { label: '52-week range', value: quote ? range(quote.week52Low, quote.week52High) : '—' },
    { label: 'Volume', value: formatCompact(quote ? quote.volume : null) },
    { label: 'Avg volume (3m)', value: formatCompact(quote ? quote.avgVolume3m : null) },
    { label: 'EPS', value: formatNum(stats.eps) },
    { label: 'P/E', value: formatNum(stats.pe) },
    { label: 'P/B', value: formatNum(stats.pb) },
    { label: 'Dividend rate', value: formatNum(stats.dividendRate) },
    { label: 'Dividend yield', value: formatPct(stats.dividendYield) },
    { label: 'Payout ratio', value: formatPct(stats.payoutRatio) },
    { label: 'Free cash flow', value: currency(stats.freeCashFlow) },
    { label: 'Book value / share', value: formatNum(stats.bookValuePerShare) },
    { label: 'Employees', value: formatCompact(stats.employees) },
  ]
  return (
    <Box sx={{ display: 'flex', flexWrap: 'wrap', gap: 1 }}>
      {rows.map(({ label, value }) => (
        <Card key={label} variant="outlined" sx={{ minWidth: 150, flex: '1 1 150px' }}>
          <CardContent sx={{ p: 1, '&:last-child': { pb: 1 } }}>
            <Typography variant="caption" color="text.secondary" sx={{ display: 'block' }}>
              {label}
            </Typography>
            <Typography variant="body2" sx={{ fontWeight: 600 }}>
              {value}
            </Typography>
          </CardContent>
        </Card>
      ))}
    </Box>
  )
}
