//! Financial Modeling Prep collector — ratios, prices, and income-statement
//! facts (the latter let the reconcile layer cross-check EDGAR).

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

use crate::collectors::{
    employee_count, nonempty, parse_json, period_type_from_fp, CollectorError, FactSource,
    HttpClient, PriceSource, ProfileSource, SourceTarget,
};
use crate::domain::{CompanyProfile, FinancialFact, PricePoint, Ratio, StatementKind};
use serde_json::Value;

const BASE: &str = "https://financialmodelingprep.com/api/v3";
const SOURCE: &str = "fmp";

/// Collects prices, income-statement facts, and ratios from FMP.
pub struct FmpCollector<H: HttpClient> {
    http: H,
    api_key: String,
}

impl<H: HttpClient> FmpCollector<H> {
    pub fn new(http: H, api_key: String) -> Self {
        Self { http, api_key }
    }

    pub fn prices_url(symbol: &str, api_key: &str) -> String {
        format!("{BASE}/historical-price-full/{symbol}?apikey={api_key}")
    }

    pub fn income_url(symbol: &str, api_key: &str) -> String {
        format!("{BASE}/income-statement/{symbol}?period=quarter&apikey={api_key}")
    }

    pub fn ratios_url(symbol: &str, api_key: &str) -> String {
        format!("{BASE}/ratios/{symbol}?apikey={api_key}")
    }

    pub fn profile_url(symbol: &str, api_key: &str) -> String {
        format!("{BASE}/profile/{symbol}?apikey={api_key}")
    }

    pub async fn collect_prices(
        &self,
        company_id: i64,
        symbol: &str,
    ) -> Result<Vec<PricePoint>, CollectorError> {
        let body = self
            .http
            .get_text(&Self::prices_url(symbol, &self.api_key))
            .await?;
        parse_prices(company_id, &body)
    }

    pub async fn collect_income(
        &self,
        company_id: i64,
        symbol: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<FinancialFact>, CollectorError> {
        let body = self
            .http
            .get_text(&Self::income_url(symbol, &self.api_key))
            .await?;
        parse_income(company_id, &body, now)
    }

    pub async fn collect_ratios(
        &self,
        company_id: i64,
        symbol: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<Ratio>, CollectorError> {
        let body = self
            .http
            .get_text(&Self::ratios_url(symbol, &self.api_key))
            .await?;
        parse_ratios(company_id, &body, now)
    }
}

#[async_trait(?Send)]
impl<H: HttpClient> FactSource for FmpCollector<H> {
    fn name(&self) -> &'static str {
        "fmp"
    }

    async fn fetch_facts(
        &self,
        company_id: i64,
        target: &SourceTarget,
        now: DateTime<Utc>,
    ) -> Result<Vec<FinancialFact>, CollectorError> {
        self.collect_income(company_id, &target.symbol, now).await
    }
}

#[async_trait(?Send)]
impl<H: HttpClient> ProfileSource for FmpCollector<H> {
    fn name(&self) -> &'static str {
        "fmp"
    }

    async fn fetch_profile(&self, target: &SourceTarget) -> Result<CompanyProfile, CollectorError> {
        let body = self
            .http
            .get_text(&Self::profile_url(&target.symbol, &self.api_key))
            .await?;
        parse_profile(&body)
    }
}

#[async_trait(?Send)]
impl<H: HttpClient> PriceSource for FmpCollector<H> {
    fn name(&self) -> &'static str {
        "fmp"
    }

    async fn fetch_prices(
        &self,
        company_id: i64,
        target: &SourceTarget,
    ) -> Result<Vec<PricePoint>, CollectorError> {
        self.collect_prices(company_id, &target.symbol).await
    }
}

#[derive(Deserialize)]
struct PriceRow {
    date: NaiveDate,
    open: Option<f64>,
    high: Option<f64>,
    low: Option<f64>,
    close: f64,
    volume: Option<i64>,
}

#[derive(Deserialize)]
struct HistoricalDoc {
    historical: Vec<PriceRow>,
}

fn parse_prices(company_id: i64, json: &str) -> Result<Vec<PricePoint>, CollectorError> {
    let doc: HistoricalDoc = parse_json(json)?;
    Ok(doc
        .historical
        .into_iter()
        .map(|r| PricePoint {
            company_id,
            date: r.date,
            open: r.open,
            high: r.high,
            low: r.low,
            close: r.close,
            volume: r.volume,
            source: SOURCE.to_string(),
        })
        .collect())
}

#[derive(Deserialize)]
struct IncomeRow {
    date: NaiveDate,
    period: String,
    revenue: Option<f64>,
    #[serde(rename = "netIncome")]
    net_income: Option<f64>,
    #[serde(rename = "grossProfit")]
    gross_profit: Option<f64>,
    #[serde(rename = "operatingIncome")]
    operating_income: Option<f64>,
    eps: Option<f64>,
    #[serde(rename = "costOfRevenue")]
    cost_of_revenue: Option<f64>,
    ebitda: Option<f64>,
}

