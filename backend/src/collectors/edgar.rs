//! SEC EDGAR companyfacts collector — the canonical fundamentals source.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;

use async_trait::async_trait;

use crate::collectors::{
    period_type_from_fp, CollectorError, FactSource, HttpClient, ProfileSource, SourceTarget,
};
use crate::domain::{CompanyProfile, FinancialFact, StatementKind};

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
    // --- Graham analysis inputs ---
    ("AssetsCurrent", StatementKind::Balance, "CurrentAssets"),
    (
        "LiabilitiesCurrent",
        StatementKind::Balance,
        "CurrentLiabilities",
    ),
    ("LongTermDebtNoncurrent", StatementKind::Balance, "LongTermDebt"),
    ("LongTermDebt", StatementKind::Balance, "LongTermDebt"),
    ("GrossProfit", StatementKind::Income, "GrossProfit"),
    ("OperatingIncomeLoss", StatementKind::Income, "OperatingIncome"),
    ("EarningsPerShareDiluted", StatementKind::Income, "Eps"),
    ("EarningsPerShareBasic", StatementKind::Income, "Eps"),
    (
        "CommonStockDividendsPerShareDeclared",
        StatementKind::Income,
        "DividendPerShare",
    ),
    (
        "PaymentsOfDividendsCommonStock",
        StatementKind::CashFlow,
        "DividendsPaid",
    ),
    (
        "WeightedAverageNumberOfDilutedSharesOutstanding",
        StatementKind::Income,
        "SharesOutstanding",
    ),
    (
        "PaymentsToAcquirePropertyPlantAndEquipment",
        StatementKind::CashFlow,
        "CapEx",
    ),
    // --- Extended income statement items ---
    (
        "DepreciationDepletionAndAmortization",
        StatementKind::Income,
        "DepreciationAmortization",
    ),
    (
        "ResearchAndDevelopmentExpense",
        StatementKind::Income,
        "ResearchAndDevelopment",
    ),
    (
        "SellingGeneralAndAdministrativeExpense",
        StatementKind::Income,
        "SellingGeneralAdmin",
    ),
    ("InterestExpense", StatementKind::Income, "InterestExpense"),
    (
        "InterestAndDebtExpense",
        StatementKind::Income,
        "InterestExpense",
    ),
    (
        "IncomeTaxExpenseBenefit",
        StatementKind::Income,
        "IncomeTaxExpense",
    ),
    // --- Extended balance sheet items ---
    ("Goodwill", StatementKind::Balance, "Goodwill"),
    (
        "IntangibleAssetsNetExcludingGoodwill",
        StatementKind::Balance,
        "IntangibleAssets",
    ),
    (
        "PropertyPlantAndEquipmentNet",
        StatementKind::Balance,
        "PropertyPlantEquipment",
    ),
    ("InventoryNet", StatementKind::Balance, "Inventories"),
    (
        "AccountsReceivableNetCurrent",
        StatementKind::Balance,
        "AccountsReceivable",
    ),
    (
        "AccountsPayableCurrent",
        StatementKind::Balance,
        "AccountsPayable",
    ),
    (
        "ShortTermBorrowings",
        StatementKind::Balance,
        "ShortTermDebt",
    ),
    (
        "LongTermDebtCurrent",
        StatementKind::Balance,
        "ShortTermDebt",
    ),
    (
        "RetainedEarningsAccumulatedDeficit",
        StatementKind::Balance,
        "RetainedEarnings",
    ),
    (
        "CommonStockSharesOutstanding",
        StatementKind::Balance,
        "SharesOutstandingBalance",
    ),
];

/// A company identity from SEC's ticker directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyRef {
    pub cik: String,
    pub ticker: String,
    pub name: String,
}

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

    /// Build the submissions (company profile) URL for a CIK.
    pub fn submissions_url(cik: &str) -> String {
        format!("https://data.sec.gov/submissions/CIK{cik:0>10}.json")
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

    /// Fetch SEC's full ticker -> CIK directory (for bootstrapping companies).
    pub async fn collect_company_tickers(&self) -> Result<Vec<CompanyRef>, CollectorError> {
        let body = self
            .http
            .get_text("https://www.sec.gov/files/company_tickers.json")
            .await?;
        parse_company_tickers(&body)
    }
}

#[async_trait(?Send)]
impl<H: HttpClient> ProfileSource for EdgarCollector<H> {
    fn name(&self) -> &'static str {
        "edgar"
    }

    async fn fetch_profile(&self, target: &SourceTarget) -> Result<CompanyProfile, CollectorError> {
        let body = self.http.get_text(&Self::submissions_url(&target.cik)).await?;
        parse_submissions_profile(&body)
    }
}

