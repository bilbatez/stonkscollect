import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App'
import * as api from './api'
import type { Company, CompanyData, GrahamScore } from './types'

vi.mock('./charts/PriceChart', () => ({ default: () => <div data-testid="price-chart" /> }))
vi.mock('./charts/IncomeChart', () => ({ default: () => <div data-testid="income-chart" /> }))
vi.mock('./charts/RatioChart', () => ({ default: () => <div data-testid="ratio-chart" /> }))
vi.mock('./charts/GrahamChart', () => ({ default: () => <div data-testid="graham-chart" /> }))
vi.mock('./api')

const mocked = vi.mocked(api)

const company = (ticker: string): Company => ({
  id: 1, cik: '0000320193', ticker, name: `${ticker} Inc.`, exchange: 'NASDAQ',
  sector: 'Basic Materials', industry: 'Building Materials',
  description: `${ticker} makes things.`, website: 'https://example.com',
})

const grahamScore = (): GrahamScore => ({
  company_id: 1, score: 6, passes_defensive: false, graham_number: 60,
  ncav_per_share: null, margin_of_safety: 0.3, net_net: false, computed_at: '',
})

function data(ticker: string): CompanyData {
  return {
    company: company(ticker),
    prices: [{ company_id: 1, date: '2024-01-02', close: 1, volume: null, source: 'fmp' }],
    facts: [],
    ratios: [
      { company_id: 1, period_end: '2023-12-31', period_type: 'annual', metric: 'roe', value: 1.5, computed_at: '' },
      // a quarterly ratio that compare's annual-only filter must skip
      { company_id: 1, period_end: '2023-09-30', period_type: 'quarterly', metric: 'roe', value: 0.4, computed_at: '' },
    ],
    news: [],
    discrepancies: [],
    graham: {
      criteria: [{ name: 'Current ratio >= 2', passed: true, detail: 'current ratio 2.5' }],
      score: 1, graham_number: 22.4, ncav_per_share: null, margin_of_safety: 0.1,
      net_net: false, passes_defensive: true,
    },
    peers: [],
    note: { body: null },
    shares: null,
  }
}

beforeEach(() => {
  localStorage.clear()
  mocked.getToken.mockReturnValue(null)
  mocked.getMe.mockResolvedValue({ email: 'u@e.com', display_name: '' })
  mocked.getSettings.mockResolvedValue({
    theme: 'system',
    graham: { min_revenue: 5e8, pe_max: 15, pb_max: 1.5, pe_pb_max: 22.5, current_ratio_min: 2, eps_growth_min: 0.33 },
  })
  mocked.updateSettings.mockResolvedValue()
  mocked.getGroups.mockResolvedValue([])
  mocked.getWatchlistQuotes.mockResolvedValue([])
  mocked.getMarketSummary.mockResolvedValue([])
  mocked.getMovers.mockResolvedValue({ gainers: [], losers: [], most_active: [] })
  mocked.getHolders.mockResolvedValue([])
  mocked.listCompanies.mockResolvedValue({ rows: [{ company: company('AAPL'), score: grahamScore() }], total: 1 })
  mocked.screen.mockResolvedValue({ rows: [{ company: company('KO'), score: grahamScore() }], total: 1 })
  mocked.getSectors.mockResolvedValue([{ sector: 'Technology', company_count: 1, avg_score: 5, pct_defensive: 1, top_ticker: 'AAPL' }])
  mocked.saveNote.mockResolvedValue()
  mocked.deleteNote.mockResolvedValue()
})
afterEach(() => vi.clearAllMocks())

test('restores a persisted light theme from localStorage on load', async () => {
  mocked.getToken.mockReturnValue('tok')
  localStorage.setItem('stonks_theme', 'light')
  render(<App />)
  await screen.findByLabelText('search stocks')
  expect(document.documentElement.dataset.theme).toBe('light')
})

