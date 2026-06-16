//! Source collectors. Each collector turns one external source into domain
//! models. Network transport is injected via [`HttpClient`] so collectors are
//! tested offline against captured fixtures.

pub mod edgar;
pub mod edgar_ownership;
pub mod fmp;
pub mod news;
pub mod scrape;
pub mod yahoo;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::domain::{CompanyProfile, FinancialFact, NewsItem, OwnershipHolding, PeriodType, PricePoint};

/// `chrono` format string for ISO `YYYY-MM-DD` dates, as emitted by EDGAR and
/// expected by the Finnhub news query.
pub(crate) const ISO_DATE: &str = "%Y-%m-%d";

/// Deserialize a JSON body into `T`, mapping any error to
/// [`CollectorError::Parse`]. Replaces the `from_str(...).map_err(...)` boilerplate
/// repeated across every collector.
pub(crate) fn parse_json<T: DeserializeOwned>(json: &str) -> Result<T, CollectorError> {
    serde_json::from_str(json).map_err(|e| CollectorError::Parse(e.to_string()))
}

/// A JSON string field as an owned `String`, or `None` when the value is absent,
/// not a string, or empty.
pub(crate) fn nonempty(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

/// An employee count that sources report as either a JSON number (Yahoo) or a
/// string (FMP).
pub(crate) fn employee_count(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

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
    /// Fetch daily bars up to `now`. When `since` is given, only bars from that
    /// date on are needed (incremental refresh); otherwise the source returns
    /// its full history window.
    async fn fetch_prices(
        &self,
        company_id: i64,
        target: &SourceTarget,
        now: DateTime<Utc>,
        since: Option<NaiveDate>,
    ) -> Result<Vec<PricePoint>, CollectorError>;
}

/// A source of company-profile metadata (description, sector/industry, website).
#[async_trait(?Send)]
pub trait ProfileSource {
    fn name(&self) -> &'static str;
    async fn fetch_profile(&self, target: &SourceTarget) -> Result<CompanyProfile, CollectorError>;
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

/// A source of holder positions (e.g. SEC Form 4 insider filings).
#[async_trait(?Send)]
pub trait HolderSource {
    fn name(&self) -> &'static str;
    async fn fetch_holders(
        &self,
        company_id: i64,
        target: &SourceTarget,
    ) -> Result<Vec<OwnershipHolding>, CollectorError>;
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

/// Result of a conditional fetch: a fresh body (with the validators it arrived
/// with) or confirmation that the cached copy is still current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    Modified {
        body: String,
        validators: crate::net::Validators,
    },
    NotModified,
}

/// Minimal HTTP transport seam. Real impls live in [`crate::http`]; tests use fakes.
#[allow(async_fn_in_trait)]
pub trait HttpClient {
    /// Fetch the body of `url` as text.
    async fn get_text(&self, url: &str) -> Result<String, CollectorError>;

    /// Conditional GET: send `validators` as If-None-Match / If-Modified-Since
    /// and report 304 as [`FetchOutcome::NotModified`]. The default ignores
    /// validators and always fetches — transports and sources opt in.
    async fn get_text_with_validators(
        &self,
        url: &str,
        _validators: &crate::net::Validators,
    ) -> Result<FetchOutcome, CollectorError> {
        Ok(FetchOutcome::Modified {
            body: self.get_text(url).await?,
            validators: crate::net::Validators::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Validators;

    /// A transport that only implements the plain fetch, so conditional
    /// requests fall through to the trait default.
    struct PlainHttp;
    impl HttpClient for PlainHttp {
        async fn get_text(&self, _url: &str) -> Result<String, CollectorError> {
            Ok("body".into())
        }
    }

    #[tokio::test]
    async fn default_conditional_fetch_ignores_validators_and_always_fetches() {
        let known = Validators { etag: Some("\"e\"".into()), last_modified: None };
        let outcome = PlainHttp.get_text_with_validators("https://u", &known).await.unwrap();
        assert_eq!(
            outcome,
            FetchOutcome::Modified { body: "body".into(), validators: Validators::default() }
        );
        assert!(format!("{outcome:?}").contains("body"));
    }
}
