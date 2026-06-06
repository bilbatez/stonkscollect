import { afterEach, expect, test, vi } from 'vitest'
import { loadCompanyData } from './api'

afterEach(() => {
  vi.restoreAllMocks()
})

function mockFetchSequence(bodies: unknown[]) {
  let i = 0
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => {
      const body = bodies[i++]
      return { ok: true, status: 200, json: async () => body } as Response
    }),
  )
}

test('loadCompanyData fetches all sections and assembles them', async () => {
  const company = { id: 1, ticker: 'AAPL', name: 'Apple', cik: '', exchange: null, sector: null, industry: null }
  mockFetchSequence([company, [{ close: 1 }], [{ line_item: 'Revenue' }], [{ metric: 'pe' }], [{ title: 'Hi' }], [{ field: 'Revenue' }]])
  const data = await loadCompanyData('AAPL')
  expect(data.company.ticker).toBe('AAPL')
  expect(data.prices).toHaveLength(1)
  expect(data.facts[0].line_item).toBe('Revenue')
  expect(data.ratios[0].metric).toBe('pe')
  expect(data.news[0].title).toBe('Hi')
  expect(data.discrepancies[0].field).toBe('Revenue')
})

test('loadCompanyData throws on a non-ok response', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => ({ ok: false, status: 404, json: async () => ({}) }) as Response),
  )
  await expect(loadCompanyData('NOPE')).rejects.toThrow(/404/)
})

test('requests target the expected URLs', async () => {
  const calls: string[] = []
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string) => {
      calls.push(url)
      return { ok: true, status: 200, json: async () => [] } as Response
    }),
  )
  await loadCompanyData('msft')
  expect(calls[0]).toBe('/api/companies/msft')
  expect(calls).toContain('/api/companies/msft/prices')
  expect(calls).toContain('/api/companies/msft/discrepancies')
})
