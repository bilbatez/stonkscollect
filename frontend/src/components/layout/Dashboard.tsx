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
import PersonIcon from '@mui/icons-material/Person'
import ViewListIcon from '@mui/icons-material/ViewList'
import StarBorderIcon from '@mui/icons-material/StarBorder'
import {
  addWatch,
  createGroup,
  deleteGroup,
  getGroups,
  getMe,
  getSectors,
  getWatchlistQuotes,
  loadCompanyData,
  removeWatch,
  renameGroup,
  tagWatch,
  untagWatch,
} from '../../api'
import { CompareView } from '../pages/CompareView'
import { AllStocks } from '../pages/AllStocks'
import { CompanyView } from '../pages/CompanyView'
import { Profile } from '../pages/Profile'
import { MarketSummary } from '../panels/MarketSummary'
import { TrendingStrip } from '../panels/TrendingStrip'
import { MoversView } from '../pages/MoversView'
import { SectorOverview } from '../pages/SectorOverview'
import { Screener } from '../pages/Screener'
import { Skeleton } from '../shared/Skeleton'
import { ThemeToggle, type Theme } from '../shared/ThemeToggle'
import { Watchlist } from '../layout/Watchlist'
import type { CompanyData, SectorStats, WatchGroup, WatchQuote } from '../../types'

type Page = 'home' | 'compare' | 'screen' | 'movers' | 'sectors' | 'profile'

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
  const [groups, setGroups] = useState<WatchGroup[]>([])
  const [displayName, setDisplayName] = useState('')
  const [page, setPage] = useState<Page>('home')
  const [tab, setTab] = useState(0)
  const [detail, setDetail] = useState<Detail>({ kind: 'none' })
  const [sectors, setSectors] = useState<SectorStats[]>([])

  const refreshWatchlist = useCallback(async () => {
    setItems(await getWatchlistQuotes())
  }, [])

  const refreshGroups = useCallback(async () => {
    setGroups(await getGroups())
  }, [])

  const refreshMe = useCallback(async () => {
    const me = await getMe()
    setDisplayName(me.display_name !== '' ? me.display_name : me.email)
  }, [])

  // Load the watchlist, groups, and profile once on mount.
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refreshWatchlist()
    void refreshGroups()
    void refreshMe()
  }, [refreshWatchlist, refreshGroups, refreshMe])

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
  async function createGroupAndRefresh(name: string) {
    await createGroup(name)
    await refreshGroups()
  }
  async function renameGroupAndRefresh(id: number, name: string) {
    await renameGroup(id, name)
    await refreshGroups()
  }
  async function deleteGroupAndRefresh(id: number) {
    await deleteGroup(id)
    await refreshGroups()
    await refreshWatchlist()
  }
  async function tag(ticker: string, groupId: number) {
    await tagWatch(ticker, groupId)
    await refreshWatchlist()
  }
  async function untag(ticker: string, groupId: number) {
    await untagWatch(ticker, groupId)
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
          <Button color="inherit" startIcon={<PersonIcon />} onClick={() => setPage('profile')}>
            {displayName !== '' ? displayName : 'Profile'}
          </Button>
          <Button color="inherit" startIcon={<LogoutIcon />} onClick={onLogout}>
            Log out
          </Button>
        </Toolbar>
      </AppBar>

      <Container maxWidth="xl" sx={{ py: 3 }}>
        {page === 'home' && (
          <Box>
            <MarketSummary />
            <TrendingStrip onSelect={(t) => void select(t)} />
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
                groups={groups}
                onSelect={(t) => void select(t)}
                onAdd={(t) => void add(t)}
                onRemove={(t) => void remove(t)}
                onCreateGroup={(n) => void createGroupAndRefresh(n)}
                onRenameGroup={(id, n) => void renameGroupAndRefresh(id, n)}
                onDeleteGroup={(id) => void deleteGroupAndRefresh(id)}
                onTag={(t, g) => void tag(t, g)}
                onUntag={(t, g) => void untag(t, g)}
              />
            )}
          </Box>
        )}
        {page === 'profile' && <Profile onProfileSaved={() => void refreshMe()} />}
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
