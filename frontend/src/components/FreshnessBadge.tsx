import type { Freshness } from '../format'

const LABELS: Record<Freshness, string> = {
  fresh: 'Fresh',
  stale: 'Stale',
  unknown: 'Unknown',
}

/** Small badge showing data freshness. */
export function FreshnessBadge({ status }: { status: Freshness }) {
  return <span className={`badge badge-${status}`}>{LABELS[status]}</span>
}
