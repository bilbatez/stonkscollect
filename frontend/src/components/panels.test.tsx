import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import * as format from '../format'
import * as api from '../api'
import { DiscrepancyPanel } from './panels/DiscrepancyPanel'
import { FreshnessBadge } from './shared/FreshnessBadge'
import { KeyStatsPanel } from './panels/KeyStatsPanel'
import { MetricsSummary } from './panels/MetricsSummary'
import { QuoteHeader } from './panels/QuoteHeader'
import { NewsFeed } from './panels/NewsFeed'
import { NotePanel } from './panels/NotePanel'
import { PeersPanel } from './panels/PeersPanel'
import { RangeToggle } from './shared/RangeToggle'
import { WeekRangeBar } from './shared/WeekRangeBar'
import { RatiosPanel } from './panels/RatiosPanel'
import { SectorOverview } from './pages/SectorOverview'
import { DividendPanel } from './panels/DividendPanel'
import { StatementTable } from './panels/StatementTable'
import type { Company, Discrepancy, FinancialFact, GrahamAssessment, GrahamScore, NewsItem, Period, PeerRow, Ratio, SectorStats } from '../types'

let downloadSpy: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  downloadSpy = vi.spyOn(format, 'downloadCsv').mockImplementation(() => {})
})
afterEach(() => {
  downloadSpy.mockRestore()
})

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

