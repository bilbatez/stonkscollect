import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App'
import * as api from './api'
import type { Company, CompanyData, GrahamScore } from './types'

vi.mock('./charts/PriceChart', () => ({ default: () => <div data-testid="price-chart" /> }))
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
  }
}

beforeEach(() => {
  mocked.getToken.mockReturnValue(null)
  mocked.getWatchlist.mockResolvedValue([])
  mocked.listCompanies.mockResolvedValue({ rows: [{ company: company('AAPL'), score: grahamScore() }], total: 1 })
  mocked.screen.mockResolvedValue({ rows: [{ company: company('KO'), score: grahamScore() }], total: 1 })
})
afterEach(() => vi.clearAllMocks())

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

test('home All Stocks tab opens a company; theme toggles; logout returns to auth', async () => {
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

  // dark is the default; toggle offers Light first
  expect(document.documentElement.dataset.theme).toBe('dark')
  await userEvent.click(screen.getByRole('button', { name: /light/i }))
  expect(document.documentElement.dataset.theme).toBe('light')
  await userEvent.click(screen.getByRole('button', { name: /dark/i }))
  expect(document.documentElement.dataset.theme).toBe('dark')

  // back home, then logout
  await userEvent.click(screen.getByRole('button', { name: /home/i }))
  await userEvent.click(screen.getByRole('button', { name: /log out/i }))
  expect(screen.getByLabelText('email')).toBeInTheDocument()
})

test('Watchlist tab adds and removes tickers', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getWatchlist.mockResolvedValueOnce([]).mockResolvedValue([company('MSFT')])
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
  // the list refreshes to still contain MSFT; selecting it opens the company
  await userEvent.click(await screen.findByRole('button', { name: 'MSFT' }))
  await waitFor(() => expect(screen.getByRole('heading', { name: /msft inc/i })).toBeInTheDocument())
})

test('Screener nav lists ranked passers and a row opens the company', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.loadCompanyData.mockResolvedValue(data('KO'))

  render(<App />)
  await userEvent.click(await screen.findByRole('button', { name: /screener/i }))
  await waitFor(() => expect(screen.getByRole('button', { name: 'KO' })).toBeInTheDocument())
  await userEvent.click(screen.getByRole('button', { name: 'KO' }))
  await waitFor(() => expect(screen.getByText(/graham scorecard/i)).toBeInTheDocument())
})

test('Compare builds a matrix from the watchlist (annual ratios only)', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getWatchlist.mockResolvedValue([company('AAPL'), company('MSFT')])
  // MSFT load fails -> allSettled keeps AAPL; compare must not hang
  mocked.loadCompanyData.mockImplementation(async (t: string) => {
    if (t === 'MSFT') {
      throw new Error('boom')
    }
    return data(t)
  })

  render(<App />)
  await screen.findByLabelText('search stocks') // home loaded
  await userEvent.click(screen.getByRole('button', { name: /compare/i }))
  await waitFor(() => expect(screen.getByText('Return on equity')).toBeInTheDocument())
  // annual roe 1.5 -> 150.0%; quarterly 0.4 filtered out
  expect(screen.getAllByText('150.0%').length).toBeGreaterThan(0)
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
