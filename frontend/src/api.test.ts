import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import {
  addWatch,
  changePassword,
  clearToken,
  createGroup,
  deleteGroup,
  deleteNote,
  getGroups,
  getHolders,
  getMarketSummary,
  getMe,
  getMovers,
  getNote,
  getPeers,
  getSectors,
  getToken,
  getWatchlist,
  getWatchlistQuotes,
  listCompanies,
  loadCompanyData,
  login,
  logout,
  removeWatch,
  renameGroup,
  saveNote,
  screen,
  setCompanyStatus,
  setToken,
  signup,
  tagWatch,
  untagWatch,
  updateProfile,
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

test('a mutation throws on a non-ok response instead of silently succeeding', async () => {
  mockFetch(() => ({ ok: false, status: 500 }))
  await expect(addWatch('AAPL')).rejects.toThrow(/500/)
})

test('loadCompanyData fetches the summary and assembles all sections', async () => {
  const company = { id: 1, ticker: 'AAPL', name: 'Apple', cik: '', exchange: null, sector: null, industry: null }
  const summary = {
    company,
    ratios: [{ metric: 'roe' }],
    graham: null,
    shares: { company_id: 1, as_of: '2023-09-30', shares: 100, source: 'edgar' },
  }
  const byPath: Record<string, unknown> = {
    '/api/companies/AAPL/summary': summary,
    '/api/companies/AAPL/prices': [{ close: 1 }],
    '/api/companies/AAPL/facts': [{ line_item: 'Revenue' }],
    '/api/companies/AAPL/news': [{ title: 'Hi' }],
    '/api/companies/AAPL/discrepancies': [{ field: 'Revenue' }],
    '/api/companies/AAPL/graham': { score: 5, criteria: [] },
    '/api/companies/AAPL/peers': [],
    '/api/companies/AAPL/note': { body: null },
  }
  mockFetch((url) => ({ json: async () => byPath[url] }))
  const data = await loadCompanyData('AAPL')
  expect(data.company.ticker).toBe('AAPL')
  expect(data.prices).toHaveLength(1)
  expect(data.ratios[0].metric).toBe('roe')
  expect(data.shares?.shares).toBe(100)
  expect(data.graham.score).toBe(5)
  const urls = calls.map((c) => c.url)
  expect(urls).toContain('/api/companies/AAPL/summary')
  expect(urls).toContain('/api/companies/AAPL/graham')
  // the summary replaces the bare company and ratios fetches
  expect(urls).not.toContain('/api/companies/AAPL')
  expect(urls).not.toContain('/api/companies/AAPL/ratios')
})

test('listCompanies builds query strings with optional search, filters, and sort', async () => {
  mockFetch(() => ({ json: async () => ({ rows: [], total: 0 }) }))
  await listCompanies('app', {}, null, 'asc', 25, 50)
  expect(calls[0].url).toBe('/api/companies?limit=25&offset=50&q=app')
  await listCompanies('', {}, null, 'asc', 25, 0)
  expect(calls[1].url).toBe('/api/companies?limit=25&offset=0')
  await listCompanies('', {}, 'score', 'desc', 25, 0)
  expect(calls[2].url).toBe('/api/companies?limit=25&offset=0&sort_by=score&sort_dir=desc')
  // per-column filters become query params; empty values are skipped
  await listCompanies('', { industry: 'Software', ticker: '', name: 'Acme' }, null, 'asc', 25, 0)
  expect(calls[3].url).toBe('/api/companies?limit=25&offset=0&industry=Software&name=Acme')
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
  // sector filter appended when non-empty
  await screen({ sector: 'Technology' })
  expect(calls[4].url).toContain('sector=Technology')
  // empty sector omitted
  await screen({ sector: '' })
  expect(calls[5].url).not.toContain('sector')
  // ratio filters appended when set
  await screen({ min_pe: 10, max_pe: 20, min_roe: 0.1, max_de: 0.5, min_margin: 0.05 })
  const ratioUrl = calls[6].url
  expect(ratioUrl).toContain('min_pe=10')
  expect(ratioUrl).toContain('max_pe=20')
  expect(ratioUrl).toContain('min_roe=0.1')
  expect(ratioUrl).toContain('max_de=0.5')
  expect(ratioUrl).toContain('min_margin=0.05')
})

test('loadCompanyData includes peers and note', async () => {
  const summary = {
    company: { id: 1, ticker: 'AAPL', name: 'Apple', cik: '', exchange: null, sector: null, industry: null },
    ratios: [],
    graham: null,
    shares: null,
  }
  mockFetch((url) => ({
    json: async () => {
      if (url.endsWith('/summary')) return summary
      if (url.endsWith('/peers')) return [{ company: { ticker: 'MSFT' }, score: null }]
      if (url.endsWith('/note')) return { body: 'my note' }
      if (url.endsWith('/graham')) return { score: 5, criteria: [] }
      return []
    },
  }))
  const data = await loadCompanyData('AAPL')
  expect(data.peers[0].company.ticker).toBe('MSFT')
  expect(data.note.body).toBe('my note')
  expect(data.shares).toBeNull()
  expect(calls.map((c) => c.url)).toContain('/api/companies/AAPL/peers')
  expect(calls.map((c) => c.url)).toContain('/api/companies/AAPL/note')
})

test('getMovers and getWatchlistQuotes hit their endpoints', async () => {
  mockFetch((url) => ({
    json: async () =>
      url.startsWith('/api/movers')
        ? { gainers: [], losers: [], most_active: [] }
        : [],
  }))
  const movers = await getMovers(5)
  expect(calls[0].url).toBe('/api/movers?limit=5')
  expect(movers.gainers).toEqual([])
  await getMovers()
  expect(calls[1].url).toBe('/api/movers?limit=10')
  await getWatchlistQuotes()
  expect(calls[2].url).toBe('/api/watchlist/quotes')
})

test('getPeers, getNote, saveNote, deleteNote hit the right endpoints', async () => {
  mockFetch(() => ({ json: async () => [] }))
  await getPeers('AAPL')
  expect(calls[0].url).toBe('/api/companies/AAPL/peers')
  await getNote('AAPL')
  expect(calls[1].url).toBe('/api/companies/AAPL/note')
  await saveNote('AAPL', 'text')
  expect(calls[2].url).toBe('/api/companies/AAPL/note')
  expect(calls[2].init.method).toBe('PUT')
  expect(JSON.parse(calls[2].init.body as string)).toEqual({ body: 'text' })
  await deleteNote('AAPL')
  expect(calls[3].url).toBe('/api/companies/AAPL/note')
  expect(calls[3].init.method).toBe('DELETE')
})

test('getSectors hits /api/sectors', async () => {
  mockFetch(() => ({ json: async () => [] }))
  await getSectors()
  expect(calls[0].url).toBe('/api/sectors')
})

test('getMarketSummary hits /api/markets/summary', async () => {
  mockFetch(() => ({ json: async () => [] }))
  await getMarketSummary()
  expect(calls[0].url).toBe('/api/markets/summary')
})

test('getHolders hits /api/companies/:ticker/holders', async () => {
  mockFetch(() => ({ json: async () => [] }))
  await getHolders('AAPL')
  expect(calls[0].url).toBe('/api/companies/AAPL/holders')
})

test('profile endpoints: getMe, updateProfile, changePassword', async () => {
  mockFetch(() => ({ json: async () => ({ email: 'u@e.com', display_name: 'Uma' }) }))
  await getMe()
  expect(calls[0].url).toBe('/auth/me')
  await updateProfile('u@e.com', 'Uma')
  expect(calls[1].url).toBe('/auth/profile')
  expect(calls[1].init.method).toBe('PUT')
  expect(JSON.parse(calls[1].init.body as string)).toEqual({ email: 'u@e.com', display_name: 'Uma' })
  await changePassword('old', 'new')
  expect(calls[2].url).toBe('/auth/password')
  expect(JSON.parse(calls[2].init.body as string)).toEqual({ old_password: 'old', new_password: 'new' })
})

test('watch group endpoints', async () => {
  mockFetch(() => ({ json: async () => [] }))
  await getGroups()
  expect(calls[0].url).toBe('/api/watchlist/groups')
  await createGroup('Tech')
  expect(calls[1].url).toBe('/api/watchlist/groups')
  expect(calls[1].init.method).toBe('POST')
  expect(JSON.parse(calls[1].init.body as string)).toEqual({ name: 'Tech' })
  await renameGroup(3, 'Technology')
  expect(calls[2].url).toBe('/api/watchlist/groups/3')
  expect(calls[2].init.method).toBe('PUT')
  await deleteGroup(3)
  expect(calls[3].url).toBe('/api/watchlist/groups/3')
  expect(calls[3].init.method).toBe('DELETE')
  await tagWatch('AAPL', 3)
  expect(calls[4].url).toBe('/api/watchlist/AAPL/groups')
  expect(JSON.parse(calls[4].init.body as string)).toEqual({ group_id: 3 })
  await untagWatch('AAPL', 3)
  expect(calls[5].url).toBe('/api/watchlist/AAPL/groups/3')
  expect(calls[5].init.method).toBe('DELETE')
})

test('listCompanies appends include_delisted and setCompanyStatus puts the status', async () => {
  mockFetch(() => ({ json: async () => ({ rows: [], total: 0 }) }))
  await listCompanies('', {}, null, 'asc', 25, 0, true)
  expect(calls[0].url).toBe('/api/companies?limit=25&offset=0&include_delisted=true')
  await setCompanyStatus('AAPL', 'delisted')
  expect(calls[1].url).toBe('/api/companies/AAPL/status')
  expect(calls[1].init.method).toBe('PUT')
  expect(JSON.parse(calls[1].init.body as string)).toEqual({ status: 'delisted' })
})
