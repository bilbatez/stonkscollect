// Pure quote + key-statistics derivation from already-loaded company data.

import type { CompanyData, FinancialFact, PricePoint, Ratio } from './types'

/** A Yahoo-style snapshot of the latest stored trading day. */
export interface Quote {
  last: number
  prevClose: number | null
  change: number | null
  changePct: number | null
  asOf: string
  dayHigh: number | null
  dayLow: number | null
  volume: number | null
  week52High: number
  week52Low: number
  avgVolume3m: number | null
}

/** A Yahoo-style key-statistics snapshot. Nulls mean "not collectable yet". */
export interface KeyStats {
  marketCap: number | null
  sharesOutstanding: number | null
  publicFloat: number | null
  eps: number | null
  dividendRate: number | null
  dividendYield: number | null
  pe: number | null
  pb: number | null
  payoutRatio: number | null
  freeCashFlow: number | null
  bookValuePerShare: number | null
  employees: number | null
}

const SOURCE_RANK: Record<string, number> = { yahoo: 0, fmp: 1 }
const sourceRank = (source: string) => SOURCE_RANK[source] ?? 2

const WEEK_52_DAYS = 365
const THREE_MONTHS_DAYS = 91

/** One bar per trading day (sources overlap): yahoo wins over fmp, then the
 *  alphabetically-first remaining source. Result is sorted oldest-first. */
export function dedupeDaily(prices: PricePoint[]): PricePoint[] {
  const byDate = new Map<string, PricePoint>()
  for (const price of prices) {
    const kept = byDate.get(price.date)
    if (!kept || beats(price, kept)) {
      byDate.set(price.date, price)
    }
  }
  return [...byDate.values()].sort((a, b) => a.date.localeCompare(b.date))
}

function beats(candidate: PricePoint, kept: PricePoint): boolean {
  const byRank = sourceRank(candidate.source) - sourceRank(kept.source)
  if (byRank !== 0) return byRank < 0
  return candidate.source < kept.source
}

function isoDaysBefore(date: string, days: number): string {
  const ms = new Date(`${date}T00:00:00Z`).getTime() - days * 86_400_000
  return new Date(ms).toISOString().slice(0, 10)
}

/** Latest-day quote derived from the price history; null without prices. */
export function computeQuote(prices: PricePoint[]): Quote | null {
  const daily = dedupeDaily(prices)
  const last = daily[daily.length - 1]
  if (!last) return null

  const prev = daily.length > 1 ? daily[daily.length - 2] : null
  const change = prev && prev.close !== 0 ? last.close - prev.close : null

  const yearWindow = daily.filter((p) => p.date >= isoDaysBefore(last.date, WEEK_52_DAYS))
  const highs = yearWindow.map((p) => p.high ?? p.close)
  const lows = yearWindow.map((p) => p.low ?? p.close)

  const volumes = daily
    .filter((p) => p.date >= isoDaysBefore(last.date, THREE_MONTHS_DAYS))
    .map((p) => p.volume)
    .filter((v): v is number => v !== null)

  return {
    last: last.close,
    prevClose: prev ? prev.close : null,
    change,
    changePct: change !== null && prev ? change / prev.close : null,
    asOf: last.date,
    dayHigh: last.high,
    dayLow: last.low,
    volume: last.volume,
    week52High: Math.max(...highs),
    week52Low: Math.min(...lows),
    avgVolume3m:
      volumes.length > 0 ? volumes.reduce((sum, v) => sum + v, 0) / volumes.length : null,
  }
}

function latestFact(facts: FinancialFact[], item: string): number | null {
  let best: FinancialFact | null = null
  for (const fact of facts) {
    if (fact.line_item !== item) continue
    if (!best || fact.period_end > best.period_end) best = fact
  }
  return best ? best.value : null
}

function latestAnnualRatio(ratios: Ratio[], metric: string): number | null {
  let best: Ratio | null = null
  for (const ratio of ratios) {
    if (ratio.metric !== metric || ratio.period_type !== 'annual') continue
    if (!best || ratio.period_end > best.period_end) best = ratio
  }
  return best ? best.value : null
}

/** Share count, preferring the collected DEI figure over income-statement and
 *  balance-sheet facts (mirrors the backend's fallback chain). */
function shareCount(data: CompanyData): number | null {
  return (
    data.shares?.shares ??
    latestFact(data.facts, 'SharesOutstanding') ??
    latestFact(data.facts, 'SharesOutstandingBalance')
  )
}

/** Yahoo-style key statistics from facts, ratios, and the latest quote. */
export function computeKeyStats(data: CompanyData, quote: Quote | null): KeyStats {
  const shares = shareCount(data)
  const dividendRate = latestFact(data.facts, 'DividendPerShare')
  const last = quote ? quote.last : null
  return {
    marketCap: shares !== null && last !== null ? shares * last : null,
    sharesOutstanding: shares,
    publicFloat: latestFact(data.facts, 'PublicFloat'),
    eps: latestFact(data.facts, 'Eps'),
    dividendRate,
    dividendYield:
      dividendRate !== null && last !== null && last !== 0 ? dividendRate / last : null,
    pe: latestAnnualRatio(data.ratios, 'pe'),
    pb: latestAnnualRatio(data.ratios, 'pb'),
    payoutRatio: latestAnnualRatio(data.ratios, 'payout_ratio'),
    freeCashFlow: latestAnnualRatio(data.ratios, 'free_cash_flow'),
    bookValuePerShare: latestAnnualRatio(data.ratios, 'book_value_per_share'),
    employees: data.company.employees ?? null,
  }
}