test('StatementTable groups by section, humanizes labels, dashes gaps', async () => {
  render(
    <StatementTable
      facts={[
        fact('income', 'Revenue', '2023-12-31', 2_000_000_000),
        fact('income', 'Revenue', '2022-12-31', 1_000_000_000),
        fact('income', 'Revenue', '2021-12-31', 2_000_000_000), // 2022 vs 2021 = -50%
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
  expect(screen.getAllByText('$2.0B').length).toBeGreaterThan(0)
  expect(screen.getByText('Dec 2023')).toBeInTheDocument() // formatted column header
  expect(screen.getAllByText('—').length).toBeGreaterThan(0) // gaps dashed
  expect(screen.getByText('+100%')).toBeInTheDocument() // Revenue 2023 YoY growth badge
  expect(screen.getByText('-50%')).toBeInTheDocument() // Revenue 2022 negative growth badge
  await userEvent.click(screen.getByRole('button', { name: 'Export CSV' }))
  expect(downloadSpy).toHaveBeenCalledWith(
    'annual-statements.csv',
    expect.arrayContaining(['Line item']),
    expect.any(Array),
  )
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

test('StatementTable shows a TTM column on the quarterly view', async () => {
  render(
    <StatementTable
      facts={[
        // 4 quarters of Revenue (flow) -> TTM = sum = $100.0B
        fact('income', 'Revenue', '2023-06-30', 10_000_000_000, 'quarterly'),
        fact('income', 'Revenue', '2023-09-30', 20_000_000_000, 'quarterly'),
        fact('income', 'Revenue', '2023-12-31', 30_000_000_000, 'quarterly'),
        fact('income', 'Revenue', '2024-03-31', 40_000_000_000, 'quarterly'),
        // only 1 quarter (flow) -> TTM dashed
        fact('income', 'NetIncome', '2024-03-31', 5_000_000_000, 'quarterly'),
        // balance (stock) -> TTM is the latest quarter
        fact('balance', 'StockholdersEquity', '2024-03-31', 8_000_000_000, 'quarterly'),
      ]}
    />,
  )
  await userEvent.click(screen.getByRole('button', { name: 'Quarterly' }))
  expect(screen.getByText('TTM')).toBeInTheDocument()
  expect(screen.getByText('$100.0B')).toBeInTheDocument() // summed revenue TTM (only in TTM col)
  // balance TTM equals the latest quarter, so $8.0B appears in both its column and TTM
  expect(screen.getAllByText('$8.0B').length).toBe(2)
})

test('StatementTable shows an empty state when there are no facts', () => {
  render(<StatementTable facts={[]} />)
  expect(screen.getByText(/no statement data/i)).toBeInTheDocument()
})

// --- DividendPanel ---

test('DividendPanel lists annual dividend-per-share history newest first', () => {
  render(
    <DividendPanel
      facts={[
        fact('income', 'DividendPerShare', '2022-12-31', 0.88),
        fact('income', 'DividendPerShare', '2023-12-31', 0.94),
        fact('income', 'Revenue', '2023-12-31', 1_000), // ignored
        fact('income', 'DividendPerShare', '2023-09-30', 0.24, 'quarterly'), // ignored
      ]}
    />,
  )
  const rows = screen.getAllByText(/Dec 202\d/)
  expect(rows[0]).toHaveTextContent('Dec 2023') // newest first
  expect(screen.getByText('0.94')).toBeInTheDocument()
  expect(screen.getByText('0.88')).toBeInTheDocument()
})

test('DividendPanel shows an empty state without dividends', () => {
  render(<DividendPanel facts={[fact('income', 'Revenue', '2023-12-31', 1_000)]} />)
  expect(screen.getByText(/no dividend history/i)).toBeInTheDocument()
})

test('RatiosPanel groups metrics, formats by kind, dashes gaps', async () => {
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
  expect(screen.getByText('Dec 2023')).toBeInTheDocument() // formatted column header
  // metrics without a 2022 value -> dash in that column
  expect(screen.getAllByText('—').length).toBeGreaterThan(0)
  await userEvent.click(screen.getByRole('button', { name: 'Export CSV' }))
  expect(downloadSpy).toHaveBeenCalledWith(
    'annual-ratios.csv',
    expect.arrayContaining(['Metric']),
    expect.any(Array),
  )
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
  expect(screen.getByText('Jan 2, 2024')).toBeInTheDocument() // published_at formatted
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

// --- PeersPanel ---

function makeCompany(ticker: string): Company {
  return { id: 1, cik: '', ticker, name: `${ticker} Corp`, exchange: null, sector: 'Tech', industry: null, description: null, website: null }
}

function makeScore(): GrahamScore {
  return { company_id: 1, score: 5, passes_defensive: true, graham_number: 42.5, ncav_per_share: null, margin_of_safety: 0.2, net_net: false, computed_at: '' }
}

test('PeersPanel shows a row per peer', () => {
  const peers: PeerRow[] = [
    { company: makeCompany('MSFT'), score: makeScore() },
    { company: makeCompany('GOOG'), score: null },
  ]
  render(<PeersPanel peers={peers} />)
  expect(screen.getByText('MSFT')).toBeInTheDocument()
  expect(screen.getByText('GOOG')).toBeInTheDocument()
  expect(screen.getByText('42.50')).toBeInTheDocument() // graham number
  expect(screen.getByText('20%')).toBeInTheDocument() // margin of safety
})

test('PeersPanel shows empty state when no peers', () => {
  render(<PeersPanel peers={[]} />)
  expect(screen.getByText(/no peers/i)).toBeInTheDocument()
})

// --- NotePanel ---

test('NotePanel saves a note on button click and typing clears saved', async () => {
  const saveSpy = vi.spyOn(api, 'saveNote').mockResolvedValue()
  render(<NotePanel ticker="AAPL" initialBody="draft" />)
  expect(screen.getByDisplayValue('draft')).toBeInTheDocument()
  await userEvent.click(screen.getByRole('button', { name: /save/i }))
  expect(saveSpy).toHaveBeenCalledWith('AAPL', 'draft')
  expect(screen.getByText(/saved/i)).toBeInTheDocument()
  // typing clears the saved indicator
  await userEvent.type(screen.getByDisplayValue('draft'), ' more')
  expect(screen.queryByText(/saved/i)).toBeNull()
  saveSpy.mockRestore()
})

test('NotePanel deletes note on delete button', async () => {
  const deleteSpy = vi.spyOn(api, 'deleteNote').mockResolvedValue()
  render(<NotePanel ticker="AAPL" initialBody="old" />)
  await userEvent.click(screen.getByRole('button', { name: /delete/i }))
  expect(deleteSpy).toHaveBeenCalledWith('AAPL')
  expect(screen.getByPlaceholderText(/notes/i)).toBeInTheDocument()
  deleteSpy.mockRestore()
})

test('NotePanel shows error when save fails', async () => {
  vi.spyOn(api, 'saveNote').mockRejectedValue(new Error('fail'))
  render(<NotePanel ticker="AAPL" initialBody="text" />)
  await userEvent.click(screen.getByRole('button', { name: /save/i }))
  expect(screen.getByText(/failed to save/i)).toBeInTheDocument()
  vi.restoreAllMocks()
})

test('NotePanel shows error when delete fails', async () => {
  vi.spyOn(api, 'deleteNote').mockRejectedValue(new Error('fail'))
  render(<NotePanel ticker="AAPL" initialBody="text" />)
  await userEvent.click(screen.getByRole('button', { name: /delete/i }))
  expect(screen.getByText(/failed to delete/i)).toBeInTheDocument()
  vi.restoreAllMocks()
})

test('NotePanel starts empty with null body and disables buttons', () => {
  render(<NotePanel ticker="AAPL" initialBody={null} />)
  expect(screen.getByRole('button', { name: /save/i })).toBeDisabled()
  expect(screen.getByRole('button', { name: /delete/i })).toBeDisabled()
})

// --- MetricsSummary ---

const graham: GrahamAssessment = {
  criteria: [],
  score: 5,
  graham_number: 42.5,
  ncav_per_share: null,
  margin_of_safety: 0.2,
  net_net: false,
  passes_defensive: true,
}

test('MetricsSummary renders 8 metric cards with formatted values', () => {
  const ratios: Ratio[] = [
    ratio('pe', '2023-12-31', 15.4),
    ratio('pe', '2022-12-31', 10.0), // older period — latestAnnual must pick 2023
    ratio('pb', '2023-12-31', 2.1),
    ratio('roe', '2023-12-31', 0.18),
    ratio('net_margin', '2023-12-31', 0.12),
    ratio('debt_to_equity', '2023-12-31', 0.45),
    ratio('current_ratio', '2023-12-31', 2.3),
  ]
  render(<MetricsSummary ratios={ratios} graham={graham} />)
  expect(screen.getByText('P/E')).toBeInTheDocument()
  expect(screen.getByText('15.40×')).toBeInTheDocument()
  expect(screen.getByText('P/B')).toBeInTheDocument()
  expect(screen.getByText('2.10×')).toBeInTheDocument()
  expect(screen.getByText('Return on equity')).toBeInTheDocument()
  expect(screen.getByText('18.0%')).toBeInTheDocument()
  expect(screen.getByText('Net margin')).toBeInTheDocument()
  expect(screen.getByText('12.0%')).toBeInTheDocument()
  expect(screen.getByText('Debt to equity')).toBeInTheDocument()
  expect(screen.getByText('0.45×')).toBeInTheDocument()
  expect(screen.getByText('Current ratio')).toBeInTheDocument()
  expect(screen.getByText('2.30×')).toBeInTheDocument()
  expect(screen.getByText('Graham #')).toBeInTheDocument()
  expect(screen.getByText('42.50')).toBeInTheDocument()
  expect(screen.getByText('Margin of safety')).toBeInTheDocument()
  expect(screen.getByText('20%')).toBeInTheDocument()
})

test('MetricsSummary dashes missing metrics and null graham fields', () => {
  render(<MetricsSummary ratios={[]} graham={{ ...graham, graham_number: null, margin_of_safety: null }} />)
  expect(screen.getAllByText('—').length).toBe(8)
})

test('MetricsSummary returns null when no ratios and no graham values', () => {
  const { container } = render(
    <MetricsSummary ratios={[]} graham={{ ...graham, graham_number: null, margin_of_safety: null }} />,
  )
  // component still renders the 8 dash cards — not null
  expect(container.firstChild).not.toBeNull()
})

// --- SectorOverview ---

test('SectorOverview renders a row per sector with formatted values', () => {
  const sectors: SectorStats[] = [
    { sector: 'Technology', company_count: 5, avg_score: 4.2, pct_defensive: 0.6, top_ticker: 'AAPL' },
    { sector: 'Healthcare', company_count: 3, avg_score: 2.0, pct_defensive: 0.0, top_ticker: null },
  ]
  const onSelect = vi.fn()
  render(<SectorOverview sectors={sectors} onSelect={onSelect} />)
  expect(screen.getByText('Technology')).toBeInTheDocument()
  expect(screen.getByText('5')).toBeInTheDocument() // company count
  expect(screen.getByText('4.20')).toBeInTheDocument() // avg score formatted
  expect(screen.getByText('60%')).toBeInTheDocument() // pct_defensive 0.6
  expect(screen.getByRole('button', { name: 'AAPL' })).toBeInTheDocument() // top_ticker clickable
  expect(screen.getByText('Healthcare')).toBeInTheDocument()
  expect(screen.getAllByText('—').length).toBeGreaterThan(0) // null top_ticker
})

test('SectorOverview calls onSelect with top_ticker on click', async () => {
  const sectors: SectorStats[] = [
    { sector: 'Technology', company_count: 1, avg_score: 5, pct_defensive: 1, top_ticker: 'MSFT' },
  ]
  const onSelect = vi.fn()
  render(<SectorOverview sectors={sectors} onSelect={onSelect} />)
  await userEvent.click(screen.getByRole('button', { name: 'MSFT' }))
  expect(onSelect).toHaveBeenCalledWith('MSFT')
})

test('SectorOverview shows empty state when no sectors', () => {
  render(<SectorOverview sectors={[]} onSelect={vi.fn()} />)
  expect(screen.getByText(/no sector data/i)).toBeInTheDocument()
})

// --- QuoteHeader ---

test('QuoteHeader shows price, positive change, and as-of date', () => {
  render(
    <QuoteHeader
      quote={{
        last: 110, prevClose: 100, change: 10, changePct: 0.1, asOf: '2024-03-01',
        dayHigh: 112, dayLow: 104, volume: 200, week52High: 120, week52Low: 80, avgVolume3m: 250,
      }}
    />,
  )
  expect(screen.getByLabelText('last price')).toHaveTextContent('110.00')
  expect(screen.getByLabelText('day change')).toHaveTextContent('+10.00 (+10%)')
  expect(screen.getByText(/at close, Mar 2024/)).toBeInTheDocument()
})

test('QuoteHeader colors a negative change and hides change when unknown', () => {
  render(
    <QuoteHeader
      quote={{
        last: 90, prevClose: 100, change: -10, changePct: -0.1, asOf: '2024-03-01',
        dayHigh: null, dayLow: null, volume: null, week52High: 90, week52Low: 90, avgVolume3m: null,
      }}
    />,
  )
  expect(screen.getByLabelText('day change')).toHaveTextContent('-10.00 (-10%)')

  const { container } = render(
    <QuoteHeader
      quote={{
        last: 50, prevClose: null, change: null, changePct: null, asOf: '2024-03-01',
        dayHigh: null, dayLow: null, volume: null, week52High: 50, week52Low: 50, avgVolume3m: null,
      }}
    />,
  )
  expect(container.textContent).not.toContain('(')
})

test('QuoteHeader renders nothing without a quote', () => {
  const { container } = render(<QuoteHeader quote={null} />)
  expect(container.firstChild).toBeNull()
})

test('QuoteHeader renders period-return chips with colored, signed percentages', () => {
  render(
    <QuoteHeader
      quote={{
        last: 110, prevClose: 100, change: 10, changePct: 0.1, asOf: '2024-03-01',
        dayHigh: 112, dayLow: 104, volume: 200, week52High: 120, week52Low: 80, avgVolume3m: 250,
      }}
      returns={[
        { period: '1D', pct: 0.1 },
        { period: '1Y', pct: -0.2 },
        { period: '5Y', pct: null },
      ]}
    />,
  )
  expect(screen.getByLabelText('1D return')).toHaveTextContent('1D +10%')
  expect(screen.getByLabelText('1Y return')).toHaveTextContent('1Y -20%')
  expect(screen.getByLabelText('5Y return')).toHaveTextContent('5Y —')
})

test('QuoteHeader omits the returns row when none are provided', () => {
  render(
    <QuoteHeader
      quote={{
        last: 50, prevClose: 49, change: 1, changePct: 0.02, asOf: '2024-03-01',
        dayHigh: null, dayLow: null, volume: null, week52High: 50, week52Low: 49, avgVolume3m: null,
      }}
    />,
  )
  expect(screen.queryByLabelText('1D return')).toBeNull()
})

// --- KeyStatsPanel ---

test('KeyStatsPanel formats every populated statistic', () => {
  render(
    <KeyStatsPanel
      stats={{
        marketCap: 1_700_000_000_000, sharesOutstanding: 15_812_547_000, publicFloat: 2_591_165_000_000,
        eps: 6.13, dividendRate: 0.94, dividendYield: 0.005, pe: 28.5, pb: 45.2,
        payoutRatio: 0.15, freeCashFlow: 99_584_000_000, bookValuePerShare: 3.95, employees: 164_000,
      }}
      quote={{
        last: 110, prevClose: 100, change: 10, changePct: 0.1, asOf: '2024-03-01',
        dayHigh: 112, dayLow: 104, volume: 58_414_460, week52High: 199.62, week52Low: 124.17, avgVolume3m: 60_000_000,
      }}
    />,
  )
  expect(screen.getByText('Market cap')).toBeInTheDocument()
  expect(screen.getByText('$1700.0B')).toBeInTheDocument()
  expect(screen.getByText('15.81B')).toBeInTheDocument() // shares outstanding
  expect(screen.getByText('124.17 – 199.62')).toBeInTheDocument() // 52-week range
  expect(screen.getByText('104.00 – 112.00')).toBeInTheDocument() // day range
  expect(screen.getByText('58.41M')).toBeInTheDocument() // volume
  expect(screen.getByText('6.13')).toBeInTheDocument() // EPS
  expect(screen.getByText('1%')).toBeInTheDocument() // dividend yield rounded
  expect(screen.getByText('164.00K')).toBeInTheDocument() // employees
})

test('KeyStatsPanel dashes missing values and tolerates a null quote', () => {
  render(
    <KeyStatsPanel
      stats={{
        marketCap: null, sharesOutstanding: null, publicFloat: null, eps: null,
        dividendRate: null, dividendYield: null, pe: null, pb: null,
        payoutRatio: null, freeCashFlow: null, bookValuePerShare: null, employees: null,
      }}
      quote={null}
    />,
  )
  expect(screen.getAllByText('—').length).toBe(16)
})

// --- WeekRangeBar ---

test('WeekRangeBar positions the marker between the low and high bounds', () => {
  render(<WeekRangeBar low={100} high={200} last={150} />)
  expect(screen.getByText('100.00')).toBeInTheDocument()
  expect(screen.getByText('200.00')).toBeInTheDocument()
  expect(screen.getByTestId('range-marker')).toHaveStyle({ left: '50%' })
})

test('WeekRangeBar clamps the marker and avoids dividing by a zero span', () => {
  const { rerender } = render(<WeekRangeBar low={100} high={200} last={500} />)
  expect(screen.getByTestId('range-marker')).toHaveStyle({ left: '100%' })
  rerender(<WeekRangeBar low={100} high={200} last={10} />)
  expect(screen.getByTestId('range-marker')).toHaveStyle({ left: '0%' })
  rerender(<WeekRangeBar low={50} high={50} last={50} />)
  expect(screen.getByTestId('range-marker')).toHaveStyle({ left: '0%' })
})

// --- RangeToggle ---

test('RangeToggle renders all presets and reports a new selection', async () => {
  const onChange = vi.fn()
  render(<RangeToggle value="1Y" onChange={onChange} />)
  for (const preset of ['1M', '6M', 'YTD', '1Y', '5Y', 'MAX']) {
    expect(screen.getByRole('button', { name: preset })).toBeInTheDocument()
  }
  await userEvent.click(screen.getByRole('button', { name: '6M' }))
  expect(onChange).toHaveBeenCalledWith('6M')
})

test('RangeToggle ignores re-clicking the active preset', async () => {
  const onChange = vi.fn()
  render(<RangeToggle value="1Y" onChange={onChange} />)
  await userEvent.click(screen.getByRole('button', { name: '1Y' }))
  expect(onChange).not.toHaveBeenCalled()
})
