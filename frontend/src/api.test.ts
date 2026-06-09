import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import {
  addWatch,
  clearToken,
  getToken,
  getWatchlist,
  listCompanies,
  loadCompanyData,
  login,
  logout,
  removeWatch,
  screen,
  setToken,
  signup,
} from './api'

interface Call {
  url: string
  init: RequestInit
}
let calls: Call[]

function mockFetch(handler: (url: string, init: RequestInit) => Partial<Response> & { json?: () => Promise<unknown> }) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init: RequestInit = {}) => {
      calls.push({ url, init })
      return { ok: true, status: 200, json: async () => ({}), ...handler(url, init) } as Response
    }),
  )
}

beforeEach(() => {
  calls = []
  localStorage.clear()
})
afterEach(() => {
  vi.restoreAllMocks()
})

test('token storage helpers round-trip', () => {
  expect(getToken()).toBeNull()
  setToken('abc')
  expect(getToken()).toBe('abc')
  clearToken()
  expect(getToken()).toBeNull()
})

test('signup and login store the token and attach it to later requests', async () => {
  mockFetch(() => ({ json: async () => ({ token: 't1' }) }))
  await signup('a@e.com', 'pw')
  expect(getToken()).toBe('t1')
  expect(calls[0].url).toBe('/auth/signup')

  await login('a@e.com', 'pw')
  expect(getToken()).toBe('t1')

  // a subsequent authed call carries the bearer header
  await getWatchlist()
  const headers = new Headers(calls[2].init.headers)
  expect(headers.get('Authorization')).toBe('Bearer t1')
})

test('requests without a token omit the Authorization header', async () => {
  mockFetch(() => ({ json: async () => [] }))
  await getWatchlist()
  const headers = new Headers(calls[0].init.headers)
  expect(headers.has('Authorization')).toBe(false)
})

test('logout clears the token', async () => {
  setToken('t')
  mockFetch(() => ({}))
  await logout()
  expect(calls[0].url).toBe('/auth/logout')
  expect(getToken()).toBeNull()
})

test('getJson throws on a non-ok response', async () => {
  mockFetch(() => ({ ok: false, status: 401 }))
  await expect(getWatchlist()).rejects.toThrow(/401/)
})

test('postJson throws on a non-ok response', async () => {
  mockFetch(() => ({ ok: false, status: 409 }))
  await expect(signup('a@e.com', 'pw')).rejects.toThrow(/409/)
})

test('watchlist add and remove hit the right method + path', async () => {
  mockFetch(() => ({ json: async () => [] }))
  await addWatch('AAPL')
  expect(calls[0].url).toBe('/api/watchlist')
  expect(calls[0].init.method).toBe('POST')
  await removeWatch('AAPL')
  expect(calls[1].url).toBe('/api/watchlist/AAPL')
  expect(calls[1].init.method).toBe('DELETE')
})

test('loadCompanyData fetches and assembles all sections', async () => {
  const company = { id: 1, ticker: 'AAPL', name: 'Apple', cik: '', exchange: null, sector: null, industry: null }
  let i = 0
  const bodies: unknown[] = [company, [{ close: 1 }], [{ line_item: 'Revenue' }], [{ metric: 'roe' }], [{ title: 'Hi' }], [{ field: 'Revenue' }], { score: 5, criteria: [] }]
  mockFetch(() => ({ json: async () => bodies[i++] }))
  const data = await loadCompanyData('AAPL')
  expect(data.company.ticker).toBe('AAPL')
  expect(data.prices).toHaveLength(1)
  expect(data.ratios[0].metric).toBe('roe')
  expect(data.graham.score).toBe(5)
  expect(calls.map((c) => c.url)).toContain('/api/companies/AAPL/graham')
})

test('listCompanies builds query strings with optional search and sort', async () => {
  mockFetch(() => ({ json: async () => ({ rows: [], total: 0 }) }))
  await listCompanies('app', null, 'asc', 25, 50)
  expect(calls[0].url).toBe('/api/companies?limit=25&offset=50&q=app')
  await listCompanies('', null, 'asc', 25, 0)
  expect(calls[1].url).toBe('/api/companies?limit=25&offset=0')
  await listCompanies('', 'score', 'desc', 25, 0)
  expect(calls[2].url).toBe('/api/companies?limit=25&offset=0&sort_by=score&sort_dir=desc')
})

test('screen builds the query string from filters and defaults', async () => {
  mockFetch(() => ({ json: async () => ({ rows: [], total: 0 }) }))
  await screen({ defensive: true, net_net: false, min_score: 3, limit: 10, offset: 0 })
  expect(calls[0].url).toBe('/api/screen?defensive=true&net_net=false&min_score=3&limit=10&offset=0')
  await screen({})
  expect(calls[1].url).toBe('/api/screen?defensive=false&net_net=false&min_score=0&limit=50&offset=0')
  await screen({ sort_by: 'score', sort_dir: 'desc' })
  expect(calls[2].url).toBe('/api/screen?defensive=false&net_net=false&min_score=0&limit=50&offset=0&sort_by=score&sort_dir=desc')
  await screen({ sort_by: 'graham_number' })
  expect(calls[3].url).toBe('/api/screen?defensive=false&net_net=false&min_score=0&limit=50&offset=0&sort_by=graham_number&sort_dir=asc')
})
