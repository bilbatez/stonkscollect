//! Core domain models. Pure data + value-object conversions, no I/O.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};

/// One company's latest daily price move.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MoverRow {
    pub company: Company,
    pub last_close: f64,
    pub change: f64,
    pub change_pct: f64,
    pub volume: Option<i64>,
    pub as_of: NaiveDate,
}

/// Market-movers buckets for the dashboard.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Movers {
    pub gainers: Vec<MoverRow>,
    pub losers: Vec<MoverRow>,
    pub most_active: Vec<MoverRow>,
}

/// Rank raw day-change rows into gainers / losers / most-active buckets of at
/// most `limit` rows each. Missing volume sorts last for "most active".
pub fn select_movers(rows: Vec<MoverRow>, limit: usize) -> Movers {
    let top_by = |key: fn(&MoverRow, &MoverRow) -> std::cmp::Ordering| {
        let mut sorted = rows.clone();
        sorted.sort_by(key);
        sorted.truncate(limit);
        sorted
    };
    Movers {
        gainers: top_by(|a, b| b.change_pct.total_cmp(&a.change_pct)),
        losers: top_by(|a, b| a.change_pct.total_cmp(&b.change_pct)),
        most_active: top_by(|a, b| b.volume.unwrap_or(0).cmp(&a.volume.unwrap_or(0))),
    }
}

/// A watchlist row with the company's latest daily quote, when prices exist.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WatchQuote {
    pub company: Company,
    pub last_close: Option<f64>,
    pub change: Option<f64>,
    pub change_pct: Option<f64>,
    pub volume: Option<i64>,
    pub as_of: Option<NaiveDate>,
}

/// A recorded per-source collection failure for a company.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SourceError {
    pub source: String,
    pub message: String,
    pub occurred_at: DateTime<Utc>,
}

/// A point-in-time share count (e.g. from an SEC filing's DEI cover page).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ShareCount {
    pub company_id: i64,
    pub as_of: NaiveDate,
    pub shares: f64,
    pub source: String,
}

/// Share count for a period, preferring the income-statement weighted figure,
/// then the balance-sheet figure, then the DEI cover-page figure.
pub(crate) fn share_count(items: &BTreeMap<&str, f64>) -> Option<f64> {
    ["SharesOutstanding", "SharesOutstandingBalance", "SharesOutstandingDei"]
        .iter()
        .find_map(|key| items.get(key).copied())
}

/// Reporting period of a financial fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PeriodType {
    Quarterly,
    Annual,
}

impl PeriodType {
    /// Canonical lowercase token stored in the database.
    pub fn as_str(&self) -> &'static str {
        match self {
            PeriodType::Quarterly => "quarterly",
            PeriodType::Annual => "annual",
        }
    }

    /// Parse a stored token; `None` if unrecognized.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "quarterly" => Some(PeriodType::Quarterly),
            "annual" => Some(PeriodType::Annual),
            _ => None,
        }
    }
}

/// Which financial statement a fact belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StatementKind {
    Income,
    Balance,
    CashFlow,
}

impl StatementKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            StatementKind::Income => "income",
            StatementKind::Balance => "balance",
            StatementKind::CashFlow => "cashflow",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "income" => Some(StatementKind::Income),
            "balance" => Some(StatementKind::Balance),
            "cashflow" => Some(StatementKind::CashFlow),
            _ => None,
        }
    }
}

/// A company before it has a database-assigned id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCompany {
    pub cik: String,
    pub ticker: String,
    pub name: String,
    pub exchange: Option<String>,
    pub sector: Option<String>,
    pub industry: Option<String>,
}

/// A persisted company.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Company {
    pub id: i64,
    pub cik: String,
    pub ticker: String,
    pub name: String,
    pub exchange: Option<String>,
    pub sector: Option<String>,
    pub industry: Option<String>,
    /// Prose "what it does" (Yahoo assetProfile longBusinessSummary).
    pub description: Option<String>,
    /// Official company website.
    pub website: Option<String>,
    /// Full-time headcount, when a vendor profile reports it.
    pub employees: Option<i64>,
}

