/** Format a USD amount with a B/M suffix for large values. */
export function formatCurrency(value: number): string {
  const sign = value < 0 ? '-' : ''
  const abs = Math.abs(value)
  if (abs >= 1_000_000_000) {
    return `${sign}$${(abs / 1_000_000_000).toFixed(1)}B`
  }
  if (abs >= 1_000_000) {
    return `${sign}$${(abs / 1_000_000).toFixed(1)}M`
  }
  return `${sign}$${abs}`
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

export type Freshness = 'fresh' | 'stale' | 'unknown'

/** Classify how fresh a timestamp is relative to `nowMs` (ms since epoch). */
export function freshness(iso: string | null, nowMs: number): Freshness {
  if (iso === null) {
    return 'unknown'
  }
  const ageDays = (nowMs - Date.parse(iso)) / 86_400_000
  return ageDays < 2 ? 'fresh' : 'stale'
}
