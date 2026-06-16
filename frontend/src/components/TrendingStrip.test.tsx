import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import { TrendingStrip } from './panels/TrendingStrip'
import * as api from '../api'
import type { Company, MoverRow, Movers } from '../types'

vi.mock('../api')
afterEach(() => vi.clearAllMocks())

function company(ticker: string): Company {
  return { id: 1, cik: '', ticker, name: ticker, exchange: null, sector: null, industry: null, description: null, website: null }
}
function row(ticker: string, pct: number): MoverRow {
  return { company: company(ticker), last_close: 10, change: pct * 10, change_pct: pct, volume: null, as_of: '2024-03-01' }
}
function movers(p: Partial<Movers>): Movers {
  return { gainers: [], losers: [], most_active: [], ...p }
}

test('renders gainer and loser chips and selects a ticker on click', async () => {
  vi.mocked(api.getMovers).mockResolvedValue(movers({ gainers: [row('UP', 0.05)], losers: [row('DN', -0.04)] }))
  const onSelect = vi.fn()
  render(<TrendingStrip onSelect={onSelect} />)
  expect(await screen.findByText('UP +5%')).toBeInTheDocument()
  expect(screen.getByText('DN -4%')).toBeInTheDocument()
  await userEvent.click(screen.getByText('UP +5%'))
  expect(onSelect).toHaveBeenCalledWith('UP')
})

test('omits the losers group when there are none', async () => {
  vi.mocked(api.getMovers).mockResolvedValue(movers({ gainers: [row('UP', 0.05)] }))
  render(<TrendingStrip onSelect={vi.fn()} />)
  expect(await screen.findByText('UP +5%')).toBeInTheDocument()
  expect(screen.queryByText('Losers')).toBeNull()
})

test('renders nothing when there are no movers', async () => {
  vi.mocked(api.getMovers).mockResolvedValue(movers({}))
  const { container } = render(<TrendingStrip onSelect={vi.fn()} />)
  await waitFor(() => expect(api.getMovers).toHaveBeenCalled())
  expect(container.firstChild).toBeNull()
})

test('stays silent when movers fail to load', async () => {
  vi.mocked(api.getMovers).mockRejectedValue(new Error('down'))
  const { container } = render(<TrendingStrip onSelect={vi.fn()} />)
  await waitFor(() => expect(api.getMovers).toHaveBeenCalled())
  expect(container.firstChild).toBeNull()
})
