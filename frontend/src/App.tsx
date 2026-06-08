import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react'
import {
  AppBar,
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  Container,
  CssBaseline,
  Stack,
  Tab,
  Tabs,
  ThemeProvider,
  Toolbar,
  Typography,
  createTheme,
} from '@mui/material'
import { addWatch, getToken, getWatchlist, loadCompanyData, logout, removeWatch } from './api'
import { freshness } from './format'
import { AllStocks } from './components/AllStocks'
import { AuthForm } from './components/AuthForm'
import { Compare, type CompareRow } from './components/Compare'
import { DiscrepancyPanel } from './components/DiscrepancyPanel'
import { FreshnessBadge } from './components/FreshnessBadge'
import { GrahamScorecard } from './components/GrahamScorecard'
import { NewsFeed } from './components/NewsFeed'
import { RatiosPanel } from './components/RatiosPanel'
import { Screener } from './components/Screener'
import { Skeleton } from './components/Skeleton'
import { StatementTable } from './components/StatementTable'
import { ThemeToggle, type Theme } from './components/ThemeToggle'
import { Watchlist } from './components/Watchlist'
import type { Company, CompanyData } from './types'

const PriceChart = lazy(() => import('./charts/PriceChart'))

type View =
  | { kind: 'home' }
  | { kind: 'loading' }
  | { kind: 'error'; ticker: string }
  | { kind: 'company'; data: CompanyData; loadedAt: number }
  | { kind: 'compare'; rows: CompareRow[] }
  | { kind: 'screen' }

/** Latest annual value per ratio metric (later periods overwrite earlier). */
function latestMetrics(data: CompanyData): Record<string, number> {
  const m: Record<string, number> = {}
  for (const r of data.ratios) {
    if (r.period_type === 'annual') {
      m[r.metric] = r.value
    }
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
  const [view, setView] = useState<View>({ kind: 'home' })
  const [tab, setTab] = useState(0)

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
    <Box sx={{ minHeight: '100vh', bgcolor: 'background.default' }}>
      <AppBar position="static" color="default" enableColorOnDark>
        <Toolbar sx={{ gap: 1 }}>
          <Typography variant="h6" component="h1" sx={{ flexGrow: 1, fontWeight: 700 }}>
            StonksCollect
          </Typography>
          <Button color="inherit" onClick={() => setView({ kind: 'home' })}>
            Home
          </Button>
          <Button color="inherit" onClick={() => void compare()}>
            Compare
          </Button>
          <Button color="inherit" onClick={() => setView({ kind: 'screen' })}>
            Screener
          </Button>
          <ThemeToggle theme={theme} onToggle={onToggleTheme} />
          <Button color="inherit" onClick={onLogout}>
            Log out
          </Button>
        </Toolbar>
      </AppBar>

      <Container maxWidth="xl" sx={{ py: 3 }}>
        {view.kind === 'home' && (
          <Box>
            <Tabs value={tab} onChange={(_e, v: number) => setTab(v)} sx={{ mb: 2 }}>
              <Tab label="All Stocks" />
              <Tab label="Watchlist" />
            </Tabs>
            {tab === 0 ? (
              <AllStocks onSelect={(t) => void select(t)} onAdd={(t) => void add(t)} />
            ) : (
              <Watchlist
                items={items}
                onSelect={(t) => void select(t)}
                onAdd={(t) => void add(t)}
                onRemove={(t) => void remove(t)}
              />
            )}
          </Box>
        )}
        {view.kind === 'loading' && <Skeleton />}
        {view.kind === 'error' && (
          <Alert
            severity="error"
            action={
              <Button color="inherit" size="small" onClick={() => void select(view.ticker)}>
                Retry
              </Button>
            }
          >
            Failed to load {view.ticker}.
          </Alert>
        )}
        {view.kind === 'compare' && <Compare rows={view.rows} />}
        {view.kind === 'screen' && <Screener onSelect={(t) => void select(t)} />}
        {view.kind === 'company' && <CompanyView data={view.data} loadedAt={view.loadedAt} />}
      </Container>
    </Box>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Box sx={{ mt: 3 }}>
      <Typography variant="subtitle1" component="h3" gutterBottom sx={{ fontWeight: 600 }}>
        {title}
      </Typography>
      {children}
    </Box>
  )
}

function CompanyView({ data, loadedAt }: { data: CompanyData; loadedAt: number }) {
  const latestPriceDate = data.prices[0]?.date ?? null
  return (
    <Card variant="outlined" component="article">
      <CardContent>
        <Stack direction="row" spacing={2} sx={{ alignItems: 'center', flexWrap: 'wrap' }}>
          <Typography variant="h5" component="h2">
            {data.company.name} ({data.company.ticker})
          </Typography>
          <FreshnessBadge status={freshness(latestPriceDate, loadedAt)} />
        </Stack>
        <Section title="Price">
          <Suspense fallback={<Skeleton label="Loading chart…" />}>
            <PriceChart prices={data.prices} />
          </Suspense>
        </Section>
        <Section title="Statements">
          <StatementTable facts={data.facts} />
        </Section>
        <Box sx={{ mt: 3 }}>
          <GrahamScorecard assessment={data.graham} />
        </Box>
        <Section title="Ratios">
          <RatiosPanel ratios={data.ratios} />
        </Section>
        <Section title="News">
          <NewsFeed news={data.news} />
        </Section>
        <Section title="Discrepancies">
          <DiscrepancyPanel discrepancies={data.discrepancies} />
        </Section>
      </CardContent>
    </Card>
  )
}

function App() {
  const [token, setTokenState] = useState<string | null>(getToken())
  const [theme, setTheme] = useState<Theme>('dark')

  useEffect(() => {
    document.documentElement.dataset.theme = theme
  }, [theme])

  const muiTheme = useMemo(
    () =>
      createTheme({
        palette: {
          mode: theme,
          primary: { main: '#14b8a6' },
          success: { main: '#22c55e' },
          error: { main: '#ef4444' },
          ...(theme === 'dark'
            ? { background: { default: '#0e1116', paper: '#161b22' } }
            : {}),
        },
      }),
    [theme],
  )

  return (
    <ThemeProvider theme={muiTheme}>
      <CssBaseline />
      {token === null ? (
        <AuthForm onAuth={setTokenState} />
      ) : (
        <Dashboard
          onLogout={() => {
            void logout()
            setTokenState(null)
          }}
          theme={theme}
          onToggleTheme={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
        />
      )}
    </ThemeProvider>
  )
}

export default App
