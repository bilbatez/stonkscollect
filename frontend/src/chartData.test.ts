import { expect, test } from 'vitest'
import {
  grahamChartData,
  incomeChartData,
  movingAverage,
  normalizedReturns,
  pricesForRange,
  ratioChartData,
} from './chartData'
import type { FinancialFact, PricePoint, Ratio } from './types'

const fact = (
  item: string,
  period_end: string,
  value: number,
  statement = 'income',
  period_type = 'annual',
): FinancialFact => ({
  company_id: 1,
  statement,
  line_item: item,
  period_type,
  period_end,
  value,
  source: 'edgar',
  fetched_at: '',
})

const ratio = (metric: string, period_end: string, value: number, period_type = 'annual'): Ratio => ({
  company_id: 1,
  period_end,
  period_type: period_type as 'annual' | 'quarterly',
  metric,
  value,
  computed_at: '',
})

test('incomeChartData extracts Revenue/GrossProfit/NetIncome sorted by date', () => {
  const facts = [
    fact('Revenue', '2023-12-31', 100),
    fact('Revenue', '2022-12-31', 80),
    fact('NetIncome', '2023-12-31', 20),
    fact('LongTermDebt', '2023-12-31', 50, 'balance'), // wrong statement, excluded
  ]
  const d = incomeChartData(facts, 'annual')
  expect(d.categories).toEqual(['2022-12-31', '2023-12-31'])
  expect(d.series.find((s) => s.name === 'Revenue')?.data).toEqual([80, 100])
  expect(d.series.find((s) => s.name === 'Net income')?.data).toEqual([null, 20]) // no 2022 → null
  expect(d.series.find((s) => s.name === 'Gross profit')).toBeUndefined()
})

test('incomeChartData returns empty when no matching facts', () => {
  expect(incomeChartData([], 'annual')).toEqual({ categories: [], series: [] })
  expect(incomeChartData([fact('LongTermDebt', '2023-12-31', 10, 'balance')], 'annual')).toEqual({
    categories: [],
    series: [],
  })
})

test('incomeChartData filters by period type', () => {
  const facts = [
    fact('Revenue', '2023-12-31', 100, 'income', 'annual'),
    fact('Revenue', '2023-09-30', 25, 'income', 'quarterly'),
  ]
  const annual = incomeChartData(facts, 'annual')
  expect(annual.categories).toEqual(['2023-12-31'])
  const quarterly = incomeChartData(facts, 'quarterly')
  expect(quarterly.categories).toEqual(['2023-09-30'])
})

test('ratioChartData extracts known metrics with human labels', () => {
  const ratios = [
    ratio('roe', '2023-12-31', 0.15),
    ratio('roe', '2022-12-31', 0.12),
    ratio('net_margin', '2023-12-31', 0.25),
    ratio('custom_metric', '2023-12-31', 1.5), // not in chart metrics, excluded
  ]
  const d = ratioChartData(ratios, 'annual')
  expect(d.categories).toEqual(['2022-12-31', '2023-12-31'])
  expect(d.series.find((s) => s.name === 'Return on equity')?.data).toEqual([0.12, 0.15])
  expect(d.series.find((s) => s.name === 'Net margin')?.data).toEqual([null, 0.25])
  expect(d.series.find((s) => s.name === 'custom_metric')).toBeUndefined()
})

test('ratioChartData returns empty when no matching ratios', () => {
  expect(ratioChartData([], 'annual')).toEqual({ categories: [], series: [] })
  expect(ratioChartData([ratio('custom', '2023-12-31', 1)], 'annual')).toEqual({
    categories: [],
    series: [],
  })
})

test('ratioChartData filters by period type', () => {
  const ratios = [
    ratio('roe', '2023-12-31', 0.15, 'annual'),
    ratio('roe', '2023-09-30', 0.04, 'quarterly'),
  ]
  expect(ratioChartData(ratios, 'annual').categories).toEqual(['2023-12-31'])
  expect(ratioChartData(ratios, 'quarterly').categories).toEqual(['2023-09-30'])
})

// --- grahamChartData ---

const price = (date: string, close: number): PricePoint => ({
  company_id: 1,
  date,
  close,
  volume: null,
  source: 'fmp',
})

