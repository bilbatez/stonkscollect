import { expect, test } from 'vitest'
import { computeKeyStats, computeQuote, dedupeDaily } from './quote'
import type { CompanyData, FinancialFact, PricePoint } from './types'

function bar(date: string, close: number, extra: Partial<PricePoint> = {}): PricePoint {
  return {
    company_id: 1,
    date,
    open: null,
    high: null,
    low: null,
    close,
    volume: null,
    source: 'yahoo',
    ...extra,
  }
}

function fact(item: string, value: number, periodEnd = '2023-12-31'): FinancialFact {
  return {
    company_id: 1,
    statement: 'income',
    line_item: item,
    period_type: 'annual',
    period_end: periodEnd,
    value,
    source: 'edgar',
    fetched_at: '2024-01-01T00:00:00Z',
  }
}

function data(overrides: Partial<CompanyData>): CompanyData {
  return {
    company: {
      id: 1,
      cik: '1',
      ticker: 'AAPL',
      name: 'Apple',
      exchange: null,
      sector: null,
      industry: null,
      description: null,
      website: null,
      employees: null,
    },
    prices: [],
    facts: [],
    ratios: [],
    news: [],
    discrepancies: [],
    graham: {
      criteria: [],
      score: 0,
      graham_number: null,
      ncav_per_share: null,
      margin_of_safety: null,
      net_net: false,
      passes_defensive: false,
    },
    peers: [],
    note: { body: null },
    shares: null,
    ...overrides,
  }
}

test('dedupeDaily keeps one bar per date preferring yahoo, then fmp, then others', () => {
  const days = dedupeDaily([
    bar('2024-01-03', 3, { source: 'zeta' }),
    bar('2024-01-03', 2, { source: 'alpha' }),
    bar('2024-01-02', 10, { source: 'fmp' }),
    bar('2024-01-02', 11, { source: 'yahoo' }),
    bar('2024-01-01', 5, { source: 'fmp' }),
  ])
  expect(days.map((d) => d.date)).toEqual(['2024-01-01', '2024-01-02', '2024-01-03'])
  expect(days[0].close).toBe(5) // fmp wins when alone
  expect(days[1].close).toBe(11) // yahoo beats fmp
  expect(days[2].close).toBe(2) // unknown sources tie-break alphabetically

  // a lower-ranked bar arriving after the winner is ignored
  const lateLoser = dedupeDaily([
    bar('2024-01-02', 11, { source: 'yahoo' }),
    bar('2024-01-02', 10, { source: 'fmp' }),
  ])
  expect(lateLoser[0].close).toBe(11)
})

test('computeQuote returns null without prices and degrades with a single bar', () => {
  expect(computeQuote([])).toBeNull()
  const q = computeQuote([bar('2024-03-01', 50, { volume: 7 })])!
  expect(q.last).toBe(50)
  expect(q.asOf).toBe('2024-03-01')
  expect(q.prevClose).toBeNull()
  expect(q.change).toBeNull()
  expect(q.changePct).toBeNull()
  expect(q.week52High).toBe(50)
  expect(q.week52Low).toBe(50)
  expect(q.volume).toBe(7)
  expect(q.avgVolume3m).toBe(7)
})

test('computeQuote derives change, day range, 52-week range, and 3-month volume', () => {
  const q = computeQuote([
    // older than 52 weeks from the last bar: excluded from the range
    bar('2022-06-01', 999, { high: 1000, low: 998 }),
    // inside 52 weeks but older than 3 months: counts for the range, not volume
    bar('2023-09-01', 90, { high: 95, low: 85, volume: 999_999 }),
    bar('2024-02-01', 100, { volume: 300 }),
    bar('2024-03-01', 110, { open: 105, high: 112, low: 104, volume: 200 }),
  ])!
  expect(q.last).toBe(110)
  expect(q.prevClose).toBe(100)
  expect(q.change).toBe(10)
  expect(q.changePct).toBeCloseTo(0.1)
  expect(q.dayHigh).toBe(112)
  expect(q.dayLow).toBe(104)
  expect(q.week52High).toBe(112) // 95 from September, 112 from March; 1000 too old
  expect(q.week52Low).toBe(85)
  expect(q.volume).toBe(200)
  expect(q.avgVolume3m).toBe(250) // (300 + 200) / 2; September is outside 3 months
})

