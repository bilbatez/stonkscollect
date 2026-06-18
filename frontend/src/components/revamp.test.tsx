import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import { AllStocks } from './pages/AllStocks'
import { AuthForm } from './auth/AuthForm'
import { Compare } from './shared/Compare'
import { GrahamScorecard } from './panels/GrahamScorecard'
import { Screener } from './pages/Screener'
import { Skeleton } from './shared/Skeleton'
import { ThemeToggle } from './shared/ThemeToggle'
import { Watchlist } from './layout/Watchlist'
import * as api from '../api'
import type { Company, GrahamAssessment, GrahamScore, WatchQuote } from '../types'

vi.mock('../api')

afterEach(() => vi.clearAllMocks())

const company = (ticker: string, industry: string | null = null): Company => ({
  id: 1,
  cik: '',
  ticker,
  name: ticker,
  exchange: null,
  sector: null,
  industry,
  description: null,
  website: null,
})

const score = (overrides: Partial<GrahamScore> = {}): GrahamScore => ({
  company_id: 1,
  score: 6,
  passes_defensive: false,
  graham_number: 60,
  ncav_per_share: null,
  margin_of_safety: 0.3,
  net_net: false,
  computed_at: '',
  ...overrides,
})

test('AuthForm logs in and reports the token', async () => {
  vi.mocked(api.login).mockResolvedValue('tok')
  const onAuth = vi.fn()
  render(<AuthForm onAuth={onAuth} />)
  await userEvent.type(screen.getByLabelText('email'), 'a@e.com')
  await userEvent.type(screen.getByLabelText('password'), 'pw')
  await userEvent.click(screen.getByRole('button', { name: /log in/i }))
  await waitFor(() => expect(onAuth).toHaveBeenCalledWith('tok'))
})

test('AuthForm surfaces a login error', async () => {
  vi.mocked(api.login).mockRejectedValue(new Error('nope'))
  render(<AuthForm onAuth={vi.fn()} />)
  await userEvent.type(screen.getByLabelText('email'), 'a@e.com')
  await userEvent.type(screen.getByLabelText('password'), 'pw')
  await userEvent.click(screen.getByRole('button', { name: /log in/i }))
  expect(await screen.findByRole('alert')).toHaveTextContent(/nope/i)
})

test('AuthForm can switch to signup and back, and surfaces signup errors', async () => {
  vi.mocked(api.signup).mockRejectedValue(new Error('nope'))
  const onAuth = vi.fn()
  render(<AuthForm onAuth={onAuth} />)
  await userEvent.click(screen.getByRole('button', { name: /need an account/i }))
  await userEvent.type(screen.getByLabelText('email'), 'a@e.com')
  await userEvent.type(screen.getByLabelText('password'), 'pw')
  await userEvent.click(screen.getByRole('button', { name: /sign up/i }))
  expect(await screen.findByRole('alert')).toHaveTextContent(/nope/i)
  expect(onAuth).not.toHaveBeenCalled()
  await userEvent.click(screen.getByRole('button', { name: /have an account/i }))
  expect(screen.getByRole('button', { name: /log in/i })).toBeInTheDocument()
})

test('AuthForm shows fallback message for non-Error rejection', async () => {
  vi.mocked(api.login).mockRejectedValue('plain string')
  render(<AuthForm onAuth={vi.fn()} />)
  await userEvent.type(screen.getByLabelText('email'), 'a@e.com')
  await userEvent.type(screen.getByLabelText('password'), 'pw')
  await userEvent.click(screen.getByRole('button', { name: /log in/i }))
  expect(await screen.findByRole('alert')).toHaveTextContent(/request failed/i)
})

test('AuthForm signup success reports the token', async () => {
  vi.mocked(api.signup).mockResolvedValue('newtok')
  const onAuth = vi.fn()
  render(<AuthForm onAuth={onAuth} />)
  await userEvent.click(screen.getByRole('button', { name: /need an account/i }))
  await userEvent.type(screen.getByLabelText('email'), 'a@e.com')
  await userEvent.type(screen.getByLabelText('password'), 'pw')
  await userEvent.click(screen.getByRole('button', { name: /sign up/i }))
  await waitFor(() => expect(onAuth).toHaveBeenCalledWith('newtok'))
})

