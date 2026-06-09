//! Yahoo Finance daily-prices collector — a keyless (no API key) price source.
//!
//! Yahoo's chart API returns daily OHLCV as JSON at
//! `https://query1.finance.yahoo.com/v8/finance/chart/<SYMBOL>?interval=1d&range=5y`.
//! Transport is injected via [`HttpClient`] so it's tested offline against a
//! captured fixture. (Replaces the earlier Stooq source, which began serving a
//! JavaScript anti-bot challenge instead of CSV.)

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use async_trait::async_trait;

use crate::collectors::{CollectorError, HttpClient, PriceSource, SourceTarget};
use crate::domain::PricePoint;

const SOURCE: &str = "yahoo";

/// Collects daily prices from Yahoo Finance's keyless chart API.
pub struct YahooCollector<H: HttpClient> {
    http: H,
}

#[derive(Deserialize)]
struct ChartResponse {
    chart: Chart,
}
#[derive(Deserialize)]
struct Chart {
    result: Option<Vec<ChartResult>>,
}
#[derive(Deserialize)]
struct ChartResult {
    timestamp: Option<Vec<i64>>,
    indicators: Indicators,
}
#[derive(Deserialize)]
struct Indicators {
    quote: Vec<Quote>,
}
// Yahoo sometimes omits some OHLCV arrays for a symbol; default each to empty so
// the response still parses and we keep whatever days have a close.
#[derive(Deserialize, Default)]
struct Quote {
    #[serde(default)]
    open: Vec<Option<f64>>,
    #[serde(default)]
    high: Vec<Option<f64>>,
    #[serde(default)]
    low: Vec<Option<f64>>,
    #[serde(default)]
    close: Vec<Option<f64>>,
    #[serde(default)]
    volume: Vec<Option<i64>>,
}

impl<H: HttpClient> YahooCollector<H> {
    pub fn new(http: H) -> Self {
        Self { http }
    }

    /// ~20 years of trading days.
    const HISTORY_DAYS: i64 = 365 * 20;

    /// Daily-chart URL for ~20y up to `now`. Uses explicit period1/period2 epochs
    /// (with `interval=1d`) rather than `range=max`, which Yahoo downsamples to
    /// monthly for long spans.
    pub fn chart_url(symbol: &str, now: DateTime<Utc>) -> String {
        let period2 = now.timestamp();
        let period1 = (now - Duration::days(Self::HISTORY_DAYS)).timestamp();
        format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&period1={period1}&period2={period2}",
            symbol.to_uppercase()
        )
    }

    pub async fn collect_prices(
        &self,
        company_id: i64,
        symbol: &str,
    ) -> Result<Vec<PricePoint>, CollectorError> {
        let body = self.http.get_text(&Self::chart_url(symbol, Utc::now())).await?;
        parse_chart_json(company_id, &body)
    }
}

#[async_trait(?Send)]
impl<H: HttpClient> PriceSource for YahooCollector<H> {
    fn name(&self) -> &'static str {
        SOURCE
    }

    async fn fetch_prices(
        &self,
        company_id: i64,
        target: &SourceTarget,
    ) -> Result<Vec<PricePoint>, CollectorError> {
        self.collect_prices(company_id, &target.symbol).await
    }
}

/// Parse Yahoo's chart JSON into price points. Days with a null close (no trade)
/// are skipped; an error/empty response (no `result`) yields no points.
fn parse_chart_json(company_id: i64, json: &str) -> Result<Vec<PricePoint>, CollectorError> {
    let doc: ChartResponse =
        serde_json::from_str(json).map_err(|e| CollectorError::Parse(e.to_string()))?;
    let Some(result) = doc.chart.result.and_then(|mut r| r.drain(..).next()) else {
        return Ok(Vec::new());
    };
    let (Some(timestamps), Some(quote)) = (result.timestamp, result.indicators.quote.into_iter().next())
    else {
        return Ok(Vec::new());
    };

    let mut points = Vec::new();
    for (i, ts) in timestamps.iter().enumerate() {
        let Some(Some(close)) = quote.close.get(i) else {
            continue; // no trade that day
        };
        let Some(date) = DateTime::from_timestamp(*ts, 0) else {
            continue;
        };
        let at = |v: &Vec<Option<f64>>| v.get(i).copied().flatten();
        points.push(PricePoint {
            company_id,
            date: date.date_naive(),
            open: at(&quote.open),
            high: at(&quote.high),
            low: at(&quote.low),
            close: *close,
            volume: quote.volume.get(i).copied().flatten(),
            source: SOURCE.to_string(),
        });
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::FakeHttp;
    use chrono::{NaiveDate, TimeZone};

    const FIXTURE: &str = include_str!("../../tests/fixtures/yahoo_vmc.json");

    #[test]
    fn chart_url_uppercases_symbol_and_spans_20y() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let url = YahooCollector::<FakeHttp>::chart_url("vmc", now);
        assert!(url.starts_with("https://query1.finance.yahoo.com/v8/finance/chart/VMC?interval=1d&period1="));
        assert!(url.contains(&format!("period2={}", now.timestamp())));
        let p1: i64 = url.split("period1=").nth(1).unwrap().split('&').next().unwrap().parse().unwrap();
        assert!(now.timestamp() - p1 >= 19 * 365 * 86_400); // ~20y back
    }

    #[test]
    fn parses_ohlcv_and_skips_null_days() {
        let p = parse_chart_json(3, FIXTURE).unwrap();
        assert_eq!(p.len(), 2); // 3rd day is all-null -> skipped
        assert_eq!(p[0].date, NaiveDate::from_ymd_opt(2024, 1, 2).unwrap());
        assert_eq!(p[0].open, Some(223.0));
        assert_eq!(p[0].close, 224.8);
        assert_eq!(p[0].volume, Some(1_200_000));
        assert_eq!(p[0].source, "yahoo");
        assert_eq!(p[0].company_id, 3);
    }

    #[test]
    fn parses_when_some_ohlcv_arrays_are_missing() {
        // Yahoo response whose quote omits `open` and `volume` entirely.
        const MISSING: &str = include_str!("../../tests/fixtures/yahoo_missing_open.json");
        let p = parse_chart_json(1, MISSING).unwrap();
        assert_eq!(p.len(), 2); // rows kept (close present)
        assert_eq!(p[0].close, 224.8);
        assert_eq!(p[0].open, None); // missing array -> None
        assert_eq!(p[0].volume, None);
        assert_eq!(p[0].high, Some(225.5));
    }

    #[test]
    fn missing_result_yields_nothing() {
        assert!(parse_chart_json(1, r#"{"chart":{"result":null,"error":"x"}}"#).unwrap().is_empty());
    }

    #[test]
    fn missing_timestamps_yields_nothing() {
        let json = r#"{"chart":{"result":[{"indicators":{"quote":[{"open":[],"high":[],"low":[],"close":[],"volume":[]}]}}]}}"#;
        assert!(parse_chart_json(1, json).unwrap().is_empty());
    }

    #[test]
    fn invalid_json_errors() {
        assert!(matches!(parse_chart_json(1, "nope").unwrap_err(), CollectorError::Parse(_)));
    }

    #[tokio::test]
    async fn fetch_prices_hits_endpoint_then_parses() {
        let c = YahooCollector::new(FakeHttp::new(FIXTURE));
        let target = SourceTarget { cik: "x".into(), symbol: "VMC".into() };
        let p = c.fetch_prices(3, &target).await.unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(c.name(), "yahoo");
        assert!(c.http.url().unwrap().contains("/chart/VMC"));
    }
}
