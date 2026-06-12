import { Stack, Typography } from '@mui/material'
import { formatPct, formatPeriodDate } from '../../format'
import type { Quote } from '../../quote'

/** Yahoo-style price banner: last close, colored day change, as-of date. */
export function QuoteHeader({ quote }: { quote: Quote | null }) {
  if (!quote) return null
  const up = (quote.change ?? 0) >= 0
  return (
    <Stack direction="row" spacing={2} sx={{ mt: 2, alignItems: 'baseline', flexWrap: 'wrap' }}>
      <Typography variant="h4" component="p" sx={{ fontWeight: 700 }} aria-label="last price">
        {quote.last.toFixed(2)}
      </Typography>
      {quote.change !== null && quote.changePct !== null && (
        <Typography
          variant="h6"
          component="p"
          color={up ? 'success.main' : 'error.main'}
          aria-label="day change"
        >
          {up ? '+' : ''}
          {quote.change.toFixed(2)} ({up ? '+' : ''}
          {formatPct(quote.changePct)})
        </Typography>
      )}
      <Typography variant="body2" color="text.secondary">
        at close, {formatPeriodDate(quote.asOf)}
      </Typography>
    </Stack>
  )
}
