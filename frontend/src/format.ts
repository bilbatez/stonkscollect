const MONTHS = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec']

/** "2024-12-31" → "Dec 2024" (period column headers) */
export function formatPeriodDate(iso: string): string {
  const [year, month] = iso.split('-')
  return `${MONTHS[Number(month) - 1]} ${year}`
}

/** ISO datetime → "Jan 2, 2024" (news timestamps, UTC) */
export function formatDateTime(iso: string): string {
  const d = new Date(iso)
  return `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}, ${d.getUTCFullYear()}`
}

/** null-safe percentage: x * 100 at 0 dp, or "—" */
export function formatPct(x: number | null): string {
  return x === null ? '—' : `${(x * 100).toFixed(0)}%`
}

/** null-safe 2 dp number, or "—" */
export function formatNum(x: number | null): string {
  return x === null ? '—' : x.toFixed(2)
}

/** Heatmap cell color for a Graham score (0–8): green whose opacity scales with
 *  the score, clamped to [0, 8]. Used by the sector overview. */
export function scoreHeatColor(score: number): string {
  const alpha = Math.min(1, Math.max(0, score / 8))
  return `rgba(34,197,94,${alpha.toFixed(2)})`
}

/** Escape one CSV cell (wrap in quotes if it contains comma, quote, or newline). */
function csvCell(v: string | number | null): string {
  const s = v === null ? '' : String(v)
  return s.includes(',') || s.includes('"') || s.includes('\n')
    ? `"${s.replace(/"/g, '""')}"`
    : s
}

/** Convert headers + 2D data to a CSV string. */
export function toCsv(headers: string[], rows: (string | number | null)[][]): string {
  return [headers, ...rows].map((r) => r.map(csvCell).join(',')).join('\n')
}

/** Trigger a browser download of a generated CSV file. */
export function downloadCsv(
  filename: string,
  headers: string[],
  rows: (string | number | null)[][],
): void {
  const csv = toCsv(headers, rows)
  const a = document.createElement('a')
  a.href = `data:text/csv;charset=utf-8,${encodeURIComponent(csv)}`
  a.download = filename
  a.click()
}

/** Format a USD amount with a B/M suffix for large values. */
export function formatCurrency(value: number): string {
  if (!Number.isFinite(value)) return '—'
  const sign = value < 0 ? '-' : ''
  const abs = Math.abs(value)
  if (abs >= 1_000_000_000) {
    return `${sign}$${(abs / 1_000_000_000).toFixed(1)}B`
  }
  if (abs >= 1_000_000) {
    return `${sign}$${(abs / 1_000_000).toFixed(1)}M`
  }
  return `${sign}$${abs.toLocaleString('en-US')}`
}

/** Compact plain quantity: 15.81B / 164.00K shares-style numbers (no $). */
export function formatCompact(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return '—'
  const sign = value < 0 ? '-' : ''
  const abs = Math.abs(value)
  if (abs >= 1_000_000_000) return `${sign}${(abs / 1_000_000_000).toFixed(2)}B`
  if (abs >= 1_000_000) return `${sign}${(abs / 1_000_000).toFixed(2)}M`
  if (abs >= 1_000) return `${sign}${(abs / 1_000).toFixed(2)}K`
  return `${sign}${abs.toLocaleString('en-US')}`
}

/** Turn a snake_case / PascalCase key into a spaced, capitalized fallback label. */
function titleize(key: string): string {
  return key
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/_/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase())
}

export type MetricKind = 'percent' | 'ratio' | 'currency' | 'plain'

export interface MetricMeta {
  label: string
  group: string
  kind: MetricKind
}

/** Display metadata for derived ratio metrics: human label, group, value kind. */
export const metricMeta: Record<string, MetricMeta> = {
  net_margin: { label: 'Net margin', group: 'Profitability', kind: 'percent' },
  gross_margin: { label: 'Gross margin', group: 'Profitability', kind: 'percent' },
  operating_margin: { label: 'Operating margin', group: 'Profitability', kind: 'percent' },
  roe: { label: 'Return on equity', group: 'Profitability', kind: 'percent' },
  fcf_margin: { label: 'Free cash flow margin', group: 'Profitability', kind: 'percent' },
  current_ratio: { label: 'Current ratio', group: 'Liquidity', kind: 'ratio' },
  working_capital: { label: 'Working capital', group: 'Liquidity', kind: 'currency' },
  free_cash_flow: { label: 'Free cash flow', group: 'Liquidity', kind: 'currency' },
  debt_to_equity: { label: 'Debt to equity', group: 'Leverage', kind: 'ratio' },
  pe: { label: 'P/E', group: 'Valuation', kind: 'ratio' },
  pb: { label: 'P/B', group: 'Valuation', kind: 'ratio' },
  payout_ratio: { label: 'Payout ratio', group: 'Valuation', kind: 'percent' },
  book_value_per_share: { label: 'Book value / share', group: 'Per share', kind: 'currency' },
}

