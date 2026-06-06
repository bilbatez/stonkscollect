//! Financial Modeling Prep collector — ratios, prices, and income-statement
//! facts (the latter let the reconcile layer cross-check EDGAR).

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

use crate::collectors::{period_type_from_fp, CollectorError, HttpClient};
use crate::domain::{FinancialFact, PricePoint, Ratio, StatementKind};

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

#[derive(Deserialize)]
struct PriceRow {
    date: NaiveDate,
    close: f64,
    volume: Option<i64>,
}

#[derive(Deserialize)]
struct HistoricalDoc {
    historical: Vec<PriceRow>,
}

fn parse_prices(company_id: i64, json: &str) -> Result<Vec<PricePoint>, CollectorError> {
    let doc: HistoricalDoc =
        serde_json::from_str(json).map_err(|e| CollectorError::Parse(e.to_string()))?;
    Ok(doc
        .historical
        .into_iter()
        .map(|r| PricePoint {
            company_id,
            date: r.date,
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
}

fn parse_income(
    company_id: i64,
    json: &str,
    now: DateTime<Utc>,
) -> Result<Vec<FinancialFact>, CollectorError> {
    let rows: Vec<IncomeRow> =
        serde_json::from_str(json).map_err(|e| CollectorError::Parse(e.to_string()))?;
    let mut facts = Vec::new();
    for row in rows {
        let Some(period_type) = period_type_from_fp(&row.period) else {
            continue;
        };
        for (item, value) in [("Revenue", row.revenue), ("NetIncome", row.net_income)] {
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
    let rows: Vec<RatioRow> =
        serde_json::from_str(json).map_err(|e| CollectorError::Parse(e.to_string()))?;
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
    use chrono::{NaiveDate, TimeZone, Utc};
    use std::cell::RefCell;

    const PRICES: &str = include_str!("../../tests/fixtures/fmp_prices.json");
    const INCOME: &str = include_str!("../../tests/fixtures/fmp_income.json");
    const RATIOS: &str = include_str!("../../tests/fixtures/fmp_ratios.json");

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
        assert_eq!(prices[1].volume, None);
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
        // FY revenue + FY netIncome + Q3 revenue + 2022 FY netIncome = 4; TTM row skipped.
        assert_eq!(facts.len(), 4);
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
    fn parse_income_invalid_json_errors() {
        assert!(matches!(parse_income(7, "nope", now()).unwrap_err(), CollectorError::Parse(_)));
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
        let http = FakeHttp { body: PRICES.to_string(), last_url: RefCell::new(None) };
        let c = FmpCollector::new(http, "KEY".into());
        let prices = c.collect_prices(7, "AAPL").await.unwrap();
        assert_eq!(prices.len(), 2);
        assert!(c.http.last_url.borrow().as_deref().unwrap().contains("AAPL"));
    }

    #[tokio::test]
    async fn collect_income_fetches_then_parses() {
        let http = FakeHttp { body: INCOME.to_string(), last_url: RefCell::new(None) };
        let c = FmpCollector::new(http, "KEY".into());
        let facts = c.collect_income(7, "AAPL", now()).await.unwrap();
        assert_eq!(facts.len(), 4);
    }

    #[tokio::test]
    async fn collect_ratios_fetches_then_parses() {
        let http = FakeHttp { body: RATIOS.to_string(), last_url: RefCell::new(None) };
        let c = FmpCollector::new(http, "KEY".into());
        let ratios = c.collect_ratios(7, "AAPL", now()).await.unwrap();
        assert_eq!(ratios.len(), 5);
    }
}