/// A partial company-profile enrichment update (from EDGAR + Yahoo). Only the
/// `Some` fields overwrite stored values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompanyProfile {
    pub sector: Option<String>,
    pub industry: Option<String>,
    pub exchange: Option<String>,
    pub website: Option<String>,
    pub description: Option<String>,
    pub employees: Option<i64>,
}

impl CompanyProfile {
    /// Merge `other` (a later, higher-priority source) on top of `self`:
    /// each `Some` field in `other` overrides; `None` fields keep `self`'s value.
    pub fn overlay(mut self, other: CompanyProfile) -> CompanyProfile {
        if other.sector.is_some() {
            self.sector = other.sector;
        }
        if other.industry.is_some() {
            self.industry = other.industry;
        }
        if other.exchange.is_some() {
            self.exchange = other.exchange;
        }
        if other.website.is_some() {
            self.website = other.website;
        }
        if other.description.is_some() {
            self.description = other.description;
        }
        if other.employees.is_some() {
            self.employees = other.employees;
        }
        self
    }
}

/// A single daily price bar from one source. OHLC is optional (older rows /
/// sources may only have close).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PricePoint {
    pub company_id: i64,
    pub date: NaiveDate,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: f64,
    pub volume: Option<i64>,
    pub source: String,
}

/// A single reported financial line-item value from one source.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FinancialFact {
    pub company_id: i64,
    pub statement: StatementKind,
    pub line_item: String,
    pub period_type: PeriodType,
    pub period_end: NaiveDate,
    pub value: f64,
    pub source: String,
    pub fetched_at: DateTime<Utc>,
}

/// A news headline (title + description only).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NewsItem {
    pub company_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub url: String,
    pub source: String,
    pub published_at: DateTime<Utc>,
    pub dedup_hash: String,
}

/// A derived financial ratio for a period.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Ratio {
    pub company_id: i64,
    pub period_end: NaiveDate,
    pub period_type: PeriodType,
    pub metric: String,
    pub value: f64,
    pub computed_at: DateTime<Utc>,
}

/// Persisted summary of a company's Graham defensive-investor assessment.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GrahamScore {
    pub company_id: i64,
    pub score: i64,
    pub passes_defensive: bool,
    pub graham_number: Option<f64>,
    pub ncav_per_share: Option<f64>,
    pub margin_of_safety: Option<f64>,
    pub net_net: bool,
    pub computed_at: DateTime<Utc>,
}

/// A record of one collection attempt, for observability.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CollectionRun {
    pub id: i64,
    pub source: String,
    pub scope: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: String,
    pub error: Option<String>,
}

/// A flagged cross-source mismatch on a numeric field.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Discrepancy {
    pub company_id: i64,
    pub field: String,
    pub period: Option<String>,
    pub source_a: String,
    pub value_a: f64,
    pub source_b: String,
    pub value_b: f64,
    pub pct_diff: f64,
    pub flagged_at: DateTime<Utc>,
}