/** Stable display order for metric groups. */
export const metricGroups = ['Profitability', 'Liquidity', 'Leverage', 'Valuation', 'Per share']

/** Human label for a ratio metric key (falls back to a titleized key). */
export function metricLabel(key: string): string {
  return metricMeta[key]?.label ?? titleize(key)
}

/** Group name for a ratio metric key (unknown keys land in "Other"). */
export function metricGroup(key: string): string {
  return metricMeta[key]?.group ?? 'Other'
}

/** Format a ratio value for humans based on its metric kind. */
export function formatMetric(key: string, value: number): string {
  if (!Number.isFinite(value)) return '—'
  switch (metricMeta[key]?.kind ?? 'plain') {
    case 'percent':
      return `${(value * 100).toFixed(1)}%`
    case 'ratio':
      return `${value.toFixed(2)}×`
    case 'currency':
      return formatCurrency(value)
    default:
      return value.toFixed(2)
  }
}

/** Human labels for statement line-item keys. */
export const lineItemLabel: Record<string, string> = {
  Revenue: 'Revenue',
  NetIncome: 'Net income',
  GrossProfit: 'Gross profit',
  OperatingIncome: 'Operating income',
  Eps: 'EPS (diluted)',
  DividendPerShare: 'Dividend / share',
  TotalAssets: 'Total assets',
  TotalLiabilities: 'Total liabilities',
  StockholdersEquity: "Shareholders' equity",
  CurrentAssets: 'Current assets',
  CurrentLiabilities: 'Current liabilities',
  LongTermDebt: 'Long-term debt',
  CashAndEquivalents: 'Cash & equivalents',
  SharesOutstanding: 'Shares outstanding',
  OperatingCashFlow: 'Operating cash flow',
  InvestingCashFlow: 'Investing cash flow',
  FinancingCashFlow: 'Financing cash flow',
  CapEx: 'Capital expenditure',
  DividendsPaid: 'Dividends paid',
  DepreciationAmortization: 'Depreciation & amortization',
  ResearchAndDevelopment: 'R&D expense',
  SellingGeneralAdmin: 'SG&A expense',
  InterestExpense: 'Interest expense',
  IncomeTaxExpense: 'Income tax expense',
  Goodwill: 'Goodwill',
  IntangibleAssets: 'Intangible assets',
  PropertyPlantEquipment: 'PP&E (net)',
  Inventories: 'Inventories',
  AccountsReceivable: 'Accounts receivable',
  AccountsPayable: 'Accounts payable',
  ShortTermDebt: 'Short-term debt',
  RetainedEarnings: 'Retained earnings',
  SharesOutstandingBalance: 'Shares outstanding (balance)',
}

/** Human label for a statement line item (falls back to a titleized key). */
export function statementItemLabel(key: string): string {
  return lineItemLabel[key] ?? titleize(key)
}

/** Human label for a statement section. */
export function statementLabel(kind: string): string {
  switch (kind) {
    case 'income':
      return 'Income statement'
    case 'balance':
      return 'Balance sheet'
    case 'cashflow':
      return 'Cash flow'
    default:
      return titleize(kind)
  }
}

/** Canonical SEC EDGAR filings ("biography") page for a CIK. */
export function secFilingsUrl(cik: string): string {
  const padded = cik.replace(/\D/g, '').padStart(10, '0')
  return `https://www.sec.gov/cgi-bin/browse-edgar?action=getcompany&CIK=${padded}&type=10-K&dateb=&owner=include&count=40`
}

/** Wikipedia search link for a company name (approximate; no fetch). */
export function wikipediaUrl(name: string): string {
  return `https://en.wikipedia.org/wiki/Special:Search?search=${encodeURIComponent(name)}`
}

/** Yahoo Finance company-profile page for a ticker. */
export function yahooProfileUrl(ticker: string): string {
  return `https://finance.yahoo.com/quote/${encodeURIComponent(ticker)}/profile`
}

export interface GrowthResult {
  pct: string
  positive: boolean
}

/** Year-over-year % change between two values. Returns null if either is null or prior is 0. */
export function calcGrowth(current: number | null, prior: number | null): GrowthResult | null {
  if (current === null || prior === null || prior === 0) return null
  const change = (current - prior) / Math.abs(prior)
  const pct = `${change >= 0 ? '+' : ''}${(change * 100).toFixed(0)}%`
  return { pct, positive: change >= 0 }
}

export type Freshness = 'fresh' | 'stale' | 'unknown'

/** Classify how fresh a timestamp is relative to `nowMs` (ms since epoch). */
export function freshness(iso: string | null, nowMs: number): Freshness {
  if (iso === null) {
    return 'unknown'
  }
  const ageDays = (nowMs - Date.parse(iso)) / 86_400_000
  return ageDays < 2 ? 'fresh' : 'stale'
}
