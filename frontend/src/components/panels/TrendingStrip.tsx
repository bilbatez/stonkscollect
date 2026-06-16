import { useEffect, useState } from 'react'
import { Box, Chip, Stack, Typography } from '@mui/material'
import { getMovers } from '../../api'
import { formatPct } from '../../format'
import type { MoverRow, Movers } from '../../types'

const TRENDING_LIMIT = 3

/** Compact Home strip of the day's top gainers and losers as clickable chips,
 *  reusing the movers endpoint. Silent until data loads. */
export function TrendingStrip({ onSelect }: { onSelect: (ticker: string) => void }) {
  const [movers, setMovers] = useState<Movers | null>(null)
  useEffect(() => {
    getMovers(TRENDING_LIMIT)
      .then(setMovers)
      .catch(() => {})
  }, [])
  if (!movers || (movers.gainers.length === 0 && movers.losers.length === 0)) return null
  const group = (label: string, rows: MoverRow[], color: 'success' | 'error') =>
    rows.length > 0 && (
      <Stack direction="row" spacing={1} sx={{ alignItems: 'center', flexWrap: 'wrap' }}>
        <Typography variant="caption" color="text.secondary">
          {label}
        </Typography>
        {rows.map((r) => (
          <Chip
            key={r.company.ticker}
            size="small"
            variant="outlined"
            color={color}
            onClick={() => onSelect(r.company.ticker)}
            label={`${r.company.ticker} ${r.change_pct >= 0 ? '+' : ''}${formatPct(r.change_pct)}`}
          />
        ))}
      </Stack>
    )
  return (
    <Box sx={{ display: 'flex', gap: 3, flexWrap: 'wrap', mb: 2 }}>
      {group('Gainers', movers.gainers, 'success')}
      {group('Losers', movers.losers, 'error')}
    </Box>
  )
}
