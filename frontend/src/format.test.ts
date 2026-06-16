import { expect, test, vi } from 'vitest'
import {
  calcGrowth,
  downloadCsv,
  formatCompact,
  formatCurrency,
  formatDateTime,
  formatMetric,
  formatNum,
  formatPct,
  formatPeriodDate,
  freshness,
  metricGroup,
  metricLabel,
  scoreHeatColor,
  secFilingsUrl,
  statementItemLabel,
  statementLabel,
  toCsv,
  wikipediaUrl,
  yahooProfileUrl,
} from './format'

test('formatCurrency scales to B/M and handles small + negative values', () => {
  expect(formatCurrency(383_285_000_000)).toBe('$383.3B')
  expect(formatCurrency(96_995_000)).toBe('$97.0M')
  expect(formatCurrency(950)).toBe('$950')
  expect(formatCurrency(1_234)).toBe('$1,234')
  expect(formatCurrency(-2_000_000)).toBe('-$2.0M')
  // non-finite inputs must not render as literal "Infinity"/"NaN"
  expect(formatCurrency(Infinity)).toBe('—')
  expect(formatCurrency(-Infinity)).toBe('—')
  expect(formatCurrency(NaN)).toBe('—')
})

test('formatCompact scales plain quantities and dashes invalid input', () => {
  expect(formatCompact(15_812_547_000)).toBe('15.81B')
  expect(formatCompact(164_000)).toBe('164.00K')
  expect(formatCompact(2_500)).toBe('2.50K')
  expect(formatCompact(950)).toBe('950')
  expect(formatCompact(-1_500_000)).toBe('-1.50M')
  expect(formatCompact(null)).toBe('—')
  expect(formatCompact(NaN)).toBe('—')
})

test('freshness classifies by age and missing dates', () => {
  const now = Date.parse('2024-01-10T00:00:00Z')
  expect(freshness(null, now)).toBe('unknown')
  expect(freshness('2024-01-09T00:00:00Z', now)).toBe('fresh')
  expect(freshness('2024-01-01T00:00:00Z', now)).toBe('stale')
})

test('formatMetric renders each metric kind', () => {
  expect(formatMetric('roe', 0.126)).toBe('12.6%') // percent
  expect(formatMetric('current_ratio', 2.6858)).toBe('2.69×') // ratio
  expect(formatMetric('free_cash_flow', 1_135_300_000)).toBe('$1.1B') // currency
  expect(formatMetric('mystery_metric', 1.234)).toBe('1.23') // plain fallback
  // non-finite values fall back to a dash for every kind
  expect(formatMetric('roe', NaN)).toBe('—')
  expect(formatMetric('current_ratio', Infinity)).toBe('—')
  expect(formatMetric('mystery_metric', NaN)).toBe('—')
})

test('metric labels and groups fall back gracefully', () => {
  expect(metricLabel('roe')).toBe('Return on equity')
})

test('scoreHeatColor scales green opacity with the Graham score and clamps', () => {
  expect(scoreHeatColor(0)).toBe('rgba(34,197,94,0.00)')
  expect(scoreHeatColor(8)).toBe('rgba(34,197,94,1.00)')
  expect(scoreHeatColor(4)).toBe('rgba(34,197,94,0.50)')
  expect(scoreHeatColor(-3)).toBe('rgba(34,197,94,0.00)') // clamped low
  expect(scoreHeatColor(99)).toBe('rgba(34,197,94,1.00)') // clamped high
  expect(metricGroup('roe')).toBe('Profitability')
  expect(metricLabel('some_new_metric')).toBe('Some New Metric')
  expect(metricGroup('some_new_metric')).toBe('Other')
})

test('reference link builders', () => {
  expect(secFilingsUrl('320193')).toContain('CIK=0000320193')
  expect(secFilingsUrl('0000320193')).toContain('CIK=0000320193')
  expect(wikipediaUrl('Vulcan Materials Co')).toBe(
    'https://en.wikipedia.org/wiki/Special:Search?search=Vulcan%20Materials%20Co',
  )
  expect(yahooProfileUrl('VMC')).toBe('https://finance.yahoo.com/quote/VMC/profile')
})

test('toCsv converts headers and rows to a CSV string', () => {
  expect(toCsv(['A', 'B'], [[1, 2], ['hello', null]])).toBe('A,B\n1,2\nhello,')
  expect(toCsv(['X'], [['with, comma'], ['with "quote"'], ['with\nnewline']])).toBe(
    'X\n"with, comma"\n"with ""quote"""\n"with\nnewline"',
  )
})

test('downloadCsv triggers an anchor click with a data URI', () => {
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
  downloadCsv('out.csv', ['Col'], [[42]])
  expect(click).toHaveBeenCalled()
  click.mockRestore()
})

test('formatPeriodDate humanizes ISO date strings', () => {
  expect(formatPeriodDate('2024-12-31')).toBe('Dec 2024')
  expect(formatPeriodDate('2023-01-01')).toBe('Jan 2023')
})

test('formatDateTime formats ISO datetimes as readable dates', () => {
  expect(formatDateTime('2024-01-02T00:00:00Z')).toBe('Jan 2, 2024')
  expect(formatDateTime('2024-12-31T23:59:59Z')).toBe('Dec 31, 2024')
})

test('formatPct and formatNum handle null and values', () => {
  expect(formatPct(null)).toBe('—')
  expect(formatPct(0.3)).toBe('30%')
  expect(formatNum(null)).toBe('—')
  expect(formatNum(60)).toBe('60.00')
})

test('statement labels resolve sections and line items with fallbacks', () => {
  expect(statementItemLabel('NetIncome')).toBe('Net income')
  expect(statementItemLabel('DepreciationAmortization')).toBe('Depreciation & amortization')
  expect(statementItemLabel('WeirdConceptName')).toBe('Weird Concept Name')
  expect(statementLabel('income')).toBe('Income statement')
  expect(statementLabel('balance')).toBe('Balance sheet')
  expect(statementLabel('cashflow')).toBe('Cash flow')
  expect(statementLabel('segments')).toBe('Segments')
})

test('calcGrowth returns formatted pct and sign', () => {
  expect(calcGrowth(200, 100)).toEqual({ pct: '+100%', positive: true })
  expect(calcGrowth(80, 100)).toEqual({ pct: '-20%', positive: false })
  expect(calcGrowth(100, 100)).toEqual({ pct: '+0%', positive: true })
  // null inputs
  expect(calcGrowth(null, 100)).toBeNull()
  expect(calcGrowth(100, null)).toBeNull()
  // zero prior
  expect(calcGrowth(100, 0)).toBeNull()
  // negative prior (e.g. net loss to profit)
  expect(calcGrowth(50, -100)).toEqual({ pct: '+150%', positive: true })
})
