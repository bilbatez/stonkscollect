//! Core domain models. Pure data + value-object conversions, no I/O.

use chrono::{DateTime, NaiveDate, Utc};

/// Reporting period of a financial fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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
            metric: "pe".into(),
            value: 28.5,
            computed_at: chrono::Utc::now(),
        };
        assert_eq!(r.clone(), r);
        assert!(format!("{r:?}").contains("pe"));
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
        };
        assert_eq!(c.clone(), c);
        assert!(format!("{c:?}").contains("AAPL"));
    }
}
