import { useEffect, useState } from 'react'
import { Box, Card, CardContent, Stack, Typography } from '@mui/material'
import { getMarketSummary } from '../../api'
import { formatNum, formatPct } from '../../format'
import type { MoverRow } from '../../types'

/** Yahoo-style market summary strip: one card per tracked index (S&P/Nasdaq/Dow)
 *  with its latest close and colored day change. Silent until data loads. */
export function MarketSummary() {
  const [indices, setIndices] = useState<MoverRow[]>([])
  useEffect(() => {
    getMarketSummary()
      .then(setIndices)
      .catch(() => {})
  }, [])
  if (indices.length === 0) return null
  return (
    <Box sx={{ display: 'flex', gap: 1, flexWrap: 'wrap', mb: 2 }}>
      {indices.map((i) => {
        const up = i.change_pct >= 0
        return (
          <Card key={i.company.ticker} variant="outlined" sx={{ minWidth: 160, flex: '1 1 160px' }}>
            <CardContent sx={{ p: 1.5, '&:last-child': { pb: 1.5 } }}>
              <Typography variant="caption" color="text.secondary" sx={{ display: 'block' }}>
                {i.company.name}
              </Typography>
              <Stack direction="row" spacing={1} sx={{ alignItems: 'baseline' }}>
                <Typography variant="h6">{formatNum(i.last_close)}</Typography>
                <Typography variant="body2" color={up ? 'success.main' : 'error.main'}>
                  {up ? '+' : ''}
                  {formatPct(i.change_pct)}
                </Typography>
              </Stack>
            </CardContent>
          </Card>
        )
      })}
    </Box>
  )
}