/// Sector-level aggregate for the overview page.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SectorStats {
    pub sector: String,
    pub company_count: i64,
    pub avg_score: f64,
    pub pct_defensive: f64,
    pub top_ticker: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_type_round_trips_through_str() {
        for pt in [PeriodType::Quarterly, PeriodType::Annual] {
            assert_eq!(PeriodType::parse(pt.as_str()), Some(pt));
        }
    }

    #[test]
    fn period_type_parse_rejects_unknown() {
        assert_eq!(PeriodType::parse("weekly"), None);
    }

    #[test]
    fn statement_kind_round_trips_through_str() {
        for sk in [
            StatementKind::Income,
            StatementKind::Balance,
            StatementKind::CashFlow,
        ] {
            assert_eq!(StatementKind::parse(sk.as_str()), Some(sk));
        }
    }

    #[test]
    fn statement_kind_parse_rejects_unknown() {
        assert_eq!(StatementKind::parse("equity"), None);
    }

    #[test]
    fn ratio_is_cloneable_and_comparable() {
        let r = Ratio {
            company_id: 1,
            period_end: chrono::NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            period_type: PeriodType::Annual,
            metric: "pe".into(),
            value: 28.5,
            computed_at: chrono::Utc::now(),
        };
        assert_eq!(r.clone(), r);
        assert!(format!("{r:?}").contains("pe"));
    }

    fn mover(ticker: &str, change_pct: f64, volume: Option<i64>) -> MoverRow {
        MoverRow {
            company: Company {
                id: 1,
                cik: "1".into(),
                ticker: ticker.into(),
                name: ticker.into(),
                exchange: None,
                sector: None,
                industry: None,
                description: None,
                website: None,
                employees: None,
            },
            last_close: 100.0 * (1.0 + change_pct),
            change: 100.0 * change_pct,
            change_pct,
            volume,
            as_of: chrono::NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
        }
    }

    #[test]
    fn select_movers_buckets_gainers_losers_and_most_active() {
        let rows = vec![
            mover("UP2", 0.02, Some(100)),
            mover("DOWN", -0.05, Some(900)),
            mover("UP9", 0.09, None),
            mover("FLAT", 0.0, Some(500)),
        ];
        let m = select_movers(rows, 2);
        let tickers = |list: &[MoverRow]| -> Vec<String> {
            list.iter().map(|r| r.company.ticker.clone()).collect()
        };
        assert_eq!(tickers(&m.gainers), ["UP9", "UP2"]);
        assert_eq!(tickers(&m.losers), ["DOWN", "FLAT"]);
        // most active by volume; a missing volume sorts last
        assert_eq!(tickers(&m.most_active), ["DOWN", "FLAT"]);
        assert_eq!(m.clone(), m);
        assert!(format!("{m:?}").contains("UP9"));
    }

    #[test]
    fn select_movers_handles_short_lists() {
        let m = select_movers(vec![mover("ONLY", 0.01, Some(1))], 10);
        assert_eq!(m.gainers.len(), 1);
        assert_eq!(m.losers.len(), 1);
        assert_eq!(m.most_active.len(), 1);
        let empty = select_movers(Vec::new(), 10);
        assert!(empty.gainers.is_empty());
    }

    #[test]
    fn company_is_cloneable_and_comparable() {
        let c = Company {
            id: 1,
            cik: "0000320193".into(),
            ticker: "AAPL".into(),
            name: "Apple Inc.".into(),
            exchange: Some("NASDAQ".into()),
            sector: None,
            industry: None,
            description: None,
            website: None,
            employees: None,
        };
        assert_eq!(c.clone(), c);
        assert!(format!("{c:?}").contains("AAPL"));
    }

    #[test]
    fn company_profile_overlay_lets_later_source_override_and_fill() {
        let edgar = CompanyProfile {
            industry: Some("Cement, Hydraulic".into()),
            exchange: Some("NYSE".into()),
            ..Default::default()
        };
        let yahoo = CompanyProfile {
            sector: Some("Basic Materials".into()),
            industry: Some("Building Materials".into()),
            website: Some("https://x.com".into()),
            description: Some("makes things".into()),
            employees: Some(10961),
            ..Default::default()
        };
        let merged = edgar.overlay(yahoo);
        assert_eq!(merged.industry.as_deref(), Some("Building Materials")); // yahoo overrides
        assert_eq!(merged.exchange.as_deref(), Some("NYSE")); // edgar kept (yahoo None)
        assert_eq!(merged.sector.as_deref(), Some("Basic Materials"));
        assert_eq!(merged.website.as_deref(), Some("https://x.com"));
        assert_eq!(merged.description.as_deref(), Some("makes things"));
        assert_eq!(merged.employees, Some(10961));

        // a later source without a headcount keeps the earlier one
        let kept = merged.overlay(CompanyProfile::default());
        assert_eq!(kept.employees, Some(10961));
    }
}