test('computeQuote normalizes bars whose optional OHLC keys are absent', () => {
  const sparse = { company_id: 1, date: '2024-01-02', close: 9, source: 'fmp' } as PricePoint
  const q = computeQuote([sparse])!
  expect(q.dayHigh).toBeNull()
  expect(q.dayLow).toBeNull()
  expect(q.volume).toBeNull()
  expect(q.avgVolume3m).toBeNull()
  expect(q.week52High).toBe(9)
})

test('computeQuote leaves change null when the previous close is zero', () => {
  const q = computeQuote([bar('2024-01-01', 0), bar('2024-01-02', 5)])!
  expect(q.change).toBeNull()
  expect(q.changePct).toBeNull()
  expect(q.prevClose).toBe(0)
})

test('computeKeyStats uses the summary share count first for market cap', () => {
  const d = data({
    shares: { company_id: 1, as_of: '2024-01-31', shares: 1000, source: 'edgar' },
    facts: [fact('SharesOutstanding', 500)],
  })
  const s = computeKeyStats(d, computeQuote([bar('2024-03-01', 10)]))
  expect(s.sharesOutstanding).toBe(1000)
  expect(s.marketCap).toBe(10_000)
})

test('computeKeyStats falls back through fact share counts and tolerates none', () => {
  const viaIncome = computeKeyStats(
    data({ facts: [fact('SharesOutstanding', 500), fact('SharesOutstandingBalance', 400)] }),
    computeQuote([bar('2024-03-01', 10)]),
  )
  expect(viaIncome.sharesOutstanding).toBe(500)
  const viaBalance = computeKeyStats(
    data({ facts: [fact('SharesOutstandingBalance', 400)] }),
    computeQuote([bar('2024-03-01', 10)]),
  )
  expect(viaBalance.sharesOutstanding).toBe(400)
  expect(viaBalance.marketCap).toBe(4000)
  const none = computeKeyStats(data({}), null)
  expect(none.sharesOutstanding).toBeNull()
  expect(none.marketCap).toBeNull()
  expect(none.dividendYield).toBeNull()
})

test('computeKeyStats picks the newest fact and computes dividend yield', () => {
  const d = data({
    facts: [
      // newest first, so the older row must NOT displace it
      fact('Eps', 6, '2023-12-31'),
      fact('Eps', 5, '2022-12-31'),
      fact('DividendPerShare', 1.2),
      fact('PublicFloat', 9_999),
    ],
    ratios: [
      {
        company_id: 1,
        period_end: '2023-12-31',
        period_type: 'annual',
        metric: 'pe',
        value: 20,
        computed_at: '2024-01-01T00:00:00Z',
      },
      {
        company_id: 1,
        period_end: '2022-12-31',
        period_type: 'annual',
        metric: 'pe',
        value: 30, // older annual pe must lose to the 2023 one
        computed_at: '2024-01-01T00:00:00Z',
      },
      {
        company_id: 1,
        period_end: '2023-12-31',
        period_type: 'quarterly',
        metric: 'pb',
        value: 9,
        computed_at: '2024-01-01T00:00:00Z',
      },
    ],
    company: { ...data({}).company, employees: 164_000 },
  })
  const s = computeKeyStats(d, computeQuote([bar('2024-03-01', 60)]))
  expect(s.eps).toBe(6)
  expect(s.dividendRate).toBe(1.2)
  expect(s.dividendYield).toBeCloseTo(0.02) // 1.2 / 60
  expect(s.publicFloat).toBe(9_999)
  expect(s.pe).toBe(20)
  expect(s.pb).toBeNull() // quarterly ratios don't count
  expect(s.payoutRatio).toBeNull()
  expect(s.employees).toBe(164_000)
})
