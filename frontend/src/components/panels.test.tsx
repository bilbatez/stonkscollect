import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, test } from 'vitest'
import { DiscrepancyPanel } from './DiscrepancyPanel'
import { FreshnessBadge } from './FreshnessBadge'
import { NewsFeed } from './NewsFeed'
import { RatiosPanel } from './RatiosPanel'
import { StatementTable } from './StatementTable'
import type { Discrepancy, FinancialFact, NewsItem, Period, Ratio } from '../types'

const fact = (
  statement: string,
  item: string,
  period: string,
  value: number,
  period_type: Period = 'annual',
): FinancialFact => ({
  company_id: 1,
  statement,
  line_item: item,
  period_type,
  period_end: period,
  value,
  source: 'edgar',
  fetched_at: '2024-01-01T00:00:00Z',
})

const ratio = (
  metric: string,
  period: string,
  value: number,
  period_type: Period = 'annual',
): Ratio => ({
  company_id: 1,
  period_end: period,
  period_type,
  metric,
  value,
  computed_at: '',
})

test('StatementTable groups by section, humanizes labels, dashes gaps', () => {
  render(
    <StatementTable
      facts={[
        fact('income', 'Revenue', '2023-12-31', 2_000_000_000),
        fact('income', 'Revenue', '2022-12-31', 1_000_000_000),
        fact('income', 'NetIncome', '2023-12-31', 500_000_000), // 2022 cell dashed
        fact('balance', 'StockholdersEquity', '2023-12-31', 8_000_000_000),
        fact('segments', 'NorthAmerica', '2023-12-31', 1_000_000), // unknown section
      ]}
    />,
  )
  expect(screen.getByText('Income statement')).toBeInTheDocument()
  expect(screen.getByText('Balance sheet')).toBeInTheDocument()
  expect(screen.getByText('Segments')).toBeInTheDocument() // unknown section, titleized
  expect(screen.getByText('Net income')).toBeInTheDocument() // humanized label
  expect(screen.getByText("Shareholders' equity")).toBeInTheDocument()
  expect(screen.getByText('$2.0B')).toBeInTheDocument()
  expect(screen.getAllByText('—').length).toBeGreaterThan(0) // gaps dashed
})

test('StatementTable toggles annual/quarterly and shows an empty period', async () => {
  render(
    <StatementTable
      facts={[fact('income', 'Revenue', '2023-09-30', 80_000_000_000, 'quarterly')]}
    />,
  )
  // default is annual; this company only has a quarterly fact
  expect(screen.getByText(/no annual statement data/i)).toBeInTheDocument()
  await userEvent.click(screen.getByRole('button', { name: 'Quarterly' }))
  expect(screen.getByText('Revenue')).toBeInTheDocument()
  expect(screen.getByText('$80.0B')).toBeInTheDocument()
})

test('StatementTable shows an empty state when there are no facts', () => {
  render(<StatementTable facts={[]} />)
  expect(screen.getByText(/no statement data/i)).toBeInTheDocument()
})

test('RatiosPanel groups metrics, formats by kind, dashes gaps', () => {
  render(
    <RatiosPanel
      ratios={[
        ratio('roe', '2023-12-31', 0.126),
        ratio('current_ratio', '2023-12-31', 2.6858),
        ratio('current_ratio', '2022-12-31', 2.0),
        ratio('book_value_per_share', '2023-12-31', 64.24),
        ratio('mystery_metric', '2023-12-31', 1.5), // -> "Other" group
      ]}
    />,
  )
  expect(screen.getByText('Profitability')).toBeInTheDocument()
  expect(screen.getByText('Liquidity')).toBeInTheDocument()
  expect(screen.getByText('Other')).toBeInTheDocument()
  expect(screen.getByText('Return on equity')).toBeInTheDocument()
  expect(screen.getByText('12.6%')).toBeInTheDocument() // percent
  expect(screen.getByText('2.69×')).toBeInTheDocument() // ratio
  expect(screen.getByText('$64.24')).toBeInTheDocument() // currency (per-share)
  // metrics without a 2022 value -> dash in that column
  expect(screen.getAllByText('—').length).toBeGreaterThan(0)
})

test('RatiosPanel shows empty period, toggles, and ignores re-clicking active', async () => {
  // only quarterly data -> the default annual view is empty
  render(<RatiosPanel ratios={[ratio('roe', '2023-09-30', 0.05, 'quarterly')]} />)
  expect(screen.getByText(/no annual ratio data/i)).toBeInTheDocument()
  await userEvent.click(screen.getByRole('button', { name: 'Quarterly' }))
  expect(screen.getByText('5.0%')).toBeInTheDocument()
  // clicking the already-selected toggle yields null -> ignored, stays quarterly
  await userEvent.click(screen.getByRole('button', { name: 'Quarterly' }))
  expect(screen.getByText('5.0%')).toBeInTheDocument()
})

test('RatiosPanel shows an empty state when there are no ratios', () => {
  render(<RatiosPanel ratios={[]} />)
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
  render(<NewsFeed news={[]} />)
  expect(screen.getByText(/no news/i)).toBeInTheDocument()
})

test('DiscrepancyPanel shows a row per flag and an empty state', async () => {
  const d: Discrepancy[] = [
    { company_id: 1, field: 'Revenue', period: '2023-12-31', source_a: 'edgar', value_a: 1, source_b: 'fmp', value_b: 2, pct_diff: 0.5, flagged_at: '' },
    { company_id: 1, field: 'NetIncome', period: null, source_a: 'edgar', value_a: 1, source_b: 'fmp', value_b: 2, pct_diff: 0.1, flagged_at: '' },
  ]
  const { rerender } = render(<DiscrepancyPanel discrepancies={d} />)
  expect(screen.getByText('Revenue')).toBeInTheDocument()
  expect(screen.getAllByText(/edgar/)[0]).toBeInTheDocument()
  expect(screen.getByText('—')).toBeInTheDocument()
  // sort each sortable column (exercises the sort accessors, incl. null period)
  await userEvent.click(screen.getByText('Field'))
  await userEvent.click(screen.getByText('Period'))
  await userEvent.click(screen.getByText('Diff'))
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
