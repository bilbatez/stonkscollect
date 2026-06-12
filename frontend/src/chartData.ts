import type { FinancialFact, PricePoint, Ratio } from './types'
import { GRAHAM_FORMULA_MULTIPLE } from './constants'
import { metricLabel } from './format'

export interface ChartSeries {
  name: string
  data: (number | null)[]
}

export interface ChartData {
  categories: string[]
  series: ChartSeries[]
}

const INCOME_ITEMS = ['Revenue', 'GrossProfit', 'NetIncome'] as const
const INCOME_LABELS: Record<string, string> = {
  Revenue: 'Revenue',
  GrossProfit: 'Gross profit',
  NetIncome: 'Net income',
}
const RATIO_METRICS = ['roe', 'net_margin', 'current_ratio', 'debt_to_equity', 'pe', 'pb'] as const

/** Extract Revenue / GrossProfit / NetIncome from facts for a grouped bar chart. */
export function incomeChartData(facts: FinancialFact[], period: 'annual' | 'quarterly'): ChartData {
  const filtered = facts.filter(
    (f) =>
      f.period_type === period &&
      (INCOME_ITEMS as readonly string[]).includes(f.line_item) &&
      f.statement === 'income',
  )
  if (filtered.length === 0) return { categories: [], series: [] }
  const categories = [...new Set(filtered.map((f) => f.period_end))].sort()
  const byItem = new Map<string, Map<string, number>>()
  for (const f of filtered) {
    if (!byItem.has(f.line_item)) byItem.set(f.line_item, new Map())
    byItem.get(f.line_item)!.set(f.period_end, f.value)
  }
  const series: ChartSeries[] = (INCOME_ITEMS as readonly string[])
    .filter((item) => byItem.has(item))
    .map((item) => ({
      name: INCOME_LABELS[item],
      data: categories.map((c) => byItem.get(item)?.get(c) ?? null),
    }))
  return { categories, series }
}

export interface GrahamChartData {
  dates: string[]
  prices: number[]
  grahamNumbers: number[]
}

/**
 * Align historical prices with a Graham Number computed per annual period.
 * Graham Number = sqrt(22.5 * EPS * BookValuePerShare).
 *
 * EPS is an annual income-statement *fact* (not a ratio); BVPS is the
 * `book_value_per_share` ratio. Each price point uses the most-recently-computed
 * Graham Number whose period_end is <= the price date. Returns null if fewer
 * than 2 computable Graham Number periods exist or no prices remain after
 * filtering.
 */
export function grahamChartData(
  prices: PricePoint[],
  facts: FinancialFact[],
  ratios: Ratio[],
): GrahamChartData | null {
  const epsByPeriod = new Map<string, number>()
  for (const f of facts) {
    if (f.line_item === 'Eps' && f.statement === 'income' && f.period_type === 'annual') {
      epsByPeriod.set(f.period_end, f.value)
    }
  }
  const bvpsByPeriod = new Map<string, number>()
  for (const r of ratios) {
    if (r.metric === 'book_value_per_share' && r.period_type === 'annual') {
      bvpsByPeriod.set(r.period_end, r.value)
    }
  }
  // Compute a Graham Number for each period that has both metrics
  const grahamByPeriod = new Map<string, number>()
  for (const [period, eps] of epsByPeriod) {
    const bvps = bvpsByPeriod.get(period)
    if (bvps !== undefined) {
      grahamByPeriod.set(period, Math.sqrt(GRAHAM_FORMULA_MULTIPLE * eps * bvps))
    }
  }
  if (grahamByPeriod.size < 2) return null
  const sortedPeriods = [...grahamByPeriod.keys()].sort()

  const dates: string[] = []
  const priceValues: number[] = []
  const grahamValues: number[] = []

  const sorted = [...prices].sort((a, b) => a.date.localeCompare(b.date))
  for (const p of sorted) {
    // Last period whose end date <= price date
    const applicable = sortedPeriods.filter((period) => period <= p.date)
    if (applicable.length === 0) continue
    const latestPeriod = applicable[applicable.length - 1]
    dates.push(p.date)
    priceValues.push(p.close)
    grahamValues.push(grahamByPeriod.get(latestPeriod)!)
  }

  if (dates.length === 0) return null
  return { dates, prices: priceValues, grahamNumbers: grahamValues }
}

/** Extract key ratios over time for a multi-line trend chart. */
export function ratioChartData(ratios: Ratio[], period: 'annual' | 'quarterly'): ChartData {
  const filtered = ratios.filter(
    (r) => r.period_type === period && (RATIO_METRICS as readonly string[]).includes(r.metric),
  )
  if (filtered.length === 0) return { categories: [], series: [] }
  const categories = [...new Set(filtered.map((r) => r.period_end))].sort()
  const byMetric = new Map<string, Map<string, number>>()
  for (const r of filtered) {
    if (!byMetric.has(r.metric)) byMetric.set(r.metric, new Map())
    byMetric.get(r.metric)!.set(r.period_end, r.value)
  }
  const series: ChartSeries[] = (RATIO_METRICS as readonly string[])
    .filter((m) => byMetric.has(m))
    .map((m) => ({
      name: metricLabel(m),
      data: categories.map((c) => byMetric.get(m)?.get(c) ?? null),
    }))
  return { categories, series }
}

/** Price-chart range presets, Yahoo-style. */
export type RangePreset = '1M' | '6M' | 'YTD' | '1Y' | '5Y' | 'MAX'

export const RANGE_PRESETS: readonly RangePreset[] = ['1M', '6M', 'YTD', '1Y', '5Y', 'MAX']

const RANGE_MONTHS: Record<Exclude<RangePreset, 'YTD' | 'MAX'>, number> = {
  '1M': 1,
  '6M': 6,
  '1Y': 12,
  '5Y': 60,
}

/** Bars within `preset`, anchored on the latest stored price date (data is
 *  latest-and-stored — anchoring on the wall clock would empty short ranges
 *  whenever collection lags). */
export function pricesForRange(prices: PricePoint[], preset: RangePreset): PricePoint[] {
  if (preset === 'MAX' || prices.length === 0) return prices
  const anchor = prices.reduce((max, p) => (p.date > max ? p.date : max), prices[0].date)
  const start =
    preset === 'YTD' ? `${anchor.slice(0, 4)}-01-01` : monthsBefore(anchor, RANGE_MONTHS[preset])
  return prices.filter((p) => p.date >= start)
}

function monthsBefore(date: string, months: number): string {
  const d = new Date(`${date}T00:00:00Z`)
  d.setUTCMonth(d.getUTCMonth() - months)
  return d.toISOString().slice(0, 10)
}