test('shows the auth form when logged out, dashboard after auth', async () => {
  render(<App />)
  expect(screen.getByRole('heading', { name: /stonkscollect/i })).toBeInTheDocument()
  expect(screen.getByLabelText('email')).toBeInTheDocument()

  mocked.login.mockResolvedValue('tok')
  await userEvent.type(screen.getByLabelText('email'), 'a@e.com')
  await userEvent.type(screen.getByLabelText('password'), 'pw')
  await userEvent.click(screen.getByRole('button', { name: /log in/i }))
  // lands on the All Stocks tab of the home dashboard
  await waitFor(() => expect(screen.getByLabelText('search stocks')).toBeInTheDocument())
})

test('home trending strip opens a company when a gainer chip is clicked', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.loadCompanyData.mockResolvedValue(data('AAPL'))
  mocked.getMovers.mockResolvedValue({
    gainers: [
      { company: company('AAPL'), last_close: 10, change: 0.5, change_pct: 0.05, volume: null, as_of: '2024-03-01' },
    ],
    losers: [],
    most_active: [],
  })
  render(<App />)
  await userEvent.click(await screen.findByText('AAPL +5%'))
  await waitFor(() => expect(screen.getByRole('heading', { name: /aapl inc/i })).toBeInTheDocument())
})

test('home All Stocks tab opens a company; logout returns to auth', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.loadCompanyData.mockResolvedValue(data('AAPL'))
  mocked.logout.mockResolvedValue()

  mocked.addWatch.mockResolvedValue()

  render(<App />)
  // add-to-watchlist from the All Stocks row
  await userEvent.click(await screen.findByRole('button', { name: 'watch AAPL' }))
  await waitFor(() => expect(mocked.addWatch).toHaveBeenCalledWith('AAPL'))
  // open the company
  await userEvent.click(screen.getByRole('button', { name: 'AAPL' }))
  await waitFor(() => expect(screen.getByRole('heading', { name: /aapl inc/i })).toBeInTheDocument())
  expect(await screen.findByTestId('price-chart')).toBeInTheDocument()

  // profile: sector/industry, description, and reference links
  expect(screen.getByText('Basic Materials')).toBeInTheDocument()
  expect(screen.getByText(/aapl makes things/i)).toBeInTheDocument()
  expect(screen.getByRole('link', { name: /sec filings/i })).toBeInTheDocument()
  expect(screen.getByRole('link', { name: /website/i })).toBeInTheDocument()
  expect(screen.getByRole('link', { name: /wikipedia/i })).toBeInTheDocument()
  expect(screen.getByRole('link', { name: /yahoo finance/i })).toBeInTheDocument()

  // tabs stay visible; "Back to list" returns to All Stocks without clicking Home
  await userEvent.click(screen.getByRole('button', { name: /back to list/i }))
  expect(await screen.findByLabelText('search stocks')).toBeInTheDocument()

  // back home, then logout
  await userEvent.click(screen.getByRole('button', { name: /home/i }))
  await userEvent.click(screen.getByRole('button', { name: /log out/i }))
  expect(screen.getByLabelText('email')).toBeInTheDocument()
})

test('Watchlist tab adds and removes tickers', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getWatchlistQuotes.mockResolvedValueOnce([]).mockResolvedValue([
    {
      company: company('MSFT'),
      last_close: 410.5,
      change: 4.5,
      change_pct: 0.011,
      volume: null,
      as_of: '2024-03-01',
      group_ids: [],
    },
  ])
  mocked.addWatch.mockResolvedValue()
  mocked.removeWatch.mockResolvedValue()
  mocked.loadCompanyData.mockResolvedValue(data('MSFT'))

  render(<App />)
  await userEvent.click(await screen.findByRole('tab', { name: /watchlist/i }))
  expect(await screen.findByText(/no tickers yet/i)).toBeInTheDocument()
  await userEvent.type(screen.getByLabelText('add ticker'), 'msft')
  await userEvent.click(screen.getByRole('button', { name: 'Add' }))
  await waitFor(() => expect(mocked.addWatch).toHaveBeenCalledWith('MSFT'))
  await userEvent.click(await screen.findByRole('button', { name: 'remove MSFT' }))
  expect(mocked.removeWatch).toHaveBeenCalledWith('MSFT')
  // the list refreshes to still contain MSFT (now with its quote); selecting
  // it opens the company
  await userEvent.click(await screen.findByRole('button', { name: /^MSFT/ }))
  await waitFor(() => expect(screen.getByRole('heading', { name: /msft inc/i })).toBeInTheDocument())
})

