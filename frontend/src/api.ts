import type {
  Company,
  CompanyData,
  Discrepancy,
  FinancialFact,
  NewsItem,
  PricePoint,
  Ratio,
} from './types'

const BASE = '/api'

async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`)
  if (!res.ok) {
    throw new Error(`request failed: ${res.status}`)
  }
  return (await res.json()) as T
}

/** Fetch a company and all of its records in parallel. */
export async function loadCompanyData(ticker: string): Promise<CompanyData> {
  const base = `/companies/${ticker}`
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
