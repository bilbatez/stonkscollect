export interface Company {
  id: number
  cik: string
  ticker: string
  name: string
  exchange: string | null
  sector: string | null
  industry: string | null
}

export interface PricePoint {
  company_id: number
  date: string
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

export interface Ratio {
  company_id: number
  period_end: string
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

export interface CompanyData {
  company: Company
  prices: PricePoint[]
  facts: FinancialFact[]
  ratios: Ratio[]
  news: NewsItem[]
  discrepancies: Discrepancy[]
}