fn parse_income(
    company_id: i64,
    json: &str,
    now: DateTime<Utc>,
) -> Result<Vec<FinancialFact>, CollectorError> {
    let rows: Vec<IncomeRow> = parse_json(json)?;
    let mut facts = Vec::new();
    for row in rows {
        let Some(period_type) = period_type_from_fp(&row.period) else {
            continue;
        };
        // Line-item names match EDGAR's, so reconcile cross-checks them.
        for (item, value) in [
            ("Revenue", row.revenue),
            ("NetIncome", row.net_income),
            ("GrossProfit", row.gross_profit),
            ("OperatingIncome", row.operating_income),
            ("Eps", row.eps),
            ("CostOfRevenue", row.cost_of_revenue),
            ("Ebitda", row.ebitda),
        ] {
            if let Some(value) = value {
                facts.push(FinancialFact {
                    company_id,
                    statement: StatementKind::Income,
                    line_item: item.to_string(),
                    period_type,
                    period_end: row.date,
                    value,
                    source: SOURCE.to_string(),
                    fetched_at: now,
                });
            }
        }
    }
    Ok(facts)
}

/// Parse FMP's `/profile/{symbol}` response (a one-element array). Missing or
/// empty fields become `None`; an empty array yields a blank profile.
fn parse_profile(json: &str) -> Result<CompanyProfile, CollectorError> {
    let doc: Value = parse_json(json)?;
    let p = &doc[0];
    Ok(CompanyProfile {
        sector: nonempty(&p["sector"]),
        industry: nonempty(&p["industry"]),
        exchange: nonempty(&p["exchangeShortName"]),
        website: nonempty(&p["website"]),
        description: nonempty(&p["description"]),
        employees: employee_count(&p["fullTimeEmployees"]),
    })
}

#[derive(Deserialize)]
struct RatioRow {
    date: NaiveDate,
    #[serde(rename = "priceEarningsRatio")]
    pe: Option<f64>,
    #[serde(rename = "returnOnEquity")]
    roe: Option<f64>,
    #[serde(rename = "netProfitMargin")]
    net_profit_margin: Option<f64>,
}

