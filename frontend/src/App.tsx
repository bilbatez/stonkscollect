import { lazy, Suspense, useState, type FormEvent } from 'react'
import { loadCompanyData } from './api'
import { freshness } from './format'
import { DiscrepancyPanel } from './components/DiscrepancyPanel'
import { FreshnessBadge } from './components/FreshnessBadge'
import { NewsFeed } from './components/NewsFeed'
import { RatiosPanel } from './components/RatiosPanel'
import { StatementTable } from './components/StatementTable'
import type { CompanyData } from './types'

// Code-split the heavy echarts bundle out of the initial page load.
const PriceChart = lazy(() => import('./charts/PriceChart'))

type Status = 'idle' | 'loading' | 'loaded' | 'error'

function Dashboard({ data, loadedAt }: { data: CompanyData; loadedAt: number }) {
  const { company } = data
  const latestPriceDate = data.prices[0]?.date ?? null
  return (
    <section>
      <header className="company-header">
        <h2>
          {company.name} ({company.ticker})
        </h2>
        <FreshnessBadge status={freshness(latestPriceDate, loadedAt)} />
      </header>

      <h3>Price</h3>
      <Suspense fallback={<p>Loading chart…</p>}>
        <PriceChart prices={data.prices} />
      </Suspense>

      <h3>Statements</h3>
      <StatementTable facts={data.facts} />

      <h3>Ratios</h3>
      <RatiosPanel ratios={data.ratios} />

      <h3>News</h3>
      <NewsFeed news={data.news} />

      <h3>Discrepancies</h3>
      <DiscrepancyPanel discrepancies={data.discrepancies} />
    </section>
  )
}

function App() {
  const [ticker, setTicker] = useState('')
  const [status, setStatus] = useState<Status>('idle')
  const [data, setData] = useState<CompanyData | null>(null)
  const [loadedAt, setLoadedAt] = useState(0)
  const [error, setError] = useState('')

  async function submit(e: FormEvent) {
    e.preventDefault()
    setStatus('loading')
    setError('')
    try {
      const loaded = await loadCompanyData(ticker.trim())
      setData(loaded)
      setLoadedAt(Date.now())
      setStatus('loaded')
    } catch (err) {
      setError(err instanceof Error ? err.message : 'failed to load')
      setStatus('error')
    }
  }

  return (
    <main>
      <h1>StonksCollect</h1>
      <form onSubmit={submit}>
        <input
          aria-label="ticker"
          placeholder="Ticker (e.g. AAPL)"
          value={ticker}
          onChange={(e) => setTicker(e.target.value)}
        />
        <button type="submit">Load</button>
      </form>

      {status === 'loading' && <p>Loading…</p>}
      {status === 'error' && <p role="alert">{error}</p>}
      {status === 'loaded' && data !== null && <Dashboard data={data} loadedAt={loadedAt} />}
    </main>
  )
}

export default App
