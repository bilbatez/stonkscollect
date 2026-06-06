import { expect, test } from 'vitest'
import { formatCurrency, freshness } from './format'

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
