import { Box, Stack, Typography } from '@mui/material'
import { formatNum } from '../../format'

/** Yahoo-style 52-week range gauge: a track with the latest price marked
 *  between the period low and high. */
export function WeekRangeBar({ low, high, last }: { low: number; high: number; last: number }) {
  const span = high - low
  const fraction = span > 0 ? Math.min(1, Math.max(0, (last - low) / span)) : 0
  return (
    <Stack spacing={0.5} sx={{ minWidth: 220 }}>
      <Box
        aria-label="52-week range"
        sx={{ position: 'relative', height: 6, borderRadius: 3, bgcolor: 'action.selected' }}
      >
        <Box
          data-testid="range-marker"
          sx={{
            position: 'absolute',
            top: -3,
            width: 12,
            height: 12,
            borderRadius: '50%',
            bgcolor: 'primary.main',
            transform: 'translateX(-50%)',
          }}
          style={{ left: `${fraction * 100}%` }}
        />
      </Box>
      <Stack direction="row" sx={{ justifyContent: 'space-between' }}>
        <Typography variant="caption" color="text.secondary">
          {formatNum(low)}
        </Typography>
        <Typography variant="caption" color="text.secondary">
          {formatNum(high)}
        </Typography>
      </Stack>
    </Stack>
  )
}
