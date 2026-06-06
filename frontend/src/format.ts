/** Format a USD amount with a B/M suffix for large values. */
export function formatCurrency(value: number): string {
  const sign = value < 0 ? '-' : ''
  const abs = Math.abs(value)
  if (abs >= 1_000_000_000) {
    return `${sign}$${(abs / 1_000_000_000).toFixed(1)}B`
  }
  if (abs >= 1_000_000) {
    return `${sign}$${(abs / 1_000_000).toFixed(1)}M`
  }
  return `${sign}$${abs}`
}

export type Freshness = 'fresh' | 'stale' | 'unknown'

/** Classify how fresh a timestamp is relative to `nowMs` (ms since epoch). */
export function freshness(iso: string | null, nowMs: number): Freshness {
  if (iso === null) {
    return 'unknown'
  }
  const ageDays = (nowMs - Date.parse(iso)) / 86_400_000
  return ageDays < 2 ? 'fresh' : 'stale'
}
