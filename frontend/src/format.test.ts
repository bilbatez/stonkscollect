import { expect, test } from 'vitest'
import {
  formatCurrency,
  formatMetric,
  freshness,
  metricGroup,
  metricLabel,
  secFilingsUrl,
  statementItemLabel,
  statementLabel,
  wikipediaUrl,
  yahooProfileUrl,
} from './format'

test('formatCurrency scales to B/M and handles small + negative values', () => {
  expect(formatCurrency(383_285_000_000)).toBe('$383.3B')
  expect(formatCurrency(96_995_000)).toBe('$97.0M')
  expect(formatCurrency(950)).toBe('$950')
  expect(formatCurrency(-2_000_000)).toBe('-$2.0M')
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
})

test('metric labels and groups fall back gracefully', () => {
  expect(metricLabel('roe')).toBe('Return on equity')
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

test('statement labels resolve sections and line items with fallbacks', () => {
  expect(statementItemLabel('NetIncome')).toBe('Net income')
  expect(statementItemLabel('WeirdConceptName')).toBe('Weird Concept Name')
  expect(statementLabel('income')).toBe('Income statement')
  expect(statementLabel('balance')).toBe('Balance sheet')
  expect(statementLabel('cashflow')).toBe('Cash flow')
  expect(statementLabel('segments')).toBe('Segments')
})
