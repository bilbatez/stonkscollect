import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App'
import * as api from './api'
import type { Company, CompanyData } from './types'

vi.mock('./charts/PriceChart', () => ({ default: () => <div data-testid="price-chart" /> }))
vi.mock('./api')

const mocked = vi.mocked(api)

const company = (ticker: string): Company => ({
  id: 1, cik: '', ticker, name: `${ticker} Inc.`, exchange: 'NASDAQ', sector: null, industry: null,
})

function data(ticker: string): CompanyData {
  return {
    company: company(ticker),
    prices: [{ company_id: 1, date: '2024-01-02', close: 1, volume: null, source: 'fmp' }],
    facts: [],
    ratios: [{ company_id: 1, period_end: '2023-12-31', metric: 'roe', value: 1.5, computed_at: '' }],
    news: [],
    discrepancies: [],
  }
}

beforeEach(() => {
  mocked.getToken.mockReturnValue(null)
  mocked.getWatchlist.mockResolvedValue([])
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
  await waitFor(() => expect(screen.getByText(/select a ticker/i)).toBeInTheDocument())
})

test('dashboard loads watchlist, selects a company, toggles theme, logs out', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getWatchlist.mockResolvedValue([company('AAPL')])
  mocked.loadCompanyData.mockResolvedValue(data('AAPL'))
  mocked.logout.mockResolvedValue()

  render(<App />)
  // watchlist rendered
  await waitFor(() => expect(screen.getByRole('button', { name: 'AAPL' })).toBeInTheDocument())
  // select -> company view
  await userEvent.click(screen.getByRole('button', { name: 'AAPL' }))
  await waitFor(() => expect(screen.getByRole('heading', { name: /aapl inc/i })).toBeInTheDocument())
  expect(await screen.findByTestId('price-chart')).toBeInTheDocument()
  // theme toggle flips the document attribute both ways
  await userEvent.click(screen.getByRole('button', { name: /dark/i }))
  expect(document.documentElement.dataset.theme).toBe('dark')
  await userEvent.click(screen.getByRole('button', { name: /light/i }))
  expect(document.documentElement.dataset.theme).toBe('light')
  // logout returns to auth form
  await userEvent.click(screen.getByRole('button', { name: /log out/i }))
  expect(screen.getByLabelText('email')).toBeInTheDocument()
})

test('add and remove watchlist tickers refresh the list', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getWatchlist.mockResolvedValueOnce([]).mockResolvedValue([company('MSFT')])
  mocked.addWatch.mockResolvedValue()
  mocked.removeWatch.mockResolvedValue()

  render(<App />)
  await waitFor(() => expect(screen.getByText(/no tickers yet/i)).toBeInTheDocument())
  await userEvent.type(screen.getByLabelText('add ticker'), 'msft')
  await userEvent.click(screen.getByRole('button', { name: 'Add' }))
  await waitFor(() => expect(mocked.addWatch).toHaveBeenCalledWith('MSFT'))
  await waitFor(() => expect(screen.getByRole('button', { name: 'MSFT' })).toBeInTheDocument())
  await userEvent.click(screen.getByRole('button', { name: 'remove MSFT' }))
  expect(mocked.removeWatch).toHaveBeenCalledWith('MSFT')
})

test('select failure shows an error with working retry', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getWatchlist.mockResolvedValue([company('AAPL')])
  mocked.loadCompanyData.mockRejectedValueOnce(new Error('boom')).mockResolvedValue(data('AAPL'))

  render(<App />)
  await userEvent.click(await screen.findByRole('button', { name: 'AAPL' }))
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/failed to load aapl/i))
  await userEvent.click(screen.getByRole('button', { name: /retry/i }))
  await waitFor(() => expect(screen.getByRole('heading', { name: /aapl inc/i })).toBeInTheDocument())
})

test('company with no prices shows unknown freshness', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getWatchlist.mockResolvedValue([company('AAPL')])
  mocked.loadCompanyData.mockResolvedValue({ ...data('AAPL'), prices: [] })

  render(<App />)
  await userEvent.click(await screen.findByRole('button', { name: 'AAPL' }))
  await waitFor(() => expect(screen.getByText(/unknown/i)).toBeInTheDocument())
})

test('compare builds a matrix across the watchlist', async () => {
  mocked.getToken.mockReturnValue('tok')
  mocked.getWatchlist.mockResolvedValue([company('AAPL'), company('MSFT')])
  mocked.loadCompanyData.mockImplementation(async (t: string) => data(t))

  render(<App />)
  await screen.findByRole('button', { name: 'AAPL' })
  await userEvent.click(screen.getByRole('button', { name: /compare/i }))
  await waitFor(() => expect(screen.getByText('roe')).toBeInTheDocument())
  // both tickers appear as rows in the compare table
  expect(screen.getAllByText(/1\.50/).length).toBeGreaterThan(0)
})
