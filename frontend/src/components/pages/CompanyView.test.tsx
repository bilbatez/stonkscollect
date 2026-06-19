import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { CompanyView } from './CompanyView'
import * as api from '../../api'
import type { Company, CompanyData, FinancialFact, NewsItem, PeerRow, Ratio } from '../../types'

// Charts are lazy-loaded and coverage-excluded; stub them so the suite is
// deterministic and offline (no echarts in jsdom).
vi.mock('../../charts/PriceChart', () => ({ default: () => <div data-testid="price-chart" /> }))
vi.mock('../../charts/IncomeChart', () => ({ default: () => <div data-testid="income-chart" /> }))
vi.mock('../../charts/RatioChart', () => ({ default: () => <div data-testid="ratio-chart" /> }))
vi.mock('../../charts/GrahamChart', () => ({ default: () => <div data-testid="graham-chart" /> }))
vi.mock('../../api')

const mocked = vi.mocked(api)

const company = (ticker: string): Company => ({
  id: 1,
  cik: '0000320193',
  ticker,
  name: `${ticker} Inc.`,
  exchange: 'NASDAQ',
  sector: 'Technology',
  industry: 'Consumer Electronics',
  description: `${ticker} makes things.`,
  website: 'https://example.com',
})

const fact = (
  statement: string,
  item: string,
  period: string,
  value: number,
): FinancialFact => ({
  company_id: 1,
  statement,
  line_item: item,
  period_type: 'annual',
  period_end: period,
  value,
  source: 'edgar',
  fetched_at: '2024-01-01T00:00:00Z',
})

const ratio = (metric: string, value: number): Ratio => ({
  company_id: 1,
  period_end: '2023-12-31',
  period_type: 'annual',
  metric,
  value,
  computed_at: '',
})

const news: NewsItem[] = [
  {
    company_id: 1,
    title: 'Headline one',
    description: 'why',
    url: 'http://a',
    source: 'reuters',
    published_at: '2024-01-02T00:00:00Z',
    dedup_hash: 'h1',
  },
]

const peers: PeerRow[] = [
  {
    company: company('MSFT'),
    score: {
      company_id: 1,
      score: 5,
      passes_defensive: true,
      graham_number: 42.5,
      ncav_per_share: null,
      margin_of_safety: 0.2,
      net_net: false,
      computed_at: '',
    },
  },
]

function fixture(ticker = 'AAPL'): CompanyData {
  return {
    company: company(ticker),
    prices: [
      { company_id: 1, date: '2023-12-29', close: 100, volume: 1000, source: 'fmp' },
      { company_id: 1, date: '2024-01-02', close: 110, volume: 1200, source: 'fmp' },
    ],
    facts: [
      fact('income', 'Revenue', '2023-12-31', 2_000_000_000),
      fact('income', 'NetIncome', '2023-12-31', 500_000_000),
      fact('income', 'DividendPerShare', '2023-12-31', 0.94),
    ],
    ratios: [ratio('roe', 0.18), ratio('pe', 15.4), ratio('current_ratio', 2.3)],
    news,
    discrepancies: [],
    graham: {
      criteria: [{ name: 'Current ratio >= 2', passed: true, detail: 'current ratio 2.5' }],
      score: 1,
      graham_number: 22.4,
      ncav_per_share: null,
      margin_of_safety: 0.1,
      net_net: false,
      passes_defensive: true,
    },
    peers,
    note: { body: null },
    shares: null,
  }
}

const LOADED_AT = Date.parse('2024-01-03T00:00:00Z')

beforeEach(() => {
  mocked.getHolders.mockResolvedValue([
    {
      company_id: 1,
      holder: 'Jane Insider',
      kind: 'insider',
      shares: 12_345,
      as_of: '2024-01-01T00:00:00Z',
      source: 'edgar',
    },
  ])
  mocked.saveNote.mockResolvedValue()
  mocked.deleteNote.mockResolvedValue()
})
afterEach(() => vi.clearAllMocks())

test('renders the identity header with company name and ticker', () => {
  render(<CompanyView data={fixture('AAPL')} loadedAt={LOADED_AT} />)
  expect(screen.getByRole('heading', { name: /aapl inc\. \(aapl\)/i })).toBeInTheDocument()
  // identity chips + reference links live in the always-visible header
  expect(screen.getByText('Technology')).toBeInTheDocument()
  expect(screen.getByText(/aapl makes things/i)).toBeInTheDocument()
  expect(screen.getByRole('link', { name: /sec filings/i })).toBeInTheDocument()
  expect(screen.getByRole('link', { name: /website/i })).toBeInTheDocument()
})

