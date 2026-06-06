//! HTML scrape fallback. Supplements/cross-checks API + EDGAR data by parsing
//! a financials table. Includes a politeness rate-limit primitive.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use scraper::{Html, Selector};

use crate::collectors::{CollectorError, HttpClient};
use crate::domain::{FinancialFact, PeriodType, StatementKind};

const SOURCE: &str = "scrape";

/// Row-label -> (statement, normalized line item).
const METRICS: &[(&str, StatementKind, &str)] = &[
    ("Revenue", StatementKind::Income, "Revenue"),
    ("Net Income", StatementKind::Income, "NetIncome"),
    ("Total Assets", StatementKind::Balance, "TotalAssets"),
    (
        "Operating Cash Flow",
        StatementKind::CashFlow,
        "OperatingCashFlow",
    ),
];

fn lookup(metric: &str) -> Option<(StatementKind, &'static str)> {
    METRICS
        .iter()
        .find(|(label, _, _)| *label == metric)
        .map(|(_, statement, item)| (*statement, *item))
}

/// Parse a numeric cell ("383,285", "1 234") into f64; `None` if not numeric.
fn parse_number(text: &str) -> Option<f64> {
    let cleaned: String = text.chars().filter(|c| *c != ',' && *c != ' ').collect();
    cleaned.parse::<f64>().ok()
}

/// Whether a request is allowed now given the last request time and a minimum
/// spacing interval (politeness rate limit).
pub fn allow_request(last: Option<DateTime<Utc>>, now: DateTime<Utc>, min_interval: Duration) -> bool {
    match last {
        None => true,
        Some(prev) => now.signed_duration_since(prev) >= min_interval,
    }
}

/// Parse a financials table into facts. Values are in millions. Rows that are
/// unmapped, undated, or non-numeric are skipped. Annual periods are assumed
/// to end Dec 31 of the stated year (a scrape approximation).
fn parse_financials(company_id: i64, html: &str, now: DateTime<Utc>) -> Vec<FinancialFact> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("table.financials tr").expect("valid selector");
    let metric_sel = Selector::parse("td.metric").expect("valid selector");
    let value_sel = Selector::parse("td.value").expect("valid selector");

    let mut facts = Vec::new();
    for row in doc.select(&row_sel) {
        let Some(year_str) = row.value().attr("data-year") else {
            continue;
        };
        let Ok(year) = year_str.parse::<i32>() else {
            continue;
        };
        let Some(metric_el) = row.select(&metric_sel).next() else {
            continue;
        };
        let metric = metric_el.text().collect::<String>();
        let metric = metric.trim();
        let Some((statement, line_item)) = lookup(metric) else {
            continue;
        };
        let Some(value_el) = row.select(&value_sel).next() else {
            continue;
        };
        let Some(value) = parse_number(&value_el.text().collect::<String>()) else {
            continue;
        };
        // Dec 31 of any parsed year is a valid date.
        let period_end = NaiveDate::from_ymd_opt(year, 12, 31).expect("Dec 31 is valid");
        facts.push(FinancialFact {
            company_id,
            statement,
            line_item: line_item.to_string(),
            period_type: PeriodType::Annual,
            period_end,
            value: value * 1_000_000.0,
            source: SOURCE.to_string(),
            fetched_at: now,
        });
    }
    facts
}

/// Scrapes a financials page (e.g. stockanalysis.com) as a fallback source.
pub struct ScrapeCollector<H: HttpClient> {
    http: H,
}

impl<H: HttpClient> ScrapeCollector<H> {
    pub fn new(http: H) -> Self {
        Self { http }
    }

    pub fn financials_url(symbol: &str) -> String {
        format!("https://stockanalysis.com/stocks/{symbol}/financials/")
    }

    pub async fn collect(
        &self,
        company_id: i64,
        symbol: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<FinancialFact>, CollectorError> {
        let body = self.http.get_text(&Self::financials_url(symbol)).await?;
        Ok(parse_financials(company_id, &body, now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::CollectorError;
    use crate::domain::{PeriodType, StatementKind};
    use chrono::{Duration, NaiveDate, TimeZone, Utc};
    use std::cell::RefCell;

    const HTML: &str = include_str!("../../tests/fixtures/scrape_financials.html");

    struct FakeHttp {
        body: String,
        last_url: RefCell<Option<String>>,
    }
    impl HttpClient for FakeHttp {
        async fn get_text(&self, url: &str) -> Result<String, CollectorError> {
            *self.last_url.borrow_mut() = Some(url.to_string());
            Ok(self.body.clone())
        }
    }

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
    }

    #[test]
    fn financials_url_includes_symbol() {
        assert!(ScrapeCollector::<FakeHttp>::financials_url("AAPL").contains("AAPL"));
    }

    #[test]
    fn parse_extracts_mapped_rows_and_skips_the_rest() {
        let facts = parse_financials(7, HTML, now());
        // valid: Revenue 2023, Net Income 2023, Revenue 2022 = 3
        assert_eq!(facts.len(), 3);
        let rev23 = facts
            .iter()
            .find(|f| f.line_item == "Revenue"
                && f.period_end == NaiveDate::from_ymd_opt(2023, 12, 31).unwrap())
            .unwrap();
        assert_eq!(rev23.value, 383_285.0 * 1_000_000.0);
        assert_eq!(rev23.statement, StatementKind::Income);
        assert_eq!(rev23.period_type, PeriodType::Annual);
        assert_eq!(rev23.source, "scrape");
        assert_eq!(rev23.company_id, 7);
        assert_eq!(rev23.fetched_at, now());
        // unmapped/malformed rows excluded
        assert!(facts.iter().all(|f| f.line_item != "Gross Profit"));
        assert!(facts.iter().all(|f| f.value != 5.0 * 1_000_000.0));
    }

    #[test]
    fn parse_empty_html_yields_nothing() {
        assert!(parse_financials(7, "<html></html>", now()).is_empty());
    }

    #[tokio::test]
    async fn collector_fetches_then_parses() {
        let http = FakeHttp { body: HTML.to_string(), last_url: RefCell::new(None) };
        let c = ScrapeCollector::new(http);
        let facts = c.collect(7, "AAPL", now()).await.unwrap();
        assert_eq!(facts.len(), 3);
        assert!(c.http.last_url.borrow().as_deref().unwrap().contains("AAPL"));
    }

    #[test]
    fn rate_limit_allows_first_and_spaced_requests() {
        let min = Duration::seconds(10);
        assert!(allow_request(None, now(), min));
        assert!(allow_request(Some(now() - Duration::seconds(20)), now(), min));
        assert!(!allow_request(Some(now() - Duration::seconds(5)), now(), min));
    }
}
