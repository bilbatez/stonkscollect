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
import HomeIcon from '@mui/icons-material/Home'
import CompareArrowsIcon from '@mui/icons-material/CompareArrows'
import FilterAltIcon from '@mui/icons-material/FilterAlt'
import LogoutIcon from '@mui/icons-material/Logout'
import ViewListIcon from '@mui/icons-material/ViewList'
import StarBorderIcon from '@mui/icons-material/StarBorder'
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

/** Which top-level page is showing. The company detail lives *inside* the home
 *  page (under the tabs) so selecting a stock never hides the tabs. */
type Page = 'home' | 'compare' | 'screen'

/** State of the in-home company detail panel. */
type Detail =
  | { kind: 'none' }
  | { kind: 'loading' }
  | { kind: 'error'; ticker: string }
  | { kind: 'company'; data: CompanyData; loadedAt: number }

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
  const [page, setPage] = useState<Page>('home')
  const [tab, setTab] = useState(0)
  const [detail, setDetail] = useState<Detail>({ kind: 'none' })
  const [compareRows, setCompareRows] = useState<CompareRow[]>([])
  const [comparing, setComparing] = useState(false)

  const refreshWatchlist = useCallback(async () => {
    setItems(await getWatchlist())
  }, [])

  // Load the watchlist once on mount (async fetch, not a synchronous cascade).
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refreshWatchlist()
  }, [refreshWatchlist])

  // Open a company in the home detail panel (tabs stay visible).
  async function select(ticker: string) {
    setPage('home')
    setDetail({ kind: 'loading' })
    try {
      const data = await loadCompanyData(ticker)
      setDetail({ kind: 'company', data, loadedAt: Date.now() })
    } catch {
      setDetail({ kind: 'error', ticker })
    }
  }

  function showTab(next: number) {
    setTab(next)
    setDetail({ kind: 'none' }) // back to the list when switching tabs
  }

  async function add(ticker: string) {
    await addWatch(ticker)
    await refreshWatchlist()
  }
  async function remove(ticker: string) {
    await removeWatch(ticker)
    await refreshWatchlist()
  }

  // Compare the watchlist; tolerate individual load failures (never hang).
  async function compare() {
    setPage('compare')
    setComparing(true)
    const settled = await Promise.allSettled(items.map((c) => loadCompanyData(c.ticker)))
    setCompareRows(
      settled.flatMap((r) =>
        r.status === 'fulfilled'
          ? [{ ticker: r.value.company.ticker, metrics: latestMetrics(r.value) }]
          : [],
      ),
    )
    setComparing(false)
  }

  return (
    <Box sx={{ minHeight: '100vh', bgcolor: 'background.default' }}>
      <AppBar position="static" color="default" enableColorOnDark>
        <Toolbar sx={{ gap: 1 }}>
          <Typography variant="h6" component="h1" sx={{ flexGrow: 1, fontWeight: 700 }}>
            StonksCollect
          </Typography>
          <Button color="inherit" startIcon={<HomeIcon />} onClick={() => setPage('home')}>
            Home
          </Button>
          <Button color="inherit" startIcon={<CompareArrowsIcon />} onClick={() => void compare()}>
            Compare
          </Button>
          <Button color="inherit" startIcon={<FilterAltIcon />} onClick={() => setPage('screen')}>
            Screener
          </Button>
          <ThemeToggle theme={theme} onToggle={onToggleTheme} />
          <Button color="inherit" startIcon={<LogoutIcon />} onClick={onLogout}>
            Log out
          </Button>
        </Toolbar>
      </AppBar>

      <Container maxWidth="xl" sx={{ py: 3 }}>
        {page === 'home' && (
          <Box>
            <Tabs value={tab} onChange={(_e, v: number) => showTab(v)} sx={{ mb: 2 }}>
              <Tab icon={<ViewListIcon />} iconPosition="start" label="All Stocks" />
              <Tab icon={<StarBorderIcon />} iconPosition="start" label="Watchlist" />
            </Tabs>
            {detail.kind === 'company' ? (
              <Box>
                <Button size="small" onClick={() => setDetail({ kind: 'none' })} sx={{ mb: 1 }}>
                  ← Back to list
                </Button>
                <CompanyView data={detail.data} loadedAt={detail.loadedAt} />
              </Box>
            ) : detail.kind === 'loading' ? (
              <Skeleton />
            ) : detail.kind === 'error' ? (
              <Alert
                severity="error"
                action={
                  <Button color="inherit" size="small" onClick={() => void select(detail.ticker)}>
                    Retry
                  </Button>
                }
              >
                Failed to load {detail.ticker}.
              </Alert>
            ) : tab === 0 ? (
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
        {page === 'compare' && (comparing ? <Skeleton /> : <Compare rows={compareRows} />)}
        {page === 'screen' && <Screener onSelect={(t) => void select(t)} />}
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
