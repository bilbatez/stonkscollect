import { Chip, Stack, Typography } from '@mui/material'
import { formatPct, formatPeriodDate } from '../../format'
import type { PeriodReturn, Quote } from '../../quote'

/** Yahoo-style price banner: last close, colored day change, as-of date, and an
 *  optional row of trailing-return chips (1D/5D/1M/6M/YTD/1Y/5Y). */
export function QuoteHeader({ quote, returns }: { quote: Quote | null; returns?: PeriodReturn[] }) {
  if (!quote) return null
  const up = (quote.change ?? 0) >= 0
  return (
    <Stack spacing={1} sx={{ mt: 2 }}>
      <Stack direction="row" spacing={2} sx={{ alignItems: 'baseline', flexWrap: 'wrap' }}>
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
      {returns && returns.length > 0 && (
        <Stack direction="row" spacing={1} sx={{ flexWrap: 'wrap', gap: 1 }}>
          {returns.map((r) => {
            const positive = r.pct !== null && r.pct >= 0
            return (
              <Chip
                key={r.period}
                size="small"
                variant="outlined"
                aria-label={`${r.period} return`}
                color={r.pct === null ? 'default' : positive ? 'success' : 'error'}
                label={`${r.period} ${r.pct === null ? '—' : `${positive ? '+' : ''}${formatPct(r.pct)}`}`}
              />
            )
          })}
        </Stack>
      )}
    </Stack>
  )
}