fn parse_ratios(
    company_id: i64,
    json: &str,
    now: DateTime<Utc>,
) -> Result<Vec<Ratio>, CollectorError> {
    let rows: Vec<RatioRow> = parse_json(json)?;
    let mut ratios = Vec::new();
    for row in rows {
        for (metric, value) in [
            ("pe", row.pe),
            ("roe", row.roe),
            ("netProfitMargin", row.net_profit_margin),
        ] {
            if let Some(value) = value {
                ratios.push(Ratio {
                    company_id,
                    period_end: row.date,
                    period_type: crate::domain::PeriodType::Annual,
                    metric: metric.to_string(),
                    value,
                    computed_at: now,
                });
            }
        }
    }
    Ok(ratios)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::CollectorError;
    use crate::domain::{PeriodType, StatementKind};
    use crate::testutil::{fixed_now as now, FakeHttp};
    use chrono::NaiveDate;

    const PRICES: &str = include_str!("../../tests/fixtures/fmp_prices.json");
    const INCOME: &str = include_str!("../../tests/fixtures/fmp_income.json");
    const RATIOS: &str = include_str!("../../tests/fixtures/fmp_ratios.json");
    const PROFILE: &str = include_str!("../../tests/fixtures/fmp_profile.json");

    #[test]
    fn url_builders_include_symbol_and_key() {
        assert_eq!(
            FmpCollector::<FakeHttp>::prices_url("AAPL", "KEY"),
            "https://financialmodelingprep.com/api/v3/historical-price-full/AAPL?apikey=KEY"
        );
        assert!(FmpCollector::<FakeHttp>::income_url("AAPL", "KEY").contains("income-statement/AAPL"));
        assert!(FmpCollector::<FakeHttp>::ratios_url("AAPL", "KEY").ends_with("apikey=KEY"));
    }

    #[test]
    fn parse_prices_maps_rows_with_optional_volume() {
        let prices = parse_prices(7, PRICES).unwrap();
        assert_eq!(prices.len(), 2);
        assert_eq!(prices[0].date, NaiveDate::from_ymd_opt(2024, 1, 3).unwrap());
        assert_eq!(prices[0].volume, Some(58414460));
        assert_eq!(prices[0].open, Some(184.0));
        assert_eq!(prices[0].high, Some(185.9));
        assert_eq!(prices[1].volume, None);
        assert_eq!(prices[1].open, None); // close-only row
        assert_eq!(prices[0].source, "fmp");
        assert_eq!(prices[0].company_id, 7);
    }

    #[test]
    fn parse_prices_invalid_json_errors() {
        assert!(matches!(parse_prices(7, "nope").unwrap_err(), CollectorError::Parse(_)));
    }

    #[test]
    fn parse_income_maps_present_fields_and_skips_unknown_period() {
        let facts = parse_income(7, INCOME, now()).unwrap();
        // FY 2023 (7 items) + Q3 revenue + 2022 FY netIncome = 9; TTM row skipped.
        assert_eq!(facts.len(), 9);
        assert!(facts
            .iter()
            .all(|f| f.statement == StatementKind::Income && f.source == "fmp"));
        let q3_rev = facts
            .iter()
            .find(|f| f.period_type == PeriodType::Quarterly && f.line_item == "Revenue")
            .unwrap();
        assert_eq!(q3_rev.value, 81797000000.0);
        // TTM row (value 1/2) excluded
        assert!(facts.iter().all(|f| f.value != 1.0 && f.value != 2.0));
    }

    #[test]
    fn parse_income_maps_extended_line_items() {
        let facts = parse_income(7, INCOME, now()).unwrap();
        let fy23 = NaiveDate::from_ymd_opt(2023, 9, 30).unwrap();
        let fy = |item: &str| {
            facts
                .iter()
                .find(|f| {
                    f.line_item == item
                        && f.period_type == PeriodType::Annual
                        && f.period_end == fy23
                })
                .unwrap_or_else(|| panic!("{item} missing"))
        };
        assert_eq!(fy("GrossProfit").value, 169148000000.0);
        assert_eq!(fy("OperatingIncome").value, 114301000000.0);
        assert_eq!(fy("Eps").value, 6.13);
        assert_eq!(fy("CostOfRevenue").value, 214137000000.0);
        assert_eq!(fy("Ebitda").value, 125820000000.0);
    }

    #[test]
    fn parse_income_invalid_json_errors() {
        assert!(matches!(parse_income(7, "nope", now()).unwrap_err(), CollectorError::Parse(_)));
    }

    #[test]
    fn parse_profile_maps_fields_including_string_employees() {
        let p = parse_profile(PROFILE).unwrap();
        assert_eq!(p.sector.as_deref(), Some("Technology"));
        assert_eq!(p.industry.as_deref(), Some("Consumer Electronics"));
        assert_eq!(p.exchange.as_deref(), Some("NASDAQ"));
        assert_eq!(p.website.as_deref(), Some("https://www.apple.com"));
        assert!(p.description.unwrap().starts_with("Apple Inc."));
        // FMP reports fullTimeEmployees as a string
        assert_eq!(p.employees, Some(164000));
    }

    #[test]
    fn parse_profile_empty_array_is_blank() {
        assert_eq!(parse_profile("[]").unwrap(), CompanyProfile::default());
    }

    #[test]
    fn parse_profile_invalid_json_errors() {
        assert!(matches!(parse_profile("nope").unwrap_err(), CollectorError::Parse(_)));
    }

    #[tokio::test]
    async fn fetch_profile_hits_profile_endpoint() {
        let c = FmpCollector::new(FakeHttp::new(PROFILE), "KEY".into());
        let target = SourceTarget { cik: "320193".into(), symbol: "AAPL".into() };
        let p = c.fetch_profile(&target).await.unwrap();
        assert_eq!(p.employees, Some(164000));
        assert_eq!(ProfileSource::name(&c), "fmp");
        assert!(c.http.url().unwrap().contains("/profile/AAPL?apikey=KEY"));
    }

    #[test]
    fn parse_ratios_emits_present_metrics_only() {
        let ratios = parse_ratios(7, RATIOS, now()).unwrap();
        // row1: pe, roe, npm (3) + row2: roe, npm (2) = 5
        assert_eq!(ratios.len(), 5);
        let pe = ratios.iter().find(|r| r.metric == "pe").unwrap();
        assert_eq!(pe.value, 28.5);
        assert_eq!(pe.computed_at, now());
        // only one pe (row2 has none)
        assert_eq!(ratios.iter().filter(|r| r.metric == "pe").count(), 1);
    }

    #[test]
    fn parse_ratios_invalid_json_errors() {
        assert!(matches!(parse_ratios(7, "nope", now()).unwrap_err(), CollectorError::Parse(_)));
    }

    #[tokio::test]
    async fn collect_prices_fetches_then_parses() {
        let c = FmpCollector::new(FakeHttp::new(PRICES), "KEY".into());
        let prices = c.collect_prices(7, "AAPL").await.unwrap();
        assert_eq!(prices.len(), 2);
        assert!(c.http.url().unwrap().contains("AAPL"));
    }

    #[tokio::test]
    async fn collect_income_fetches_then_parses() {
        let c = FmpCollector::new(FakeHttp::new(INCOME), "KEY".into());
        let facts = c.collect_income(7, "AAPL", now()).await.unwrap();
        assert_eq!(facts.len(), 9);
    }

    #[tokio::test]
    async fn collect_ratios_fetches_then_parses() {
        let c = FmpCollector::new(FakeHttp::new(RATIOS), "KEY".into());
        let ratios = c.collect_ratios(7, "AAPL", now()).await.unwrap();
        assert_eq!(ratios.len(), 5);
    }
}
