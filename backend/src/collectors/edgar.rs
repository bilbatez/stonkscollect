//! SEC EDGAR companyfacts collector — the canonical fundamentals source.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;

use crate::collectors::{period_type_from_fp, CollectorError, HttpClient};
use crate::domain::{FinancialFact, StatementKind};

/// XBRL us-gaap concept -> (statement, normalized line item).
const CONCEPTS: &[(&str, StatementKind, &str)] = &[
    ("Revenues", StatementKind::Income, "Revenue"),
    (
        "RevenueFromContractWithCustomerExcludingAssessedTax",
        StatementKind::Income,
        "Revenue",
    ),
    ("NetIncomeLoss", StatementKind::Income, "NetIncome"),
    ("Assets", StatementKind::Balance, "TotalAssets"),
    ("Liabilities", StatementKind::Balance, "TotalLiabilities"),
    (
        "StockholdersEquity",
        StatementKind::Balance,
        "StockholdersEquity",
    ),
    (
        "CashAndCashEquivalentsAtCarryingValue",
        StatementKind::Balance,
        "CashAndEquivalents",
    ),
    (
        "NetCashProvidedByUsedInOperatingActivities",
        StatementKind::CashFlow,
        "OperatingCashFlow",
    ),
    (
        "NetCashProvidedByUsedInInvestingActivities",
        StatementKind::CashFlow,
        "InvestingCashFlow",
    ),
    (
        "NetCashProvidedByUsedInFinancingActivities",
        StatementKind::CashFlow,
        "FinancingCashFlow",
    ),
];

/// Collects canonical fundamentals from SEC EDGAR's companyfacts API.
pub struct EdgarCollector<H: HttpClient> {
    http: H,
}

impl<H: HttpClient> EdgarCollector<H> {
    pub fn new(http: H) -> Self {
        Self { http }
    }

    /// Build the companyfacts URL for a (possibly unpadded) CIK.
    pub fn companyfacts_url(cik: &str) -> String {
        format!("https://data.sec.gov/api/xbrl/companyfacts/CIK{cik:0>10}.json")
    }

    /// Fetch and parse a company's facts into normalized [`FinancialFact`]s.
    pub async fn collect_facts(
        &self,
        company_id: i64,
        cik: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<FinancialFact>, CollectorError> {
        let url = Self::companyfacts_url(cik);
        let body = self.http.get_text(&url).await?;
        parse_companyfacts(company_id, &body, now)
    }
}

/// Parse a companyfacts JSON document. Later filings for the same
/// (line item, period type, period end) supersede earlier ones.
fn parse_companyfacts(
    company_id: i64,
    json: &str,
    now: DateTime<Utc>,
) -> Result<Vec<FinancialFact>, CollectorError> {
    let doc: Value = serde_json::from_str(json).map_err(|e| CollectorError::Parse(e.to_string()))?;
    let mut deduped: BTreeMap<(&'static str, &'static str, NaiveDate), FinancialFact> =
        BTreeMap::new();

    for (concept, statement, line_item) in CONCEPTS {
        let Some(units) = doc["facts"]["us-gaap"][concept]["units"].as_object() else {
            continue;
        };
        let entry_array = units.get("USD").or_else(|| units.values().next());
        let Some(entries) = entry_array.and_then(|a| a.as_array()) else {
            continue;
        };
        for entry in entries {
            let Some(fp) = entry["fp"].as_str() else {
                continue;
            };
            let Some(period_type) = period_type_from_fp(fp) else {
                continue;
            };
            let Some(end_str) = entry["end"].as_str() else {
                continue;
            };
            let Ok(period_end) = NaiveDate::parse_from_str(end_str, "%Y-%m-%d") else {
                continue;
            };
            let Some(value) = entry["val"].as_f64() else {
                continue;
            };
            deduped.insert(
                (line_item, period_type.as_str(), period_end),
                FinancialFact {
                    company_id,
                    statement: *statement,
                    line_item: (*line_item).to_string(),
                    period_type,
                    period_end,
                    value,
                    source: "edgar".to_string(),
                    fetched_at: now,
                },
            );
        }
    }
    Ok(deduped.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::CollectorError;
    use crate::domain::{PeriodType, StatementKind};
    use chrono::{NaiveDate, TimeZone, Utc};
    use std::cell::RefCell;

    const FIXTURE: &str = include_str!("../../tests/fixtures/edgar_companyfacts.json");

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
    fn companyfacts_url_zero_pads_cik() {
        assert_eq!(
            EdgarCollector::<FakeHttp>::companyfacts_url("320193"),
            "https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json"
        );
    }

    fn find<'a>(
        facts: &'a [crate::domain::FinancialFact],
        item: &str,
        pt: PeriodType,
        end: NaiveDate,
    ) -> &'a crate::domain::FinancialFact {
        facts
            .iter()
            .find(|f| f.line_item == item && f.period_type == pt && f.period_end == end)
            .expect("fact present")
    }

