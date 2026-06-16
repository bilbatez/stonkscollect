import { render, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import { MarketSummary } from './panels/MarketSummary'
import * as api from '../api'
import type { Company, MoverRow } from '../types'

vi.mock('../api')
afterEach(() => vi.clearAllMocks())

function company(ticker: string, name: string): Company {
  return { id: 1, cik: '', ticker, name, exchange: null, sector: null, industry: null, description: null, website: null }
}

function idx(ticker: string, name: string, last: number, pct: number): MoverRow {
  return { company: company(ticker, name), last_close: last, change: pct * last, change_pct: pct, volume: null, as_of: '2024-03-01' }
}

test('renders a card per index with name, close, and colored change', async () => {
  vi.mocked(api.getMarketSummary).mockResolvedValue([
    idx('^GSPC', 'S&P 500', 4200, 0.01),
    idx('^IXIC', 'Nasdaq Composite', 13000, -0.02),
  ])
  render(<MarketSummary />)
  expect(await screen.findByText('S&P 500')).toBeInTheDocument()
  expect(screen.getByText('Nasdaq Composite')).toBeInTheDocument()
  expect(screen.getByText('4200.00')).toBeInTheDocument()
  expect(screen.getByText('+1%')).toBeInTheDocument()
  expect(screen.getByText('-2%')).toBeInTheDocument()
})

test('renders nothing when there are no indices', async () => {
  vi.mocked(api.getMarketSummary).mockResolvedValue([])
  const { container } = render(<MarketSummary />)
  await waitFor(() => expect(api.getMarketSummary).toHaveBeenCalled())
  expect(container.firstChild).toBeNull()
})

test('stays silent when the summary fails to load', async () => {
  vi.mocked(api.getMarketSummary).mockRejectedValue(new Error('down'))
  const { container } = render(<MarketSummary />)
  await waitFor(() => expect(api.getMarketSummary).toHaveBeenCalled())
  expect(container.firstChild).toBeNull()
})
