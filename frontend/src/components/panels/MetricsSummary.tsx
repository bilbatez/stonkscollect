import { Box, Card, CardContent, Typography } from '@mui/material'
import { formatMetric, formatNum, formatPct } from '../../format'
import type { GrahamAssessment, Ratio } from '../../types'

interface MetricCard {
  label: string
  value: string
}

function latestAnnual(ratios: Ratio[], metric: string): number | null {
  const matches = ratios
    .filter((r) => r.metric === metric && r.period_type === 'annual')
    .sort((a, b) => b.period_end.localeCompare(a.period_end))
  return matches.length > 0 ? matches[0].value : null
}

export function MetricsSummary({
  ratios,
  graham,
}: {
  ratios: Ratio[]
  graham: GrahamAssessment
}) {
  const r = (metric: string) => latestAnnual(ratios, metric)

  const cards: MetricCard[] = [
    { label: 'P/E', value: r('pe') !== null ? formatMetric('pe', r('pe')!) : '—' },
    { label: 'P/B', value: r('pb') !== null ? formatMetric('pb', r('pb')!) : '—' },
    { label: 'Return on equity', value: r('roe') !== null ? formatMetric('roe', r('roe')!) : '—' },
    { label: 'Net margin', value: r('net_margin') !== null ? formatMetric('net_margin', r('net_margin')!) : '—' },
    { label: 'Debt to equity', value: r('debt_to_equity') !== null ? formatMetric('debt_to_equity', r('debt_to_equity')!) : '—' },
    { label: 'Current ratio', value: r('current_ratio') !== null ? formatMetric('current_ratio', r('current_ratio')!) : '—' },
    { label: 'Graham # (target price)', value: graham.graham_number !== null ? `$${formatNum(graham.graham_number)}` : '—' },
    { label: 'Margin of safety', value: graham.margin_of_safety !== null ? formatPct(graham.margin_of_safety) : '—' },
  ]

  return (
    <Box sx={{ display: 'flex', flexWrap: 'wrap', gap: 1, mb: 2 }}>
      {cards.map(({ label, value }) => (
        <Card key={label} variant="outlined" sx={{ minWidth: 110, flex: '1 1 110px' }}>
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
