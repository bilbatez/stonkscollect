import type { GrahamAssessment } from '../types'

function pct(x: number | null): string {
  return x === null ? '—' : `${(x * 100).toFixed(0)}%`
}
function num(x: number | null): string {
  return x === null ? '—' : x.toFixed(2)
}

/** Graham defensive-investor scorecard for one company. */
export function GrahamScorecard({ assessment }: { assessment: GrahamAssessment }) {
  const { criteria, score, passes_defensive, graham_number, margin_of_safety, net_net } = assessment
  return (
    <section className="graham">
      <header>
        <h3>Graham scorecard</h3>
        <span className={`badge badge-${passes_defensive ? 'fresh' : 'stale'}`}>
          {score}/{criteria.length} {passes_defensive ? '— defensive' : ''}
        </span>
        {net_net && <span className="badge badge-fresh">net-net</span>}
      </header>
      <ul className="criteria">
        {criteria.map((c) => (
          <li key={c.name} className={c.passed ? 'pass' : 'fail'}>
            <span aria-label={c.passed ? 'pass' : 'fail'}>{c.passed ? '✓' : '✗'}</span> {c.name}
            <span className="detail"> — {c.detail}</span>
          </li>
        ))}
      </ul>
      <dl className="valuation">
        <dt>Graham Number</dt>
        <dd>{num(graham_number)}</dd>
        <dt>Margin of safety</dt>
        <dd>{pct(margin_of_safety)}</dd>
      </dl>
    </section>
  )
}