const watchQuote = (ticker: string, overrides: Partial<WatchQuote> = {}): WatchQuote => ({
  company: company(ticker),
  last_close: 110,
  change: 10,
  change_pct: 0.1,
  volume: null,
  as_of: '2024-03-01',
  ...overrides,
})

test('Watchlist selects, adds (trimmed/upper), ignores blanks, removes', async () => {
  const onSelect = vi.fn()
  const onAdd = vi.fn()
  const onRemove = vi.fn()
  render(
    <Watchlist
      items={[watchQuote('AAPL')]}
      onSelect={onSelect}
      onAdd={onAdd}
      onRemove={onRemove}
    />,
  )
  await userEvent.click(screen.getByRole('button', { name: /^AAPL/ }))
  expect(onSelect).toHaveBeenCalledWith('AAPL')
  await userEvent.click(screen.getByRole('button', { name: 'remove AAPL' }))
  expect(onRemove).toHaveBeenCalledWith('AAPL')
  await userEvent.type(screen.getByLabelText('add ticker'), ' msft ')
  await userEvent.click(screen.getByRole('button', { name: 'Add' }))
  expect(onAdd).toHaveBeenCalledWith('MSFT')
  await userEvent.click(screen.getByRole('button', { name: 'Add' }))
  expect(onAdd).toHaveBeenCalledTimes(1)
})

test('Watchlist rows show the last price and a colored day change', () => {
  render(
    <Watchlist
      items={[
        watchQuote('UP'),
        watchQuote('DOWN', { last_close: 90, change: -10, change_pct: -0.1 }),
        watchQuote('BARE', { last_close: null, change: null, change_pct: null, as_of: null }),
      ]}
      onSelect={vi.fn()}
      onAdd={vi.fn()}
      onRemove={vi.fn()}
    />,
  )
  expect(screen.getByText('110.00')).toBeInTheDocument()
  expect(screen.getByText('+10%')).toBeInTheDocument()
  expect(screen.getByText('-10%')).toBeInTheDocument()
  // an unpriced company still lists, with a dash and no change chip
  expect(screen.getByText('BARE')).toBeInTheDocument()
  expect(screen.getByText('—')).toBeInTheDocument()
})

test('Watchlist shows empty state', () => {
  render(<Watchlist items={[]} onSelect={vi.fn()} onAdd={vi.fn()} onRemove={vi.fn()} />)
  expect(screen.getByText(/no tickers yet/i)).toBeInTheDocument()
})

test('ThemeToggle shows the opposite theme and toggles', async () => {
  const onToggle = vi.fn()
  const { rerender } = render(<ThemeToggle theme="light" onToggle={onToggle} />)
  const btn = screen.getByRole('button')
  expect(btn).toHaveTextContent(/dark/i)
  await userEvent.click(btn)
  expect(onToggle).toHaveBeenCalled()
  rerender(<ThemeToggle theme="dark" onToggle={onToggle} />)
  expect(screen.getByRole('button')).toHaveTextContent(/light/i)
})

test('Skeleton renders a status label', () => {
  render(<Skeleton label="Loading prices" />)
  expect(screen.getByRole('status')).toHaveTextContent('Loading prices')
  render(<Skeleton />)
  expect(screen.getAllByRole('status')[1]).toHaveTextContent('Loading…')
})

test('GrahamScorecard renders criteria, valuation, net-net, and the price-gap hint', () => {
  const assessment: GrahamAssessment = {
    criteria: [
      { name: 'Current ratio >= 2', passed: true, detail: 'current ratio 2.5' },
      { name: 'P/E <= 15', passed: false, detail: 'insufficient data' },
    ],
    score: 1,
    graham_number: 22.4,
    ncav_per_share: 5,
    margin_of_safety: null,
    net_net: true,
    passes_defensive: false,
  }
  render(<GrahamScorecard assessment={assessment} />)
  expect(screen.getByText(/Current ratio >= 2/)).toBeInTheDocument()
  expect(screen.getByText('22.40')).toBeInTheDocument()
  expect(screen.getByText('net-net')).toBeInTheDocument()
  expect(screen.getByText('1/2')).toBeInTheDocument()
  expect(screen.getByText('needs price data')).toBeInTheDocument() // price-dependent P/E
  render(<GrahamScorecard assessment={{ ...assessment, graham_number: null, margin_of_safety: 0.2 }} />)
  expect(screen.getAllByText('—').length).toBeGreaterThan(0)
})

