import { Chip } from '@mui/material'
import type { Freshness } from '../../format'

const LABELS: Record<Freshness, string> = {
  fresh: 'Fresh',
  stale: 'Stale',
  unknown: 'Unknown',
}

const COLORS: Record<Freshness, 'success' | 'warning' | 'default'> = {
  fresh: 'success',
  stale: 'warning',
  unknown: 'default',
}

/** Small badge showing data freshness. */
export function FreshnessBadge({ status }: { status: Freshness }) {
  return <Chip size="small" color={COLORS[status]} label={LABELS[status]} />
}