test('Watchlist tab manages groups: create, tag, untag, rename, delete', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getGroups.mockResolvedValue([
    { id: 1, name: 'Tech' },
    { id: 2, name: 'Div' },
  ])
  mocked.getWatchlistQuotes.mockResolvedValue([
    {
      company: company('MSFT'),
      last_close: 410,
      change: 4,
      change_pct: 0.01,
      volume: 10,
      as_of: '2024-03-01',
      group_ids: [1],
    },
  ])
  mocked.createGroup.mockResolvedValue()
  mocked.renameGroup.mockResolvedValue()
  mocked.deleteGroup.mockResolvedValue()
  mocked.tagWatch.mockResolvedValue()
  mocked.untagWatch.mockResolvedValue()

  render(<App />)
  await userEvent.click(await screen.findByRole('tab', { name: /watchlist/i }))
  // create a group
  await userEvent.type(await screen.findByLabelText('new group'), 'Growth')
  await userEvent.click(screen.getByRole('button', { name: 'Create' }))
  await waitFor(() => expect(mocked.createGroup).toHaveBeenCalledWith('Growth'))
  // tag MSFT into Div (it is only in Tech)
  await userEvent.click(screen.getByRole('button', { name: 'tag MSFT into Div' }))
  await waitFor(() => expect(mocked.tagWatch).toHaveBeenCalledWith('MSFT', 2))
  // untag MSFT from Tech (chip in the MSFT row)
  const msftRow = screen.getByRole('row', { name: /MSFT/ })
  const techChip = within(msftRow).getByText('Tech').closest('.MuiChip-root') as HTMLElement
  await userEvent.click(within(techChip).getByTestId('CancelIcon'))
  await waitFor(() => expect(mocked.untagWatch).toHaveBeenCalledWith('MSFT', 1))
  // rename + delete via the filter row
  const filters = screen.getByRole('group', { name: 'group filters' })
  await userEvent.click(within(filters).getByRole('button', { name: 'edit Tech' }))
  await userEvent.type(screen.getByLabelText('rename Tech'), '!{enter}')
  await waitFor(() => expect(mocked.renameGroup).toHaveBeenCalledWith(1, 'Tech!'))
  const divChip = within(filters).getByText('Div').closest('.MuiChip-root') as HTMLElement
  await userEvent.click(within(divChip).getByTestId('CancelIcon'))
  await waitFor(() => expect(mocked.deleteGroup).toHaveBeenCalledWith(2))
})

test('Screener nav lists ranked passers and a row opens the company', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.loadCompanyData.mockResolvedValue(data('KO'))

  render(<App />)
  await userEvent.click(await screen.findByRole('button', { name: /screener/i }))
  await waitFor(() => expect(screen.getByRole('button', { name: 'KO' })).toBeInTheDocument())
  await userEvent.click(screen.getByRole('button', { name: 'KO' }))
  await waitFor(() => expect(screen.getByRole('heading', { name: /ko inc/i })).toBeInTheDocument())
  // the Graham scorecard now lives on the Valuation & quality tab
  await userEvent.click(screen.getByRole('tab', { name: /valuation/i }))
  expect(await screen.findByText(/graham scorecard/i)).toBeInTheDocument()
})

test('Profile nav shows the account page and the header reflects the display name', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getMe.mockResolvedValue({ email: 'u@e.com', display_name: 'Uma' })
  mocked.updateProfile.mockResolvedValue()

  render(<App />)
  // header button shows the display name once loaded
  await screen.findByRole('button', { name: /uma/i })
  await userEvent.click(screen.getByRole('button', { name: /uma/i }))
  // the account page renders with the loaded email
  await waitFor(() => expect(screen.getByLabelText('profile email')).toHaveValue('u@e.com'))
  await userEvent.click(screen.getByRole('button', { name: /save profile/i }))
  await waitFor(() => expect(mocked.updateProfile).toHaveBeenCalled())
})

