import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import * as api from '../../api'
import { MoversView } from './MoversView'
import type { Company, MoverRow, Movers } from '../../types'

vi.mock('../../api')
const mocked = vi.mocked(api)
afterEach(() => vi.clearAllMocks())

function mover(ticker: string, changePct: number, volume: number | null): MoverRow {
  const company: Company = {
    id: 1,
    cik: '1',
    ticker,
    name: `${ticker} Inc.`,
    exchange: null,
    sector: null,
    industry: null,
    description: null,
    website: null,
  }
  return {
    company,
    last_close: 110,
    change: 100 * changePct,
    change_pct: changePct,
    volume,
    as_of: '2024-03-01',
  }
}

test('renders gainers, losers, and most active with formatted moves', async () => {
  mocked.getMovers.mockResolvedValue({
    gainers: [mover('UP', 0.1, 50)],
    losers: [mover('DOWN', -0.2, 900)],
    most_active: [mover('BUSY', 0.01, 1_200_000)],
  })
  const onSelect = vi.fn()
  render(<MoversView onSelect={onSelect} />)

  expect(await screen.findByText('Top gainers')).toBeInTheDocument()
  expect(screen.getByText('Top losers')).toBeInTheDocument()
  expect(screen.getByText('Most active')).toBeInTheDocument()
  expect(screen.getByText('+10.00')).toBeInTheDocument() // UP change
  expect(screen.getByText('+10%')).toBeInTheDocument()
  expect(screen.getByText('-20.00')).toBeInTheDocument() // DOWN change
  expect(screen.getByText('1.20M')).toBeInTheDocument() // BUSY volume

  await userEvent.click(screen.getByRole('button', { name: 'UP' }))
  expect(onSelect).toHaveBeenCalledWith('UP')
})

test('an empty bucket shows its empty message', async () => {
  mocked.getMovers.mockResolvedValue({ gainers: [], losers: [], most_active: [] })
  render(<MoversView onSelect={vi.fn()} />)
  expect(await screen.findAllByText(/no movers yet/i)).toHaveLength(3)
})

test('a failed load shows an error with a working retry', async () => {
  mocked.getMovers.mockRejectedValueOnce(new Error('boom'))
  mocked.getMovers.mockResolvedValueOnce({
    gainers: [mover('OK', 0.05, 1)],
    losers: [],
    most_active: [],
  })
  render(<MoversView onSelect={vi.fn()} />)
  expect(await screen.findByText(/failed to load movers/i)).toBeInTheDocument()
  await userEvent.click(screen.getByRole('button', { name: /retry/i }))
  await waitFor(() => expect(screen.getByRole('button', { name: 'OK' })).toBeInTheDocument())
  expect(mocked.getMovers).toHaveBeenCalledTimes(2)
})

test('late responses after unmount are ignored', async () => {
  let resolveLoad!: (m: Movers) => void
  mocked.getMovers.mockReturnValue(
    new Promise((resolve) => {
      resolveLoad = resolve
    }),
  )
  const { unmount } = render(<MoversView onSelect={vi.fn()} />)
  unmount()
  resolveLoad({ gainers: [], losers: [], most_active: [] })
  await Promise.resolve()

  let rejectLoad!: (e: Error) => void
  mocked.getMovers.mockReturnValue(
    new Promise((_resolve, reject) => {
      rejectLoad = reject
    }),
  )
  const second = render(<MoversView onSelect={vi.fn()} />)
  second.unmount()
  rejectLoad(new Error('late'))
  await Promise.resolve()
})