    #[test]
    fn parse_maps_concepts_and_periods() {
        let facts = parse_companyfacts(7, FIXTURE, now()).unwrap();
        let rev_fy = find(
            &facts,
            "Revenue",
            PeriodType::Annual,
            NaiveDate::from_ymd_opt(2022, 9, 24).unwrap(),
        );
        assert_eq!(rev_fy.statement, StatementKind::Income);
        assert_eq!(rev_fy.company_id, 7);
        assert_eq!(rev_fy.source, "edgar");
        assert_eq!(rev_fy.fetched_at, now());

        let q3 = find(
            &facts,
            "Revenue",
            PeriodType::Quarterly,
            NaiveDate::from_ymd_opt(2023, 7, 1).unwrap(),
        );
        assert_eq!(q3.value, 81797000000.0);

        let ocf = find(
            &facts,
            "OperatingCashFlow",
            PeriodType::Annual,
            NaiveDate::from_ymd_opt(2023, 9, 30).unwrap(),
        );
        assert_eq!(ocf.statement, StatementKind::CashFlow);
    }

    #[test]
    fn parse_keeps_latest_restatement() {
        let facts = parse_companyfacts(7, FIXTURE, now()).unwrap();
        let rev_2023 = find(
            &facts,
            "Revenue",
            PeriodType::Annual,
            NaiveDate::from_ymd_opt(2023, 9, 30).unwrap(),
        );
        // 10-K/A (383285...) supersedes the original 10-K (383000...).
        assert_eq!(rev_2023.value, 383285000000.0);
    }

    #[test]
    fn parse_skips_unknown_period_and_unmapped_concepts() {
        let facts = parse_companyfacts(7, FIXTURE, now()).unwrap();
        // The CY 8-K entry and the UnmappedConcept must not appear.
        assert!(facts.iter().all(|f| f.value != 999.0));
        assert!(facts.iter().all(|f| f.line_item != "UnmappedConcept"));
    }

    #[test]
    fn parse_handles_non_usd_unit_and_skips_malformed_entries() {
        let facts = parse_companyfacts(7, FIXTURE, now()).unwrap();
        // Investing CF lives under a non-USD unit key ("pure").
        let inv = find(
            &facts,
            "InvestingCashFlow",
            PeriodType::Annual,
            NaiveDate::from_ymd_opt(2023, 9, 30).unwrap(),
        );
        assert_eq!(inv.value, 3705000000.0);
        // Only the single well-formed Liabilities row survives the edge entries.
        let liabs: Vec<_> = facts
            .iter()
            .filter(|f| f.line_item == "TotalLiabilities")
            .collect();
        assert_eq!(liabs.len(), 1);
        assert_eq!(liabs[0].value, 290437000000.0);
    }

    #[test]
    fn parse_empty_facts_yields_no_rows() {
        let facts = parse_companyfacts(7, "{}", now()).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn parse_invalid_json_errors() {
        let err = parse_companyfacts(7, "not json", now()).unwrap_err();
        assert!(matches!(err, CollectorError::Parse(_)));
    }

    #[tokio::test]
    async fn collect_facts_fetches_then_parses() {
        let http = FakeHttp {
            body: FIXTURE.to_string(),
            last_url: RefCell::new(None),
        };
        let collector = EdgarCollector::new(http);
        let facts = collector.collect_facts(7, "320193", now()).await.unwrap();
        assert!(!facts.is_empty());
        assert_eq!(
            collector.http.last_url.borrow().as_deref(),
            Some("https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json")
        );
    }

    #[test]
    fn collector_error_display_covers_variants() {
        assert!(CollectorError::Http("x".into()).to_string().contains("http"));
        assert!(CollectorError::Parse("y".into()).to_string().contains("parse"));
    }
}