/// Parse SEC's `submissions/CIK….json` into the canonical bits of a company
/// profile: `sicDescription` → industry, first `exchanges` entry → exchange.
/// (EDGAR's prose `description`/`website` are usually empty; Yahoo fills those.)
fn parse_submissions_profile(json: &str) -> Result<CompanyProfile, CollectorError> {
    let doc: Value = serde_json::from_str(json).map_err(|e| CollectorError::Parse(e.to_string()))?;
    let nonempty = |v: &Value| v.as_str().filter(|s| !s.is_empty()).map(str::to_string);
    let exchange = doc["exchanges"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(nonempty);
    Ok(CompanyProfile {
        industry: nonempty(&doc["sicDescription"]),
        exchange,
        sector: None,
        website: nonempty(&doc["website"]),
        description: nonempty(&doc["description"]),
    })
}

/// Parse SEC's `company_tickers.json` (an object keyed by index). Entries
/// missing a CIK or ticker are skipped.
fn parse_company_tickers(json: &str) -> Result<Vec<CompanyRef>, CollectorError> {
    let doc: Value = serde_json::from_str(json).map_err(|e| CollectorError::Parse(e.to_string()))?;
    let Some(entries) = doc.as_object() else {
        return Ok(Vec::new());
    };
    let mut refs = Vec::new();
    for entry in entries.values() {
        let Some(cik) = entry["cik_str"].as_i64() else {
            continue;
        };
        let Some(ticker) = entry["ticker"].as_str() else {
            continue;
        };
        refs.push(CompanyRef {
            cik: format!("{cik:0>10}"),
            ticker: ticker.to_uppercase(),
            name: entry["title"].as_str().unwrap_or("").to_string(),
        });
    }
    Ok(refs)
}

#[async_trait(?Send)]
impl<H: HttpClient> FactSource for EdgarCollector<H> {
    fn name(&self) -> &'static str {
        "edgar"
    }

    async fn fetch_facts(
        &self,
        company_id: i64,
        target: &SourceTarget,
        now: DateTime<Utc>,
    ) -> Result<Vec<FinancialFact>, CollectorError> {
        self.collect_facts(company_id, &target.cik, now).await
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
    // Keep, per (line item, period type, period end), the entry from the
    // latest filing. "filed" is ISO (YYYY-MM-DD) so lexicographic compare works.
    let mut deduped: BTreeMap<(&'static str, &'static str, NaiveDate), (String, FinancialFact)> =
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
            let filed = entry["filed"].as_str().unwrap_or("").to_string();
            let key = (*line_item, period_type.as_str(), period_end);
            // Only replace an existing entry if this filing is at least as recent.
            if deduped.get(&key).is_some_and(|(prev, _)| filed < *prev) {
                continue;
            }
            deduped.insert(
                key,
                (
                    filed,
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
                ),
            );
        }
    }
    Ok(deduped.into_values().map(|(_, fact)| fact).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::{CollectorError, ProfileSource, SourceTarget};
    use crate::domain::{PeriodType, StatementKind};
    use crate::testutil::{fixed_now as now, FakeHttp};
    use chrono::NaiveDate;

    const FIXTURE: &str = include_str!("../../tests/fixtures/edgar_companyfacts.json");
    const TICKERS: &str = include_str!("../../tests/fixtures/company_tickers.json");
    const SUBMISSIONS: &str = include_str!("../../tests/fixtures/edgar_submissions.json");

    #[test]
    fn parse_company_tickers_pads_cik_and_skips_incomplete() {
        let refs = parse_company_tickers(TICKERS).unwrap();
        // AAPL, MSFT, NONAME(no title) kept; missing-ticker + missing-cik skipped.
        assert_eq!(refs.len(), 3);
        let aapl = refs.iter().find(|r| r.ticker == "AAPL").unwrap();
        assert_eq!(aapl.cik, "0000320193");
        assert_eq!(aapl.name, "Apple Inc.");
        let noname = refs.iter().find(|r| r.ticker == "NONAME").unwrap();
        assert_eq!(noname.name, "");
        assert_eq!(aapl.clone(), *aapl);
    }

    #[test]
    fn parse_company_tickers_non_object_is_empty() {
        assert!(parse_company_tickers("[]").unwrap().is_empty());
    }

    #[test]
    fn parse_company_tickers_invalid_json_errors() {
        assert!(matches!(parse_company_tickers("x").unwrap_err(), CollectorError::Parse(_)));
    }

    #[tokio::test]
    async fn collect_company_tickers_fetches_directory() {
        let collector = EdgarCollector::new(FakeHttp::new(TICKERS));
        let refs = collector.collect_company_tickers().await.unwrap();
        assert_eq!(refs.len(), 3);
        assert!(collector.http.url().unwrap().contains("company_tickers.json"));
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
    fn parse_captures_graham_inputs_including_per_share_units() {
        let facts = parse_companyfacts(7, FIXTURE, now()).unwrap();
        let fy23 = NaiveDate::from_ymd_opt(2023, 9, 30).unwrap();
        // EPS lives under a "USD/shares" unit; the parser falls back past USD.
        assert_eq!(find(&facts, "Eps", PeriodType::Annual, fy23).value, 6.13);
        assert_eq!(find(&facts, "SharesOutstanding", PeriodType::Annual, fy23).value, 15812547000.0);
        assert_eq!(find(&facts, "CurrentAssets", PeriodType::Annual, fy23).value, 143566000000.0);
        assert_eq!(find(&facts, "CurrentLiabilities", PeriodType::Annual, fy23).value, 145308000000.0);
        assert_eq!(find(&facts, "DividendPerShare", PeriodType::Annual, fy23).value, 0.94);
        // Extended items from the expanded CONCEPTS array
        assert_eq!(find(&facts, "DepreciationAmortization", PeriodType::Annual, fy23).value, 11519000000.0);
        assert_eq!(find(&facts, "ResearchAndDevelopment", PeriodType::Annual, fy23).value, 29915000000.0);
        assert_eq!(find(&facts, "AccountsPayable", PeriodType::Annual, fy23).value, 62611000000.0);
        assert_eq!(find(&facts, "Goodwill", PeriodType::Annual, fy23).value, 0.0);
        assert_eq!(find(&facts, "Inventories", PeriodType::Annual, fy23).value, 6331000000.0);
        assert_eq!(find(&facts, "RetainedEarnings", PeriodType::Annual, fy23).value, -214966000000.0);
        assert_eq!(find(&facts, "SharesOutstandingBalance", PeriodType::Annual, fy23).value, 15552752000.0);
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
    fn parse_keeps_latest_filed_regardless_of_array_order() {
        // Amended 10-K/A (filed later, val 999) appears BEFORE the original
        // 10-K (filed earlier, val 100) in the array; the amendment must win.
        let json = r#"{"facts":{"us-gaap":{"NetIncomeLoss":{"units":{"USD":[
            {"end":"2023-12-31","val":999,"fp":"FY","form":"10-K/A","filed":"2024-03-01"},
            {"end":"2023-12-31","val":100,"fp":"FY","form":"10-K","filed":"2024-01-15"}
        ]}}}}}"#;
        let facts = parse_companyfacts(7, json, now()).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].value, 999.0);
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
        let collector = EdgarCollector::new(FakeHttp::new(FIXTURE));
        let facts = collector.collect_facts(7, "320193", now()).await.unwrap();
        assert!(!facts.is_empty());
        assert_eq!(
            collector.http.url().as_deref(),
            Some("https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json")
        );
    }

    #[test]
    fn submissions_url_zero_pads_cik() {
        assert_eq!(
            EdgarCollector::<FakeHttp>::submissions_url("109563"),
            "https://data.sec.gov/submissions/CIK0000109563.json"
        );
    }

    #[test]
    fn parse_submissions_profile_maps_industry_and_exchange() {
        let p = parse_submissions_profile(SUBMISSIONS).unwrap();
        assert_eq!(p.industry.as_deref(), Some("Concrete, Gypsum & Plaster Products"));
        assert_eq!(p.exchange.as_deref(), Some("NYSE"));
        assert_eq!(p.sector, None);
        assert_eq!(p.website, None); // empty string -> None
        assert_eq!(p.description, None);
    }

    #[test]
    fn parse_submissions_profile_invalid_json_errors() {
        assert!(matches!(parse_submissions_profile("nope").unwrap_err(), CollectorError::Parse(_)));
    }

    #[tokio::test]
    async fn fetch_profile_hits_submissions_then_parses() {
        let c = EdgarCollector::new(FakeHttp::new(SUBMISSIONS));
        let target = SourceTarget { cik: "109563".into(), symbol: "VMC".into() };
        let p = c.fetch_profile(&target).await.unwrap();
        assert_eq!(p.industry.as_deref(), Some("Concrete, Gypsum & Plaster Products"));
        assert_eq!(ProfileSource::name(&c), "edgar");
        assert!(c.http.url().unwrap().contains("/submissions/CIK0000109563.json"));
    }

    #[test]
    fn collector_error_display_covers_variants() {
        assert!(CollectorError::Http("x".into()).to_string().contains("http"));
        assert!(CollectorError::Parse("y".into()).to_string().contains("parse"));
    }
}
