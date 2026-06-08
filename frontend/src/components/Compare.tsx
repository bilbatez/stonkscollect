export interface CompareRow {
  ticker: string
  metrics: Record<string, number>
}

/** Compare a set of ratio metrics across multiple tickers. */
export function Compare({ rows }: { rows: CompareRow[] }) {
  if (rows.length === 0) {
    return <p>Nothing to compare.</p>
  }
  // Union of metric names across rows, sorted for stable columns.
  const metrics = [...new Set(rows.flatMap((r) => Object.keys(r.metrics)))].sort()
  return (
    <table className="compare">
      <thead>
        <tr>
          <th>Ticker</th>
          {metrics.map((m) => (
            <th key={m}>{m}</th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.ticker}>
            <td>{r.ticker}</td>
            {metrics.map((m) => {
              const v = r.metrics[m]
              return <td key={m}>{v === undefined ? '—' : v.toFixed(2)}</td>
            })}
          </tr>
        ))}
      </tbody>
    </table>
  )
}
