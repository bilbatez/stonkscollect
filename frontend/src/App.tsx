import { lazy, Suspense, useCallback, useEffect, useState } from 'react'
import {
  addWatch,
  getToken,
  getWatchlist,
  loadCompanyData,
  logout,
  removeWatch,
} from './api'
import { freshness } from './format'
import { AuthForm } from './components/AuthForm'
import { Compare, type CompareRow } from './components/Compare'
import { DiscrepancyPanel } from './components/DiscrepancyPanel'
import { FreshnessBadge } from './components/FreshnessBadge'
import { NewsFeed } from './components/NewsFeed'
import { RatiosPanel } from './components/RatiosPanel'
import { Skeleton } from './components/Skeleton'
import { StatementTable } from './components/StatementTable'
import { ThemeToggle, type Theme } from './components/ThemeToggle'
import { Watchlist } from './components/Watchlist'
import type { Company, CompanyData } from './types'

const PriceChart = lazy(() => import('./charts/PriceChart'))

type View =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'error'; ticker: string }
  | { kind: 'company'; data: CompanyData; loadedAt: number }
  | { kind: 'compare'; rows: CompareRow[] }

/** Latest value per ratio metric (later periods overwrite earlier). */
function latestMetrics(data: CompanyData): Record<string, number> {
  const m: Record<string, number> = {}
  for (const r of data.ratios) {
    m[r.metric] = r.value
  }
  return m
}

function Dashboard({
  onLogout,
  theme,
  onToggleTheme,
}: {
  onLogout: () => void
  theme: Theme
  onToggleTheme: () => void
}) {
  const [items, setItems] = useState<Company[]>([])
  const [view, setView] = useState<View>({ kind: 'idle' })

  const refreshWatchlist = useCallback(async () => {
    setItems(await getWatchlist())
  }, [])

  // Load the watchlist once on mount (async fetch, not a synchronous cascade).
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refreshWatchlist()
  }, [refreshWatchlist])

  async function select(ticker: string) {
    setView({ kind: 'loading' })
    try {
      const data = await loadCompanyData(ticker)
      setView({ kind: 'company', data, loadedAt: Date.now() })
    } catch {
      setView({ kind: 'error', ticker })
    }
  }

  async function add(ticker: string) {
    await addWatch(ticker)
    await refreshWatchlist()
  }
  async function remove(ticker: string) {
    await removeWatch(ticker)
    await refreshWatchlist()
  }

  async function compare() {
    setView({ kind: 'loading' })
    const all = await Promise.all(items.map((c) => loadCompanyData(c.ticker)))
    const rows = all.map((d) => ({ ticker: d.company.ticker, metrics: latestMetrics(d) }))
    setView({ kind: 'compare', rows })
  }

  return (
    <div className="app">
      <header className="topbar">
        <h1>StonksCollect</h1>
        <div>
          <button type="button" onClick={() => void compare()}>
            Compare
          </button>
          <ThemeToggle theme={theme} onToggle={onToggleTheme} />
          <button type="button" onClick={onLogout}>
            Log out
          </button>
        </div>
      </header>

      <div className="layout">
        <Watchlist
          items={items}
          onSelect={(t) => void select(t)}
          onAdd={(t) => void add(t)}
          onRemove={(t) => void remove(t)}
        />
        <section className="content">
          {view.kind === 'idle' && <p>Select a ticker to begin.</p>}
          {view.kind === 'loading' && <Skeleton />}
          {view.kind === 'error' && (
            <div role="alert">
              <p>Failed to load {view.ticker}.</p>
              <button type="button" onClick={() => void select(view.ticker)}>
                Retry
              </button>
            </div>
          )}
          {view.kind === 'compare' && <Compare rows={view.rows} />}
          {view.kind === 'company' && <CompanyView data={view.data} loadedAt={view.loadedAt} />}
        </section>
      </div>
    </div>
  )
}

function CompanyView({ data, loadedAt }: { data: CompanyData; loadedAt: number }) {
  const latestPriceDate = data.prices[0]?.date ?? null
  return (
    <article>
      <header className="company-header">
        <h2>
          {data.company.name} ({data.company.ticker})
        </h2>
        <FreshnessBadge status={freshness(latestPriceDate, loadedAt)} />
      </header>
      <h3>Price</h3>
      <Suspense fallback={<Skeleton label="Loading chart…" />}>
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
    </article>
  )
}

function App() {
  const [token, setTokenState] = useState<string | null>(getToken())
  const [theme, setTheme] = useState<Theme>('light')

  useEffect(() => {
    document.documentElement.dataset.theme = theme
  }, [theme])

  if (token === null) {
    return <AuthForm onAuth={setTokenState} />
  }
  return (
    <Dashboard
      onLogout={() => {
        void logout()
        setTokenState(null)
      }}
      theme={theme}
      onToggleTheme={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
    />
  )
}

export default App
