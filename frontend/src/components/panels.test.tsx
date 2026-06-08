import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { DiscrepancyPanel } from './DiscrepancyPanel'
import { FreshnessBadge } from './FreshnessBadge'
import { NewsFeed } from './NewsFeed'
import { RatiosPanel } from './RatiosPanel'
import { StatementTable } from './StatementTable'
import type { Discrepancy, FinancialFact, NewsItem, Ratio } from '../types'

const fact = (item: string, period: string, value: number): FinancialFact => ({
  company_id: 1,
  statement: 'income',
  line_item: item,
  period_type: 'annual',
  period_end: period,
  value,
  source: 'edgar',
  fetched_at: '2024-01-01T00:00:00Z',
})

test('StatementTable pivots facts and fills missing cells with a dash', () => {
  render(
    <StatementTable
      facts={[
        fact('Revenue', '2023-12-31', 2_000_000_000),
        fact('Revenue', '2022-12-31', 1_000_000_000),
        // NetIncome only in 2023 -> its 2022 cell must render a dash.
        fact('NetIncome', '2023-12-31', 500_000_000),
      ]}
    />,
  )
  expect(screen.getByText('Revenue')).toBeInTheDocument()
  expect(screen.getByText('NetIncome')).toBeInTheDocument()
  expect(screen.getByText('$2.0B')).toBeInTheDocument()
  expect(screen.getByText('2023-12-31')).toBeInTheDocument()
  expect(screen.getByText('2022-12-31')).toBeInTheDocument()
  // the missing NetIncome/2022 cell
  expect(screen.getByText('—')).toBeInTheDocument()
})

test('StatementTable shows an empty state when there are no facts', () => {
  render(<StatementTable facts={[]} />)
  expect(screen.getByText(/no statement data/i)).toBeInTheDocument()
})

test('RatiosPanel lists metrics and values', () => {
  const ratios: Ratio[] = [
    { company_id: 1, period_end: '2023-12-31', metric: 'pe', value: 28.5, computed_at: '' },
  ]
  const { rerender } = render(<RatiosPanel ratios={ratios} />)
  expect(screen.getByText('pe')).toBeInTheDocument()
  expect(screen.getByText('28.5')).toBeInTheDocument()
  rerender(<RatiosPanel ratios={[]} />)
  expect(screen.getByText(/no ratio data/i)).toBeInTheDocument()
})

test('NewsFeed renders headlines with optional descriptions', () => {
  const news: NewsItem[] = [
    { company_id: 1, title: 'Up', description: 'why', url: 'http://a', source: 'reuters', published_at: '2024-01-02T00:00:00Z', dedup_hash: 'h1' },
    { company_id: 1, title: 'Flat', description: null, url: 'http://b', source: 'ap', published_at: '2024-01-01T00:00:00Z', dedup_hash: 'h2' },
  ]
  render(<NewsFeed news={news} />)
  expect(screen.getByText('Up')).toBeInTheDocument()
  expect(screen.getByText('why')).toBeInTheDocument()
  expect(screen.getByText('Flat')).toBeInTheDocument()
  expect(screen.getByText('reuters')).toBeInTheDocument()
})

test('DiscrepancyPanel shows a row per flag and an empty state', () => {
  const d: Discrepancy[] = [
    { company_id: 1, field: 'Revenue', period: '2023-12-31', source_a: 'edgar', value_a: 1, source_b: 'fmp', value_b: 2, pct_diff: 0.5, flagged_at: '' },
    // null period must render a dash
    { company_id: 1, field: 'NetIncome', period: null, source_a: 'edgar', value_a: 1, source_b: 'fmp', value_b: 2, pct_diff: 0.1, flagged_at: '' },
  ]
  const { rerender } = render(<DiscrepancyPanel discrepancies={d} />)
  expect(screen.getByText('Revenue')).toBeInTheDocument()
  expect(screen.getAllByText(/edgar/)[0]).toBeInTheDocument()
  expect(screen.getByText('—')).toBeInTheDocument()
  rerender(<DiscrepancyPanel discrepancies={[]} />)
  expect(screen.getByText(/no discrepancies/i)).toBeInTheDocument()
})

test('FreshnessBadge reflects each status', () => {
  const { rerender } = render(<FreshnessBadge status="fresh" />)
  expect(screen.getByText(/fresh/i)).toBeInTheDocument()
  rerender(<FreshnessBadge status="stale" />)
  expect(screen.getByText(/stale/i)).toBeInTheDocument()
  rerender(<FreshnessBadge status="unknown" />)
  expect(screen.getByText(/unknown/i)).toBeInTheDocument()
})
