import type {
  Company,
  CompanyData,
  CompanyRow,
  CompanySummary,
  Discrepancy,
  FinancialFact,
  GrahamAssessment,
  MoverRow,
  Movers,
  NewsItem,
  Note,
  OwnershipHolding,
  Me,
  Page,
  PeerRow,
  PricePoint,
  ScreenFilters,
  ScreenRow,
  SectorStats,
  UserSettings,
  WatchGroup,
  WatchQuote,
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

/** Send a body-bearing or empty mutation and throw on a non-ok response.
 *  Mutations return no JSON, but a failed write must not look like success. */
async function mutate(method: string, path: string, body?: unknown): Promise<void> {
  const init: RequestInit = { method }
  if (body !== undefined) {
    init.headers = { 'content-type': 'application/json' }
    init.body = JSON.stringify(body)
  }
  const res = await authedFetch(path, init)
  if (!res.ok) {
    throw new Error(`request failed: ${res.status} ${res.statusText}`)
  }
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
  await mutate('POST', '/auth/logout')
  clearToken()
}

// --- Profile ---

/** The current user's profile (email + display name). */
export function getMe(): Promise<Me> {
  return getJson<Me>('/auth/me')
}

/** Update the current user's email + display name. */
export async function updateProfile(email: string, displayName: string): Promise<void> {
  await mutate('PUT', '/auth/profile', { email, display_name: displayName })
}

/** Change the current user's password (old password verified server-side). */
export async function changePassword(oldPassword: string, newPassword: string): Promise<void> {
  await mutate('PUT', '/auth/password', { old_password: oldPassword, new_password: newPassword })
}

/** The current user's settings (theme + Graham thresholds). */
export function getSettings(): Promise<UserSettings> {
  return getJson<UserSettings>('/auth/settings')
}

/** Replace the current user's settings. */
export async function updateSettings(settings: UserSettings): Promise<void> {
  await mutate('PUT', '/auth/settings', settings)
}

// --- Watchlist ---

export function getWatchlist(): Promise<Company[]> {
  return getJson<Company[]>('/api/watchlist')
}

export async function addWatch(ticker: string): Promise<void> {
  await mutate('POST', '/api/watchlist', { ticker })
}

export async function removeWatch(ticker: string): Promise<void> {
  await mutate('DELETE', `/api/watchlist/${encodeURIComponent(ticker)}`)
}

// --- Watch groups (tags) ---

export function getGroups(): Promise<WatchGroup[]> {
  return getJson<WatchGroup[]>('/api/watchlist/groups')
}

export async function createGroup(name: string): Promise<void> {
  await mutate('POST', '/api/watchlist/groups', { name })
}

export async function renameGroup(id: number, name: string): Promise<void> {
  await mutate('PUT', `/api/watchlist/groups/${id}`, { name })
}

export async function deleteGroup(id: number): Promise<void> {
  await mutate('DELETE', `/api/watchlist/groups/${id}`)
}

export async function tagWatch(ticker: string, groupId: number): Promise<void> {
  await mutate('POST', `/api/watchlist/${encodeURIComponent(ticker)}/groups`, { group_id: groupId })
}

export async function untagWatch(ticker: string, groupId: number): Promise<void> {
  await mutate('DELETE', `/api/watchlist/${encodeURIComponent(ticker)}/groups/${groupId}`)
}

// --- Company data ---

/** Fetch a company and all of its records in parallel. The summary endpoint
 *  supplies the company, ratios, and latest share count in one round trip. */
export async function loadCompanyData(ticker: string): Promise<CompanyData> {
  const base = `/api/companies/${ticker}`
  const [summary, prices, facts, news, discrepancies, graham, peers, note] =
    await Promise.all([
      getJson<CompanySummary>(`${base}/summary`),
      getJson<PricePoint[]>(`${base}/prices`),
      getJson<FinancialFact[]>(`${base}/facts`),
      getJson<NewsItem[]>(`${base}/news`),
      getJson<Discrepancy[]>(`${base}/discrepancies`),
      getJson<GrahamAssessment>(`${base}/graham`),
      getJson<PeerRow[]>(`${base}/peers`),
      getJson<Note>(`${base}/note`),
    ])
  return {
    company: summary.company,
    ratios: summary.ratios,
    shares: summary.shares,
    prices,
    facts,
    news,
    discrepancies,
    graham,
    peers,
    note,
  }
}

/** Market movers: top gainers / losers / most active by latest daily change. */
export function getMovers(limit = 10): Promise<Movers> {
  return getJson<Movers>(`/api/movers?limit=${limit}`)
}

/** The watchlist with each company's latest daily quote. */
export function getWatchlistQuotes(): Promise<WatchQuote[]> {
  return getJson<WatchQuote[]>('/api/watchlist/quotes')
}

/** Latest close + day change for each tracked market index (S&P/Nasdaq/Dow). */
export function getMarketSummary(): Promise<MoverRow[]> {
  return getJson<MoverRow[]>('/api/markets/summary')
}

/** Paginated, optionally-searched/filtered directory of all companies + their
 *  scores. `filters` holds optional per-column substring filters keyed by column
 *  id (ticker/name/industry); empty values are omitted. */
export function listCompanies(
  q: string,
  filters: Record<string, string>,
  sortBy: string | null,
  sortDir: 'asc' | 'desc',
  limit: number,
  offset: number,
  includeDelisted = false,
): Promise<Page<CompanyRow>> {
  const params = new URLSearchParams({ limit: String(limit), offset: String(offset) })
  if (q !== '') {
    params.set('q', q)
  }
  for (const [key, value] of Object.entries(filters)) {
    if (value !== '') {
      params.set(key, value)
    }
  }
  if (sortBy !== null) {
    params.set('sort_by', sortBy)
    params.set('sort_dir', sortDir)
  }
  if (includeDelisted) {
    params.set('include_delisted', 'true')
  }
  return getJson<Page<CompanyRow>>(`/api/companies?${params.toString()}`)
}

/** Manually set a company's listing status (active/delisted). */
export async function setCompanyStatus(ticker: string, status: string): Promise<void> {
  await mutate('PUT', `/api/companies/${encodeURIComponent(ticker)}/status`, { status })
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
  if (f.sector !== undefined && f.sector !== '') params.set('sector', f.sector)
  if (f.min_pe !== undefined) params.set('min_pe', String(f.min_pe))
  if (f.max_pe !== undefined) params.set('max_pe', String(f.max_pe))
  if (f.min_roe !== undefined) params.set('min_roe', String(f.min_roe))
  if (f.max_de !== undefined) params.set('max_de', String(f.max_de))
  if (f.min_margin !== undefined) params.set('min_margin', String(f.min_margin))
  return getJson<Page<ScreenRow>>(`/api/screen?${params.toString()}`)
}

/** Peers in the same sector, sorted by Graham score. */
export function getPeers(ticker: string): Promise<PeerRow[]> {
  return getJson<PeerRow[]>(`/api/companies/${ticker}/peers`)
}

/** A company's holders (e.g. insider Form 4 positions), newest filing first. */
export function getHolders(ticker: string): Promise<OwnershipHolding[]> {
  return getJson<OwnershipHolding[]>(`/api/companies/${ticker}/holders`)
}

/** Get the current user's note for a company. */
export function getNote(ticker: string): Promise<Note> {
  return getJson<Note>(`/api/companies/${ticker}/note`)
}

/** Save (upsert) a note for a company. */
export async function saveNote(ticker: string, body: string): Promise<void> {
  await mutate('PUT', `/api/companies/${encodeURIComponent(ticker)}/note`, { body })
}

/** Delete the note for a company. */
export async function deleteNote(ticker: string): Promise<void> {
  await mutate('DELETE', `/api/companies/${encodeURIComponent(ticker)}/note`)
}

/** Sector-level aggregates for the overview page. */
export function getSectors(): Promise<SectorStats[]> {
  return getJson<SectorStats[]>('/api/sectors')
}
