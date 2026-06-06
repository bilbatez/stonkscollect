import { formatCurrency } from '../format'
import type { Discrepancy } from '../types'

/** Cross-source mismatches flagged by the reconcile layer. */
export function DiscrepancyPanel({ discrepancies }: { discrepancies: Discrepancy[] }) {
  if (discrepancies.length === 0) {
    return <p>No discrepancies flagged.</p>
  }
  return (
    <table className="discrepancies">
      <thead>
        <tr>
          <th>Field</th>
          <th>Period</th>
          <th>Sources</th>
          <th>Diff</th>
        </tr>
      </thead>
      <tbody>
        {discrepancies.map((d, i) => (
          <tr key={`${d.field}-${d.period ?? 'na'}-${i}`}>
            <td>{d.field}</td>
            <td>{d.period ?? '—'}</td>
            <td>
              {d.source_a} {formatCurrency(d.value_a)} vs {d.source_b} {formatCurrency(d.value_b)}
            </td>
            <td>{(d.pct_diff * 100).toFixed(1)}%</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
