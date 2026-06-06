import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import App from './App'
import { loadCompanyData } from './api'
import type { CompanyData } from './types'

// Chart wrapper renders to canvas (echarts); stub it in component tests.
vi.mock('./charts/PriceChart', () => ({
  default: () => <div data-testid="price-chart" />,
}))
vi.mock('./api')

const mockedLoad = vi.mocked(loadCompanyData)

afterEach(() => {
  vi.clearAllMocks()
})

function sampleData(overrides: Partial<CompanyData> = {}): CompanyData {
  return {
    company: { id: 1, cik: '', ticker: 'AAPL', name: 'Apple Inc.', exchange: 'NASDAQ', sector: null, industry: null },
    prices: [{ company_id: 1, date: '2024-01-02', close: 185, volume: 1, source: 'fmp' }],
    facts: [],
    ratios: [],
    news: [],
    discrepancies: [],
    ...overrides,
  }
}

test('renders the application title in the idle state', () => {
  render(<App />)
  expect(screen.getByRole('heading', { name: /stonkscollect/i })).toBeInTheDocument()
  expect(screen.queryByText(/loading/i)).not.toBeInTheDocument()
})

test('loads and renders the dashboard on submit', async () => {
  mockedLoad.mockResolvedValue(sampleData())
  render(<App />)
  await userEvent.type(screen.getByLabelText('ticker'), 'AAPL')
  await userEvent.click(screen.getByRole('button', { name: /load/i }))

  await waitFor(() =>
    expect(screen.getByRole('heading', { name: /apple inc\./i })).toBeInTheDocument(),
  )
  // chart is lazy-loaded
  expect(await screen.findByTestId('price-chart')).toBeInTheDocument()
  expect(mockedLoad).toHaveBeenCalledWith('AAPL')
})

test('renders unknown freshness when there are no prices', async () => {
  mockedLoad.mockResolvedValue(sampleData({ prices: [] }))
  render(<App />)
  await userEvent.type(screen.getByLabelText('ticker'), 'AAPL')
  await userEvent.click(screen.getByRole('button', { name: /load/i }))
  await waitFor(() => expect(screen.getByText(/unknown/i)).toBeInTheDocument())
})

test('shows an Error message when loading fails', async () => {
  mockedLoad.mockRejectedValue(new Error('request failed: 404'))
  render(<App />)
  await userEvent.type(screen.getByLabelText('ticker'), 'NOPE')
  await userEvent.click(screen.getByRole('button', { name: /load/i }))
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/404/))
})

test('falls back to a generic message for non-Error rejections', async () => {
  mockedLoad.mockRejectedValue('boom')
  render(<App />)
  await userEvent.type(screen.getByLabelText('ticker'), 'X')
  await userEvent.click(screen.getByRole('button', { name: /load/i }))
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/failed to load/i))
})
