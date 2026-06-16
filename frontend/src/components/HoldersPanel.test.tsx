import { render, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import { HoldersPanel } from './panels/HoldersPanel'
import * as api from '../api'
import type { OwnershipHolding } from '../types'

vi.mock('../api')
afterEach(() => vi.clearAllMocks())

function holding(holder: string, shares: number): OwnershipHolding {
  return { company_id: 1, holder, kind: 'insider', shares, as_of: '2024-02-01', source: 'edgar' }
}

test('lists holders with formatted share counts', async () => {
  vi.mocked(api.getHolders).mockResolvedValue([holding('Tim Cook', 3_280_000), holding('Jeff Williams', 500_000)])
  render(<HoldersPanel ticker="AAPL" />)
  expect(await screen.findByText('Tim Cook')).toBeInTheDocument()
  expect(screen.getByText('Jeff Williams')).toBeInTheDocument()
  expect(api.getHolders).toHaveBeenCalledWith('AAPL')
})

test('shows an empty state when there are no holders', async () => {
  vi.mocked(api.getHolders).mockResolvedValue([])
  render(<HoldersPanel ticker="AAPL" />)
  expect(await screen.findByText(/no holder data/i)).toBeInTheDocument()
})

test('shows the empty state when the request fails', async () => {
  vi.mocked(api.getHolders).mockRejectedValue(new Error('down'))
  render(<HoldersPanel ticker="AAPL" />)
  await waitFor(() => expect(api.getHolders).toHaveBeenCalled())
  expect(screen.getByText(/no holder data/i)).toBeInTheDocument()
})