test('grahamChartData computes sqrt(22.5 * eps * bvps), EPS from facts + BVPS from ratios', () => {
  const facts = [
    fact('Eps', '2022-12-31', 4.0),
    fact('Eps', '2023-12-31', 5.0),
    // non-matching facts that must be ignored by the EPS filter:
    fact('NetIncome', '2023-12-31', 50), // wrong line_item
    fact('Eps', '2023-09-30', 9.9, 'income', 'quarterly'), // wrong period_type
    fact('Eps', '2023-12-31', 7.0, 'balance'), // wrong statement
  ]
  const ratios = [
    ratio('book_value_per_share', '2022-12-31', 20.0, 'annual'),
    ratio('book_value_per_share', '2023-12-31', 25.0, 'annual'),
    ratio('book_value_per_share', '2023-09-30', 99.0, 'quarterly'), // wrong period_type, ignored
  ]
  const prices = [
    price('2022-12-31', 100),
    price('2023-06-30', 110), // uses 2022 Graham # (latest period_end <= date)
    price('2023-12-31', 120),
    price('2024-01-15', 130), // uses 2023 Graham #
  ]
  const d = grahamChartData(prices, facts, ratios)
  expect(d).not.toBeNull()
  // Graham # 2022: sqrt(22.5 * 4 * 20) = sqrt(1800) ≈ 42.43
  // Graham # 2023: sqrt(22.5 * 5 * 25) = sqrt(2812.5) ≈ 53.03
  expect(d!.dates).toEqual(['2022-12-31', '2023-06-30', '2023-12-31', '2024-01-15'])
  expect(d!.prices).toEqual([100, 110, 120, 130])
  expect(d!.grahamNumbers[0]).toBeCloseTo(Math.sqrt(22.5 * 4 * 20), 5)
  expect(d!.grahamNumbers[1]).toBeCloseTo(Math.sqrt(22.5 * 4 * 20), 5) // still 2022
  expect(d!.grahamNumbers[2]).toBeCloseTo(Math.sqrt(22.5 * 5 * 25), 5)
  expect(d!.grahamNumbers[3]).toBeCloseTo(Math.sqrt(22.5 * 5 * 25), 5)
})

test('grahamChartData returns null when fewer than 2 computable graham periods', () => {
  // only one period with both eps + bvps → not enough
  const facts = [fact('Eps', '2023-12-31', 5.0)]
  const ratios = [ratio('book_value_per_share', '2023-12-31', 25.0, 'annual')]
  expect(grahamChartData([price('2023-12-31', 100)], facts, ratios)).toBeNull()
})

test('grahamChartData returns null with no prices', () => {
  const facts = [fact('Eps', '2022-12-31', 4.0), fact('Eps', '2023-12-31', 5.0)]
  const ratios = [
    ratio('book_value_per_share', '2022-12-31', 20.0, 'annual'),
    ratio('book_value_per_share', '2023-12-31', 25.0, 'annual'),
  ]
  expect(grahamChartData([], facts, ratios)).toBeNull()
})

test('grahamChartData skips periods where eps or bvps is missing', () => {
  // 2022 has both; 2023 is missing bvps → only 1 valid period → null
  const facts = [fact('Eps', '2022-12-31', 4.0), fact('Eps', '2023-12-31', 5.0)]
  const ratios = [
    ratio('book_value_per_share', '2022-12-31', 20.0, 'annual'),
    // no bvps for 2023
  ]
  expect(
    grahamChartData([price('2022-12-31', 100), price('2023-12-31', 110)], facts, ratios),
  ).toBeNull()
})

test('grahamChartData skips prices with no applicable graham period', () => {
  // price is before any computed graham period → no entry for it
  const facts = [fact('Eps', '2022-12-31', 4.0), fact('Eps', '2023-12-31', 5.0)]
  const ratios = [
    ratio('book_value_per_share', '2022-12-31', 20.0, 'annual'),
    ratio('book_value_per_share', '2023-12-31', 25.0, 'annual'),
  ]
  const prices = [
    price('2021-01-01', 80), // before any graham period
    price('2022-12-31', 100),
    price('2023-12-31', 120),
  ]
  const d = grahamChartData(prices, facts, ratios)
  expect(d).not.toBeNull()
  // 2021-01-01 dropped; only 2 price points remain
  expect(d!.dates).toEqual(['2022-12-31', '2023-12-31'])
})