test('Screener lists ranked rows, filters, paginates, and selects', async () => {
  vi.mocked(api.screen).mockResolvedValue({
    rows: [
      { company: company('KO'), score: score() },
      // null graham#/margin + net-net to exercise the dash + ✓ branches
      { company: company('JNJ'), score: score({ score: 5, graham_number: null, margin_of_safety: null, net_net: true }) },
    ],
    total: 50,
  })
  const onSelect = vi.fn()
  render(<Screener onSelect={onSelect} />)
  await screen.findByText('60.00') // graham number
  expect(screen.getByText('30%')).toBeInTheDocument() // margin of safety
  expect(screen.getByText('6/8')).toBeInTheDocument()
  expect(screen.getByText('✓')).toBeInTheDocument() // JNJ net-net
  expect(screen.getAllByText('—').length).toBeGreaterThan(0) // JNJ null graham#/margin
  // sort each column (exercises the column sort accessors)
  await userEvent.click(screen.getByText('Ticker'))
  await userEvent.click(screen.getByText('Score'))
  await userEvent.click(screen.getByText('Graham #'))
  await userEvent.click(screen.getByText('Margin of safety'))
  await userEvent.click(within(screen.getByRole('columnheader', { name: /Net-net/ })).getByText('Net-net'))
  await userEvent.click(screen.getByRole('button', { name: 'KO' }))
  expect(onSelect).toHaveBeenCalledWith('KO')
  await userEvent.click(screen.getByRole('button', { name: /next page/i }))
  await waitFor(() =>
    expect(vi.mocked(api.screen)).toHaveBeenCalledWith(expect.objectContaining({ offset: 25 })),
  )
  await userEvent.click(screen.getByLabelText('Defensive only'))
  await waitFor(() =>
    expect(vi.mocked(api.screen)).toHaveBeenCalledWith(expect.objectContaining({ defensive: true })),
  )
  await userEvent.click(screen.getByLabelText('Net-net'))
  await waitFor(() =>
    expect(vi.mocked(api.screen)).toHaveBeenCalledWith(expect.objectContaining({ net_net: true })),
  )
})

test('Screener shows an empty state when nothing matches', async () => {
  vi.mocked(api.screen).mockResolvedValue({ rows: [], total: 0 })
  render(<Screener onSelect={vi.fn()} />)
  expect(await screen.findByText(/no matches/i)).toBeInTheDocument()
})

test('Screener sector filter sends sector param', async () => {
  vi.mocked(api.screen).mockResolvedValue({ rows: [], total: 0 })
  render(<Screener onSelect={vi.fn()} />)
  await screen.findByText(/no matches/i)
  await userEvent.type(screen.getByLabelText('Sector'), 'Technology')
  await waitFor(() =>
    expect(vi.mocked(api.screen)).toHaveBeenCalledWith(expect.objectContaining({ sector: 'Technology' })),
  )
})

test('Screener ratio filter fields render and send params', async () => {
  vi.mocked(api.screen).mockResolvedValue({ rows: [], total: 0 })
  render(<Screener onSelect={vi.fn()} />)
  await screen.findByText(/no matches/i)
  await userEvent.type(screen.getByLabelText('Min P/E'), '10')
  await waitFor(() =>
    expect(vi.mocked(api.screen)).toHaveBeenCalledWith(expect.objectContaining({ min_pe: 10 })),
  )
  await userEvent.type(screen.getByLabelText('Max P/E'), '20')
  await waitFor(() =>
    expect(vi.mocked(api.screen)).toHaveBeenCalledWith(expect.objectContaining({ max_pe: 20 })),
  )
  await userEvent.type(screen.getByLabelText('Min ROE'), '0.1')
  await waitFor(() =>
    expect(vi.mocked(api.screen)).toHaveBeenCalledWith(expect.objectContaining({ min_roe: 0.1 })),
  )
  await userEvent.type(screen.getByLabelText('Max D/E'), '0.5')
  await waitFor(() =>
    expect(vi.mocked(api.screen)).toHaveBeenCalledWith(expect.objectContaining({ max_de: 0.5 })),
  )
  await userEvent.type(screen.getByLabelText('Min margin'), '0.05')
  await waitFor(() =>
    expect(vi.mocked(api.screen)).toHaveBeenCalledWith(expect.objectContaining({ min_margin: 0.05 })),
  )
})