test('Compare navigates to the empty CompareView', async () => {
  mocked.getToken.mockReturnValue('tok')

  render(<App />)
  await screen.findByLabelText('search stocks') // home loaded
  await userEvent.click(screen.getByRole('button', { name: /compare/i }))
  expect(await screen.findByText(/add tickers above/i)).toBeInTheDocument()
})

test('select failure shows an error with a working retry', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.loadCompanyData.mockRejectedValueOnce(new Error('boom')).mockResolvedValue(data('AAPL'))

  render(<App />)
  await userEvent.click(await screen.findByRole('button', { name: 'AAPL' }))
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/failed to load aapl/i))
  await userEvent.click(screen.getByRole('button', { name: /retry/i }))
  await waitFor(() => expect(screen.getByRole('heading', { name: /aapl inc/i })).toBeInTheDocument())
})

test('company with no prices shows unknown freshness', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.loadCompanyData.mockResolvedValue({ ...data('AAPL'), prices: [] })

  render(<App />)
  await userEvent.click(await screen.findByRole('button', { name: 'AAPL' }))
  await waitFor(() => expect(screen.getByText(/unknown/i)).toBeInTheDocument())
})

test('Sectors nav shows sector rows and clicking top_ticker opens company', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.loadCompanyData.mockResolvedValue(data('AAPL'))

  render(<App />)
  await screen.findByLabelText('search stocks') // home loaded
  await userEvent.click(screen.getByRole('button', { name: /sectors/i }))
  await waitFor(() => expect(screen.getByText('Technology')).toBeInTheDocument())
  expect(screen.getByText('100%')).toBeInTheDocument() // pct_defensive = 1 → 100%
  await userEvent.click(screen.getByRole('button', { name: 'AAPL' }))
  await waitFor(() => expect(screen.getByRole('heading', { name: /aapl inc/i })).toBeInTheDocument())
})

test('Movers nav shows the three buckets and a row opens the company', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getMovers.mockResolvedValue({
    gainers: [
      { company: company('UP'), last_close: 110, change: 10, change_pct: 0.1, volume: 50, as_of: '2024-03-01' },
    ],
    losers: [],
    most_active: [],
  })
  mocked.loadCompanyData.mockResolvedValue(data('UP'))

  render(<App />)
  await screen.findByLabelText('search stocks') // home loaded
  await userEvent.click(screen.getByRole('button', { name: /movers/i }))
  expect(await screen.findByText('Top gainers')).toBeInTheDocument()
  await userEvent.click(screen.getByRole('button', { name: 'UP' }))
  await waitFor(() => expect(screen.getByRole('heading', { name: /up inc/i })).toBeInTheDocument())
})

test('theme follows the OS when preference is system and reacts to scheme changes', async () => {
  mocked.getToken.mockReturnValue('tok')
  let handler: (() => void) | undefined
  const mql = {
    matches: true,
    media: '',
    onchange: null,
    addEventListener: (_e: string, h: () => void) => {
      handler = h
    },
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  }
  const prev = globalThis.matchMedia
  globalThis.matchMedia = (() => mql) as unknown as typeof window.matchMedia
  try {
    render(<App />)
    await screen.findByLabelText('search stocks')
    expect(document.documentElement.dataset.theme).toBe('dark') // system + OS dark
    mql.matches = false
    handler?.()
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe('light'))
  } finally {
    globalThis.matchMedia = prev
  }
})

test('explicit dark preference from settings overrides the OS scheme', async () => {
  mocked.getToken.mockReturnValue('tok')
  localStorage.setItem('stonks_theme', 'dark')
  mocked.getSettings.mockResolvedValue({
    theme: 'dark',
    graham: { min_revenue: 5e8, pe_max: 15, pb_max: 1.5, pe_pb_max: 22.5, current_ratio_min: 2, eps_growth_min: 0.33 },
  })
  render(<App />)
  await screen.findByLabelText('search stocks')
  await waitFor(() => expect(document.documentElement.dataset.theme).toBe('dark'))
})

test('a failed settings load leaves the stored theme intact', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getSettings.mockRejectedValue(new Error('boom'))
  render(<App />)
  await screen.findByLabelText('search stocks')
  expect(document.documentElement.dataset.theme).toBe('light') // system -> light (matchMedia false)
})
