export interface Company {
  id: number
  cik: string
  ticker: string
  name: string
  exchange: string | null
  sector: string | null
  industry: string | null
  description: string | null
  website: string | null
  employees?: number | null
}

export interface PricePoint {
  company_id: number
  date: string
  open?: number | null
  high?: number | null
  low?: number | null
  close: number
  volume: number | null
  source: string
}

export interface FinancialFact {
  company_id: number
  statement: string
  line_item: string
  period_type: string
  period_end: string
  value: number
  source: string
  fetched_at: string
}

export type Period = 'annual' | 'quarterly'

export interface Ratio {
  company_id: number
  period_end: string
  period_type: Period
  metric: string
  value: number
  computed_at: string
}

export interface NewsItem {
  company_id: number
  title: string
  description: string | null
  url: string
  source: string
  published_at: string
  dedup_hash: string
}

export interface Discrepancy {
  company_id: number
  field: string
  period: string | null
  source_a: string
  value_a: number
  source_b: string
  value_b: number
  pct_diff: number
  flagged_at: string
}

export interface GrahamCriterion {
  name: string
  passed: boolean
  detail: string
}

export interface GrahamAssessment {
  criteria: GrahamCriterion[]
  score: number
  graham_number: number | null
  ncav_per_share: number | null
  margin_of_safety: number | null
  net_net: boolean
  passes_defensive: boolean
}

export interface GrahamScore {
  company_id: number
  score: number
  passes_defensive: boolean
  graham_number: number | null
  ncav_per_share: number | null
  margin_of_safety: number | null
  net_net: boolean
  computed_at: string
}

export interface ScreenRow {
  company: Company
  score: GrahamScore
}

/** A company in the directory, with its Graham score when computed. */
export interface CompanyRow {
  company: Company
  score: GrahamScore | null
}

/** A page of results plus the total match count. */
export interface Page<T> {
  rows: T[]
  total: number
}

export interface ScreenFilters {
  defensive?: boolean
  net_net?: boolean
  min_score?: number
  sector?: string
  min_pe?: number
  max_pe?: number
  min_roe?: number
  max_de?: number
  min_margin?: number
  sort_by?: string
  sort_dir?: string
  limit?: number
  offset?: number
}

export interface PeerRow {
  company: Company
  score: GrahamScore | null
}

export interface Note {
  body: string | null
}

export interface SectorStats {
  sector: string
  company_count: number
  avg_score: number
  pct_defensive: number
  top_ticker: string | null
}

export interface ShareCount {
  company_id: number
  as_of: string
  shares: number
  source: string
}

export interface CompanySummary {
  company: Company
  ratios: Ratio[]
  graham: GrahamScore | null
  shares: ShareCount | null
}

export interface MoverRow {
  company: Company
  last_close: number
  change: number
  change_pct: number
  volume: number | null
  as_of: string
}

export interface Movers {
  gainers: MoverRow[]
  losers: MoverRow[]
  most_active: MoverRow[]
}

export interface WatchQuote {
  company: Company
  last_close: number | null
  change: number | null
  change_pct: number | null
  volume: number | null
  as_of: string | null
}

export interface CompanyData {
  company: Company
  prices: PricePoint[]
  facts: FinancialFact[]
  ratios: Ratio[]
  news: NewsItem[]
  discrepancies: Discrepancy[]
  graham: GrahamAssessment
  peers: PeerRow[]
  note: Note
  shares: ShareCount | null
}
