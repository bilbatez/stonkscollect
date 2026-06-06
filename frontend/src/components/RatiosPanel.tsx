import type { Ratio } from '../types'

/** Table of derived ratios. */
export function RatiosPanel({ ratios }: { ratios: Ratio[] }) {
  if (ratios.length === 0) {
    return <p>No ratio data.</p>
  }
  return (
    <table className="ratios">
      <thead>
        <tr>
          <th>Metric</th>
          <th>Period</th>
          <th>Value</th>
        </tr>
      </thead>
      <tbody>
        {ratios.map((r) => (
          <tr key={`${r.metric}-${r.period_end}`}>
            <td>{r.metric}</td>
            <td>{r.period_end}</td>
            <td>{r.value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
