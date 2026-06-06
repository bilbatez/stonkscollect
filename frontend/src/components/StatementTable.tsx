import { formatCurrency } from '../format'
import type { FinancialFact } from '../types'

/** Pivot facts into a line-item × period table (periods newest-first). */
export function StatementTable({ facts }: { facts: FinancialFact[] }) {
  if (facts.length === 0) {
    return <p>No statement data.</p>
  }

  const periods = [...new Set(facts.map((f) => f.period_end))].sort().reverse()
  const byItem = new Map<string, Map<string, number>>()
  for (const f of facts) {
    if (!byItem.has(f.line_item)) {
      byItem.set(f.line_item, new Map())
    }
    byItem.get(f.line_item)!.set(f.period_end, f.value)
  }

  return (
    <table className="statements">
      <thead>
        <tr>
          <th>Line item</th>
          {periods.map((p) => (
            <th key={p}>{p}</th>
          ))}
        </tr>
      </thead>
      <tbody>
        {[...byItem.entries()].map(([item, values]) => (
          <tr key={item}>
            <td>{item}</td>
            {periods.map((p) => {
              const v = values.get(p)
              return <td key={p}>{v === undefined ? '—' : formatCurrency(v)}</td>
            })}
          </tr>
        ))}
      </tbody>
    </table>
  )
}
