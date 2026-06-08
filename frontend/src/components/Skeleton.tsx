/** Loading placeholder. */
export function Skeleton({ label = 'Loading…' }: { label?: string }) {
  return (
    <div className="skeleton" role="status" aria-live="polite">
      {label}
    </div>
  )
}
