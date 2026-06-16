import { useCallback, useEffect, useState } from 'react'
import {
  AppBar,
  Alert,
  Box,
  Button,
  Container,
  Tab,
  Tabs,
  Toolbar,
  Typography,
} from '@mui/material'
import HomeIcon from '@mui/icons-material/Home'
import CompareArrowsIcon from '@mui/icons-material/CompareArrows'
import FilterAltIcon from '@mui/icons-material/FilterAlt'
import TrendingUpIcon from '@mui/icons-material/TrendingUp'
import LogoutIcon from '@mui/icons-material/Logout'
import ViewListIcon from '@mui/icons-material/ViewList'
import StarBorderIcon from '@mui/icons-material/StarBorder'
import { addWatch, getSectors, getWatchlistQuotes, loadCompanyData, removeWatch } from '../../api'
import { CompareView } from '../pages/CompareView'
import { AllStocks } from '../pages/AllStocks'
import { CompanyView } from '../pages/CompanyView'
import { MarketSummary } from '../panels/MarketSummary'
import { MoversView } from '../pages/MoversView'
import { SectorOverview } from '../pages/SectorOverview'
import { Screener } from '../pages/Screener'
import { Skeleton } from '../shared/Skeleton'
import { ThemeToggle, type Theme } from '../shared/ThemeToggle'
import { Watchlist } from '../layout/Watchlist'
import type { CompanyData, SectorStats, WatchQuote } from '../../types'

type Page = 'home' | 'compare' | 'screen' | 'movers' | 'sectors'

type Detail =
  | { kind: 'none' }
  | { kind: 'loading' }
  | { kind: 'error'; ticker: string }
  | { kind: 'company'; data: CompanyData; loadedAt: number }

export function Dashboard({
  onLogout,
  theme,
  onToggleTheme,
}: {
  onLogout: () => void
  theme: Theme
  onToggleTheme: () => void
}) {
  const [items, setItems] = useState<WatchQuote[]>([])
  const [page, setPage] = useState<Page>('home')
  const [tab, setTab] = useState(0)
  const [detail, setDetail] = useState<Detail>({ kind: 'none' })
  const [sectors, setSectors] = useState<SectorStats[]>([])

  const refreshWatchlist = useCallback(async () => {
    setItems(await getWatchlistQuotes())
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
    setDetail({ kind: 'none' })
  }

  async function add(ticker: string) {
    await addWatch(ticker)
    await refreshWatchlist()
  }
  async function remove(ticker: string) {
    await removeWatch(ticker)
    await refreshWatchlist()
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
          <Button color="inherit" startIcon={<CompareArrowsIcon />} onClick={() => setPage('compare')}>
            Compare
          </Button>
          <Button color="inherit" startIcon={<FilterAltIcon />} onClick={() => setPage('screen')}>
            Screener
          </Button>
          <Button color="inherit" startIcon={<TrendingUpIcon />} onClick={() => setPage('movers')}>
            Movers
          </Button>
          <Button
            color="inherit"
            onClick={() => {
              setPage('sectors')
              void getSectors().then(setSectors)
            }}
          >
            Sectors
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
            <MarketSummary />
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
        {page === 'compare' && <CompareView />}
        {page === 'screen' && <Screener onSelect={(t) => void select(t)} />}
        {page === 'movers' && <MoversView onSelect={(t) => void select(t)} />}
        {page === 'sectors' && (
          <SectorOverview
            sectors={sectors}
            onSelect={(ticker) => {
              void select(ticker)
              setPage('home')
            }}
          />
        )}
      </Container>
    </Box>
  )
}