test('Overview tab (default) shows key statistics', async () => {
  render(<CompanyView data={fixture()} loadedAt={LOADED_AT} />)
  // Overview is the default tab
  expect(await screen.findByText('Key statistics')).toBeInTheDocument()
  expect(screen.getByText('Price')).toBeInTheDocument()
  expect(await screen.findByTestId('price-chart')).toBeInTheDocument()
})

test('Financials tab shows the Statements section', async () => {
  render(<CompanyView data={fixture()} loadedAt={LOADED_AT} />)
  await userEvent.click(screen.getByRole('tab', { name: /financials/i }))
  expect(await screen.findByText('Statements')).toBeInTheDocument()
  expect(screen.getByText('Income')).toBeInTheDocument()
  expect(screen.getByText('Dividends')).toBeInTheDocument()
  // identity header still present after switching tabs
  expect(screen.getByRole('heading', { name: /aapl inc\. \(aapl\)/i })).toBeInTheDocument()
})

test('Valuation & quality tab shows the Graham scorecard', async () => {
  render(<CompanyView data={fixture()} loadedAt={LOADED_AT} />)
  await userEvent.click(screen.getByRole('tab', { name: /valuation/i }))
  expect(await screen.findByText(/graham scorecard/i)).toBeInTheDocument()
  expect(screen.getByText('Ratios')).toBeInTheDocument()
  expect(screen.getByRole('heading', { name: /aapl inc\. \(aapl\)/i })).toBeInTheDocument()
})

test('Ownership & news tab shows holders and news', async () => {
  render(<CompanyView data={fixture()} loadedAt={LOADED_AT} />)
  await userEvent.click(screen.getByRole('tab', { name: /ownership & news/i }))
  expect(await screen.findByText('Holders')).toBeInTheDocument()
  // HoldersPanel resolves its fetch and renders the row
  expect(await screen.findByText('Jane Insider')).toBeInTheDocument()
  expect(screen.getByText('Headline one')).toBeInTheDocument() // News
  expect(screen.getByText('Peers')).toBeInTheDocument()
  expect(screen.getByRole('heading', { name: /aapl inc\. \(aapl\)/i })).toBeInTheDocument()
})

/** True when at least one rendered 6M range toggle is currently selected. */
function sixMonthIsSelected() {
  return screen
    .getAllByRole('button', { name: '6M' })
    .some((b) => b.getAttribute('aria-pressed') === 'true')
}

test('price range selection persists across a tab round-trip', async () => {
  render(<CompanyView data={fixture()} loadedAt={LOADED_AT} />)
  // Overview tab: change the price range from the default 1Y to 6M
  const [range] = await screen.findAllByRole('group', { name: /price range/i })
  await userEvent.click(within(range).getByRole('button', { name: '6M' }))
  await waitFor(() => expect(sixMonthIsSelected()).toBe(true))

  // switch to another tab and back (top-level priceRange state must survive)
  await userEvent.click(screen.getByRole('tab', { name: /financials/i }))
  await userEvent.click(screen.getByRole('tab', { name: /overview/i }))

  await waitFor(() => expect(sixMonthIsSelected()).toBe(true))
})

/** True when at least one rendered Quarterly toggle is currently selected. */
function quarterlyIsSelected() {
  return screen
    .getAllByRole('button', { name: /quarterly/i })
    .some((b) => b.getAttribute('aria-pressed') === 'true')
}

test('chart period selection persists across a tab round-trip', async () => {
  render(<CompanyView data={fixture()} loadedAt={LOADED_AT} />)
  // Financials tab hosts the period toggle (annual default -> quarterly)
  await userEvent.click(screen.getByRole('tab', { name: /financials/i }))
  const [group] = await screen.findAllByRole('group', { name: /period/i })
  await userEvent.click(within(group).getByRole('button', { name: /quarterly/i }))
  await waitFor(() => expect(quarterlyIsSelected()).toBe(true))

  // round-trip through another tab; chartPeriod state must persist
  await userEvent.click(screen.getByRole('tab', { name: /valuation/i }))
  await userEvent.click(screen.getByRole('tab', { name: /financials/i }))

  await waitFor(() => expect(quarterlyIsSelected()).toBe(true))
})