test('AllStocks lists, paginates, searches, selects and watches', async () => {
  vi.mocked(api.listCompanies).mockResolvedValue({
    rows: [
      { company: company('AAPL', 'Software'), score: score() },
      { company: company('ZZZ'), score: null }, // null industry + null score
    ],
    total: 50,
  })
  const onSelect = vi.fn()
  const onAdd = vi.fn()
  render(<AllStocks onSelect={onSelect} onAdd={onAdd} />)
  await screen.findByText('6/8')
  expect(screen.getByText('Software')).toBeInTheDocument() // industry column
  expect(screen.getAllByText('—').length).toBeGreaterThan(0) // ZZZ null score + null industry
  // sort each sortable column (exercises the sort accessors, incl. nulls)
  await userEvent.click(screen.getByText('Ticker'))
  await userEvent.click(screen.getByText('Name'))
  await userEvent.click(screen.getByText('Industry'))
  await userEvent.click(screen.getByText('Graham score'))
  await userEvent.click(screen.getByRole('button', { name: 'AAPL' }))
  expect(onSelect).toHaveBeenCalledWith('AAPL')
  await userEvent.click(screen.getByRole('button', { name: 'watch ZZZ' }))
  expect(onAdd).toHaveBeenCalledWith('ZZZ')
  await userEvent.click(screen.getByRole('button', { name: /next page/i }))
  // numeric column → TanStack sorts desc first; page offset = 25
  await waitFor(() =>
    expect(vi.mocked(api.listCompanies)).toHaveBeenCalledWith('', {}, 'score', 'desc', 25, 25),
  )
  await userEvent.type(screen.getByLabelText('search stocks'), 'a')
  await waitFor(() =>
    expect(vi.mocked(api.listCompanies)).toHaveBeenCalledWith('a', {}, 'score', 'desc', 25, 0),
  )
  // second click on same column toggles to asc
  await userEvent.click(screen.getByText('Graham score'))
  await waitFor(() =>
    expect(vi.mocked(api.listCompanies)).toHaveBeenCalledWith('a', {}, 'score', 'asc', 25, 0),
  )
})

test('AllStocks pushes a per-column filter to the backend and resets the page', async () => {
  vi.mocked(api.listCompanies).mockResolvedValue({
    rows: [{ company: company('AAPL', 'Software'), score: score() }],
    total: 50,
  })
  render(<AllStocks onSelect={vi.fn()} onAdd={vi.fn()} />)
  await screen.findByText('6/8')
  // move to page 2 first so we can assert the filter resets to page 0
  await userEvent.click(screen.getByRole('button', { name: /next page/i }))
  await waitFor(() =>
    expect(vi.mocked(api.listCompanies)).toHaveBeenCalledWith('', {}, null, 'asc', 25, 25),
  )
  vi.mocked(api.listCompanies).mockClear()
  // typing in the Industry column filter triggers a server refetch with the
  // industry param and offset reset to 0 (page 0)
  await userEvent.type(screen.getByLabelText('filter industry'), 'Soft')
  await waitFor(() =>
    expect(vi.mocked(api.listCompanies)).toHaveBeenCalledWith(
      '',
      { industry: 'Soft' },
      null,
      'asc',
      25,
      0,
    ),
  )
})

test('Compare builds a metric matrix and dashes missing cells', () => {
  const { rerender } = render(
    <Compare
      rows={[
        { ticker: 'AAPL', metrics: { roe: 1.5, net_margin: 0.25 } },
        { ticker: 'MSFT', metrics: { roe: 0.4 } },
      ]}
    />,
  )
  expect(screen.getByText('150.0%')).toBeInTheDocument() // roe formatted as percent
  expect(screen.getByText('—')).toBeInTheDocument()
  rerender(<Compare rows={[]} />)
  expect(screen.getByText(/nothing to compare/i)).toBeInTheDocument()
})

test('AllStocks surfaces a fetch error', async () => {
  vi.mocked(api.listCompanies).mockRejectedValue(new Error('network down'))
  render(<AllStocks onSelect={vi.fn()} onAdd={vi.fn()} />)
  expect(await screen.findByText('network down')).toBeInTheDocument()
})

test('Screener surfaces a fetch error', async () => {
  vi.mocked(api.screen).mockRejectedValue(new Error('screen failed'))
  render(<Screener onSelect={vi.fn()} />)
  expect(await screen.findByText('screen failed')).toBeInTheDocument()
})