// --- pricesForRange ---

function rangeBar(date: string): PricePoint {
  return { company_id: 1, date, open: null, high: null, low: null, close: 1, volume: null, source: 'yahoo' }
}

test('pricesForRange windows by preset anchored on the latest price date', () => {
  const prices = [
    rangeBar('2018-06-01'),
    rangeBar('2023-11-15'),
    rangeBar('2024-01-05'),
    rangeBar('2024-02-20'),
    rangeBar('2024-03-01'),
  ]
  expect(pricesForRange(prices, 'MAX')).toHaveLength(5)
  expect(pricesForRange(prices, '1M').map((p) => p.date)).toEqual(['2024-02-20', '2024-03-01'])
  expect(pricesForRange(prices, '6M').map((p) => p.date)).toEqual([
    '2023-11-15', '2024-01-05', '2024-02-20', '2024-03-01',
  ])
  // YTD: only 2024 bars
  expect(pricesForRange(prices, 'YTD').map((p) => p.date)).toEqual([
    '2024-01-05', '2024-02-20', '2024-03-01',
  ])
  expect(pricesForRange(prices, '1Y')).toHaveLength(4)
  expect(pricesForRange(prices, '5Y')).toHaveLength(4) // 2018 bar is older than 5y
})

// --- movingAverage ---

test('movingAverage returns null until the window fills, then trailing mean of close', () => {
  const prices = [
    price('2024-01-01', 10),
    price('2024-01-02', 20),
    price('2024-01-03', 30),
    price('2024-01-04', 40),
  ]
  expect(movingAverage(prices, 3)).toEqual([null, null, 20, 30])
})

test('movingAverage with window 1 just echoes closes', () => {
  expect(movingAverage([price('2024-01-01', 5), price('2024-01-02', 7)], 1)).toEqual([5, 7])
})

test('movingAverage returns all nulls when fewer bars than the window', () => {
  expect(movingAverage([price('2024-01-01', 5), price('2024-01-02', 7)], 3)).toEqual([null, null])
})

test('movingAverage on empty input is empty', () => {
  expect(movingAverage([], 50)).toEqual([])
})

// --- normalizedReturns ---

test('normalizedReturns rebases each ticker to % change from the first common date', () => {
  const d = normalizedReturns({
    AAPL: [price('2024-01-01', 100), price('2024-01-02', 110), price('2024-01-03', 120)],
    MSFT: [price('2024-01-02', 200), price('2024-01-03', 220), price('2024-01-04', 230)],
  })
  expect(d.categories).toEqual(['2024-01-01', '2024-01-02', '2024-01-03', '2024-01-04'])
  const aapl = d.series.find((s) => s.name === 'AAPL')!.data
  const msft = d.series.find((s) => s.name === 'MSFT')!.data
  // common base date is 2024-01-02 (AAPL base 110, MSFT base 200)
  expect(aapl[0]).toBeNull() // before the shared base
  expect(aapl[1]).toBeCloseTo(0)
  expect(aapl[2]).toBeCloseTo((120 / 110 - 1) * 100)
  expect(aapl[3]).toBeNull() // AAPL has no 2024-01-04 bar
  expect(msft[1]).toBeCloseTo(0)
  expect(msft[2]).toBeCloseTo(10)
  expect(msft[3]).toBeCloseTo(15)
})

test('normalizedReturns is empty without input or a shared base date', () => {
  expect(normalizedReturns({})).toEqual({ categories: [], series: [] })
  expect(
    normalizedReturns({ AAPL: [price('2024-01-01', 100)], MSFT: [price('2024-02-01', 200)] }),
  ).toEqual({ categories: [], series: [] })
})

test('pricesForRange handles empty input and unordered dates', () => {
  expect(pricesForRange([], '1M')).toEqual([])
  // anchor is the max date even when bars arrive unordered
  const shuffled = [rangeBar('2024-03-01'), rangeBar('2020-01-01')]
  expect(pricesForRange(shuffled, '1Y').map((p) => p.date)).toEqual(['2024-03-01'])
})
