import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import { CompareView } from './pages/CompareView'
import * as api from '../api'
import type { Company, CompanyData, CompanyRow } from '../types'

vi.mock('../api')
// Canvas chart wrapper is coverage-excluded glue; echarts needs a real canvas.
vi.mock('../charts/CompareChart', () => ({ default: () => <div data-testid="compare-chart" /> }))
afterEach(() => vi.clearAllMocks())

function company(ticker: string): Company {
  return { id: 1, cik: '', ticker, name: `${ticker} Corp`, exchange: null, sector: null, industry: null, description: null, website: null }
}

function companyRow(ticker: string): CompanyRow {
  return { company: company(ticker), score: null }
}

function data(ticker: string): CompanyData {
  return {
    company: company(ticker),
    prices: [],
    facts: [],
    ratios: [
      { company_id: 1, period_end: '2023-12-31', period_type: 'annual', metric: 'roe', value: 1.5, computed_at: '' },
      // quarterly entry must be ignored by latestMetrics
      { company_id: 1, period_end: '2023-09-30', period_type: 'quarterly', metric: 'roe', value: 0.4, computed_at: '' },
    ],
    news: [],
    discrepancies: [],
    graham: { criteria: [], score: 0, graham_number: null, ncav_per_share: null, margin_of_safety: null, net_net: false, passes_defensive: false },
    peers: [],
    note: { body: null },
    shares: null,
  }
}

test('renders empty state with prompt text', () => {
  render(<CompareView />)
  expect(screen.getByText(/add tickers above/i)).toBeInTheDocument()
})

test('typing triggers a company search', async () => {
  vi.mocked(api.listCompanies).mockResolvedValue({ rows: [companyRow('AAPL')], total: 1 })
  render(<CompareView />)
  await userEvent.type(screen.getByRole('combobox'), 'ap')
  await waitFor(
    () => expect(vi.mocked(api.listCompanies)).toHaveBeenCalledWith('ap', {}, null, 'asc', 8, 0),
    { timeout: 2000 },
  )
})

test('selecting an option adds a chip and a compare row', async () => {
  vi.mocked(api.listCompanies).mockResolvedValue({ rows: [companyRow('AAPL')], total: 1 })
  vi.mocked(api.loadCompanyData).mockResolvedValue(data('AAPL'))
  render(<CompareView />)

  await userEvent.type(screen.getByRole('combobox'), 'ap')
  const option = await screen.findByRole('option', { name: /AAPL/ }, { timeout: 2000 })
  await userEvent.click(option)

  await waitFor(() => expect(screen.getByText('Return on equity')).toBeInTheDocument())
  // AAPL appears in the chip + in the compare table row
  expect(screen.getAllByText('AAPL').length).toBeGreaterThan(0)
})

test('duplicate ticker is ignored', async () => {
  vi.mocked(api.listCompanies).mockResolvedValue({ rows: [companyRow('AAPL')], total: 1 })
  vi.mocked(api.loadCompanyData).mockResolvedValue(data('AAPL'))
  render(<CompareView />)

  const addAapl = async () => {
    await userEvent.type(screen.getByRole('combobox'), 'ap')
    await userEvent.click(await screen.findByRole('option', { name: /AAPL/ }, { timeout: 2000 }))
  }

  await addAapl()
  await screen.findByText('Return on equity')

  await addAapl()
  expect(vi.mocked(api.loadCompanyData)).toHaveBeenCalledTimes(1)
})

test('removing a chip removes the row from the table', async () => {
  vi.mocked(api.listCompanies).mockResolvedValue({ rows: [companyRow('AAPL')], total: 1 })
  vi.mocked(api.loadCompanyData).mockResolvedValue(data('AAPL'))
  render(<CompareView />)

  await userEvent.type(screen.getByRole('combobox'), 'ap')
  await userEvent.click(await screen.findByRole('option', { name: /AAPL/ }, { timeout: 2000 }))
  await screen.findByText('Return on equity')

  await userEvent.click(screen.getByTestId('CancelIcon'))

  await waitFor(() => {
    expect(screen.queryByText('Return on equity')).not.toBeInTheDocument()
    expect(screen.getByText(/add tickers above/i)).toBeInTheDocument()
  })
})

test('clearing then re-selecting covers null onChange and duplicate guard', async () => {
  vi.mocked(api.listCompanies).mockResolvedValue({ rows: [companyRow('AAPL')], total: 1 })
  vi.mocked(api.loadCompanyData).mockResolvedValue(data('AAPL'))
  render(<CompareView />)

  // First add
  await userEvent.type(screen.getByRole('combobox'), 'ap')
  await userEvent.click(await screen.findByRole('option', { name: /AAPL/ }, { timeout: 2000 }))
  await screen.findByText('Return on equity')

  // Clear resets internal MUI value → null → addTicker(null) → line 35 early return
  const clearBtn = screen.getByRole('button', { name: 'Clear', hidden: true })
  fireEvent.click(clearBtn)

  // Re-select AAPL: MUI internal value was null, now changes → fires onChange
  // addTicker(companyRow('AAPL')) → tickers.includes('AAPL') = true → line 37 early return
  await userEvent.type(screen.getByRole('combobox'), 'ap')
  await userEvent.click(await screen.findByRole('option', { name: /AAPL/ }, { timeout: 2000 }))

  // loadCompanyData only called once (duplicate guard prevented second load)
  expect(vi.mocked(api.loadCompanyData)).toHaveBeenCalledTimes(1)
})

test('shows skeleton while loading', async () => {
  vi.mocked(api.listCompanies).mockResolvedValue({ rows: [companyRow('AAPL')], total: 1 })
  vi.mocked(api.loadCompanyData).mockImplementation(() => new Promise(() => {}))
  render(<CompareView />)

  await userEvent.type(screen.getByRole('combobox'), 'ap')
  await userEvent.click(await screen.findByRole('option', { name: /AAPL/ }, { timeout: 2000 }))

  expect(screen.getByRole('status')).toBeInTheDocument()
})
