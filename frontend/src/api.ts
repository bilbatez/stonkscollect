import type {
  Company,
  CompanyData,
  Discrepancy,
  FinancialFact,
  NewsItem,
  PricePoint,
  Ratio,
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
    throw new Error(`request failed: ${res.status}`)
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
    throw new Error(`request failed: ${res.status}`)
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
  const [company, prices, facts, ratios, news, discrepancies] = await Promise.all([
    getJson<Company>(base),
    getJson<PricePoint[]>(`${base}/prices`),
    getJson<FinancialFact[]>(`${base}/facts`),
    getJson<Ratio[]>(`${base}/ratios`),
    getJson<NewsItem[]>(`${base}/news`),
    getJson<Discrepancy[]>(`${base}/discrepancies`),
  ])
  return { company, prices, facts, ratios, news, discrepancies }
}
