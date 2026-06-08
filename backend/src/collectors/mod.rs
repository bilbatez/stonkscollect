//! Source collectors. Each collector turns one external source into domain
//! models. Network transport is injected via [`HttpClient`] so collectors are
//! tested offline against captured fixtures.

pub mod edgar;
pub mod fmp;
pub mod news;
pub mod scrape;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::domain::{FinancialFact, NewsItem, PeriodType, PricePoint};

/// Identifies a company across sources: EDGAR keys on CIK, vendors on ticker.
#[derive(Debug, Clone)]
pub struct SourceTarget {
    pub cik: String,
    pub symbol: String,
}

/// A source of daily prices (Strategy).
#[async_trait(?Send)]
pub trait PriceSource {
    fn name(&self) -> &'static str;
    async fn fetch_prices(
        &self,
        company_id: i64,
        target: &SourceTarget,
    ) -> Result<Vec<PricePoint>, CollectorError>;
}

/// A source of company news headlines (Strategy).
#[async_trait(?Send)]
pub trait NewsSource {
    fn name(&self) -> &'static str;
    async fn fetch_news(
        &self,
        company_id: i64,
        target: &SourceTarget,
        now: DateTime<Utc>,
    ) -> Result<Vec<NewsItem>, CollectorError>;
}

/// A source that yields financial facts for a company (Strategy pattern).
///
/// Implemented by EDGAR, FMP, and the HTML scraper so the ingest pipeline can
/// aggregate from a heterogeneous, open-ended set of sources without knowing
/// their concrete types. `?Send`: futures are awaited inline by the driver.
#[async_trait(?Send)]
pub trait FactSource {
    /// Stable identifier used in error reports and run tags.
    fn name(&self) -> &'static str;

    /// Fetch and normalize this source's facts for `target`.
    async fn fetch_facts(
        &self,
        company_id: i64,
        target: &SourceTarget,
        now: DateTime<Utc>,
    ) -> Result<Vec<FinancialFact>, CollectorError>;
}

/// Map an XBRL/FMP fiscal-period token ("FY", "Q1".."Q4") to a [`PeriodType`].
/// Returns `None` for tokens we do not model (e.g. "TTM", "CY").
pub(crate) fn period_type_from_fp(fp: &str) -> Option<PeriodType> {
    match fp {
        "FY" => Some(PeriodType::Annual),
        "Q1" | "Q2" | "Q3" | "Q4" => Some(PeriodType::Quarterly),
        _ => None,
    }
}

/// Errors produced while collecting from an external source.
#[derive(Debug, thiserror::Error)]
pub enum CollectorError {
    #[error("http error: {0}")]
    Http(String),
    #[error("parse error: {0}")]
    Parse(String),
}

/// Minimal HTTP transport seam. Real impls live in [`crate::http`]; tests use fakes.
#[allow(async_fn_in_trait)]
pub trait HttpClient {
    /// Fetch the body of `url` as text.
    async fn get_text(&self, url: &str) -> Result<String, CollectorError>;
}
