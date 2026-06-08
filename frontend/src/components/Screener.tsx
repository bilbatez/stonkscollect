import type { ScreenRow } from '../types'

interface Props {
  rows: ScreenRow[]
  defensiveOnly: boolean
  onToggleDefensive: () => void
  onSelect: (ticker: string) => void
}

/** Graham screener results, ranked by score. */
export function Screener({ rows, defensiveOnly, onToggleDefensive, onSelect }: Props) {
  return (
    <section className="screener">
      <header>
        <h2>Screener</h2>
        <label>
          <input type="checkbox" checked={defensiveOnly} onChange={onToggleDefensive} /> Defensive
          only
        </label>
      </header>
      {rows.length === 0 ? (
        <p>No matches.</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Ticker</th>
              <th>Score</th>
              <th>Graham #</th>
              <th>Margin</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.company.ticker}>
                <td>
                  <button type="button" className="link" onClick={() => onSelect(r.company.ticker)}>
                    {r.company.ticker}
                  </button>
                </td>
                <td>{r.score.score}</td>
                <td>{r.score.graham_number === null ? '—' : r.score.graham_number.toFixed(2)}</td>
                <td>
                  {r.score.margin_of_safety === null
                    ? '—'
                    : `${(r.score.margin_of_safety * 100).toFixed(0)}%`}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}
