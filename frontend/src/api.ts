import type {
  Company,
  CompanyData,
  CompanyRow,
  Discrepancy,
  FinancialFact,
  GrahamAssessment,
  NewsItem,
  Page,
  PricePoint,
  Ratio,
  ScreenFilters,
  ScreenRow,
} from './types'

const TOKEN_KEY = 'stonks_token'

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY)
}
export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token)
}
export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY)
}

/** fetch with the bearer token attached (when present). */
async function authedFetch(path: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers)
  const token = getToken()
  if (token) {
    headers.set('Authorization', `Bearer ${token}`)
  }
  return fetch(path, { ...init, headers })
}

async function getJson<T>(path: string): Promise<T> {
  const res = await authedFetch(path)
  if (!res.ok) {
    throw new Error(`request failed: ${res.status} ${res.statusText}`)
  }
  return (await res.json()) as T
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await authedFetch(path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) {
    throw new Error(`request failed: ${res.status} ${res.statusText}`)
  }
  return (await res.json()) as T
}

// --- Auth ---

interface TokenResponse {
  token: string
}

export async function signup(email: string, password: string): Promise<string> {
  const { token } = await postJson<TokenResponse>('/auth/signup', { email, password })
  setToken(token)
  return token
}

export async function login(email: string, password: string): Promise<string> {
  const { token } = await postJson<TokenResponse>('/auth/login', { email, password })
  setToken(token)
  return token
}

export async function logout(): Promise<void> {
  await authedFetch('/auth/logout', { method: 'POST' })
  clearToken()
}

// --- Watchlist ---

export function getWatchlist(): Promise<Company[]> {
  return getJson<Company[]>('/api/watchlist')
}

export async function addWatch(ticker: string): Promise<void> {
  await authedFetch('/api/watchlist', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ ticker }),
  })
}

export async function removeWatch(ticker: string): Promise<void> {
  await authedFetch(`/api/watchlist/${ticker}`, { method: 'DELETE' })
}

// --- Company data ---

/** Fetch a company and all of its records in parallel. */
export async function loadCompanyData(ticker: string): Promise<CompanyData> {
  const base = `/api/companies/${ticker}`
  const [company, prices, facts, ratios, news, discrepancies, graham] = await Promise.all([
    getJson<Company>(base),
    getJson<PricePoint[]>(`${base}/prices`),
    getJson<FinancialFact[]>(`${base}/facts`),
    getJson<Ratio[]>(`${base}/ratios`),
    getJson<NewsItem[]>(`${base}/news`),
    getJson<Discrepancy[]>(`${base}/discrepancies`),
    getJson<GrahamAssessment>(`${base}/graham`),
  ])
  return { company, prices, facts, ratios, news, discrepancies, graham }
}

/** Paginated, optionally-searched directory of all companies + their scores. */
export function listCompanies(
  q: string,
  sortBy: string | null,
  sortDir: 'asc' | 'desc',
  limit: number,
  offset: number,
): Promise<Page<CompanyRow>> {
  const params = new URLSearchParams({ limit: String(limit), offset: String(offset) })
  if (q !== '') {
    params.set('q', q)
  }
  if (sortBy !== null) {
    params.set('sort_by', sortBy)
    params.set('sort_dir', sortDir)
  }
  return getJson<Page<CompanyRow>>(`/api/companies?${params.toString()}`)
}

/** Screen companies by Graham score, ranked, with optional filters + paging. */
export function screen(f: ScreenFilters): Promise<Page<ScreenRow>> {
  const params = new URLSearchParams({
    defensive: String(f.defensive ?? false),
    net_net: String(f.net_net ?? false),
    min_score: String(f.min_score ?? 0),
    limit: String(f.limit ?? 50),
    offset: String(f.offset ?? 0),
  })
  if (f.sort_by !== undefined) {
    params.set('sort_by', f.sort_by)
    params.set('sort_dir', f.sort_dir ?? 'asc')
  }
  return getJson<Page<ScreenRow>>(`/api/screen?${params.toString()}`)
}
