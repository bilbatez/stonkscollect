//! Source collectors. Each collector turns one external source into domain
//! models. Network transport is injected via [`HttpClient`] so collectors are
//! tested offline against captured fixtures.

pub mod edgar;
pub mod fmp;
pub mod news;

use crate::domain::PeriodType;

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
