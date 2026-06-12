//! Yahoo Finance daily-prices collector — a keyless (no API key) price source.
//!
//! Yahoo's chart API returns daily OHLCV as JSON at
//! `https://query1.finance.yahoo.com/v8/finance/chart/<SYMBOL>?interval=1d&range=5y`.
//! Transport is injected via [`HttpClient`] so it's tested offline against a
//! captured fixture. (Replaces the earlier Stooq source, which began serving a
//! JavaScript anti-bot challenge instead of CSV.)

use std::sync::Mutex;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::Value;

use async_trait::async_trait;

use crate::collectors::{
    employee_count, nonempty, parse_json, CollectorError, HttpClient, PriceSource, ProfileSource,
    SourceTarget,
};
use crate::domain::{CompanyProfile, PricePoint};

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

    /// Daily-chart URL up to `now`, starting at `since` when given (incremental
    /// refresh) or ~20y back otherwise. Uses explicit period1/period2 epochs
    /// (with `interval=1d`) rather than `range=max`, which Yahoo downsamples to
    /// monthly for long spans.
    pub fn chart_url(symbol: &str, now: DateTime<Utc>, since: Option<NaiveDate>) -> String {
        let period2 = now.timestamp();
        let period1 = match since.and_then(|d| d.and_hms_opt(0, 0, 0)) {
            Some(midnight) => midnight.and_utc().timestamp(),
            None => (now - Duration::days(Self::HISTORY_DAYS)).timestamp(),
        };
        format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&period1={period1}&period2={period2}",
            symbol.to_uppercase()
        )
    }

    pub async fn collect_prices(
        &self,
        company_id: i64,
        symbol: &str,
        now: DateTime<Utc>,
        since: Option<NaiveDate>,
    ) -> Result<Vec<PricePoint>, CollectorError> {
        let body = self.http.get_text(&Self::chart_url(symbol, now, since)).await?;
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
        now: DateTime<Utc>,
        since: Option<NaiveDate>,
    ) -> Result<Vec<PricePoint>, CollectorError> {
        self.collect_prices(company_id, &target.symbol, now, since).await
    }
}

/// Parse Yahoo's chart JSON into price points. Days with a null close (no trade)
/// are skipped; an error/empty response (no `result`) yields no points.
fn parse_chart_json(company_id: i64, json: &str) -> Result<Vec<PricePoint>, CollectorError> {
    let doc: ChartResponse = parse_json(json)?;
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

/// Collects company-profile metadata from Yahoo's `quoteSummary?modules=assetProfile`,
/// which requires a cookie + crumb handshake. Keyless. (The cookie is held by the
/// underlying reqwest client's cookie store; the crumb is cached here per run.)
pub struct YahooProfileCollector<H: HttpClient> {
    http: H,
    crumb: Mutex<Option<String>>,
}

impl<H: HttpClient> YahooProfileCollector<H> {
    pub fn new(http: H) -> Self {
        Self { http, crumb: Mutex::new(None) }
    }

    pub fn profile_url(symbol: &str, crumb: &str) -> String {
        format!(
            "https://query1.finance.yahoo.com/v10/finance/quoteSummary/{}?modules=assetProfile&crumb={}",
            symbol.to_uppercase(),
            crumb
        )
    }

    /// Fetch (and cache) the Yahoo crumb, seeding the session cookie first.
    async fn crumb(&self) -> Result<String, CollectorError> {
        if let Some(c) = self.crumb.lock().unwrap().clone() {
            return Ok(c);
        }
        // Best-effort cookie seed; the reqwest cookie store keeps it for the crumb call.
        let _ = self.http.get_text("https://fc.yahoo.com").await;
        let c = self.http.get_text("https://query1.finance.yahoo.com/v1/test/getcrumb").await?;
        // A real crumb is a single whitespace-free token; reject error bodies like
        // "Too Many Requests" so we don't cache and reuse garbage.
        if c.split_whitespace().count() != 1 {
            return Err(CollectorError::Http(format!("bad crumb: {c}")));
        }
        *self.crumb.lock().unwrap() = Some(c.clone());
        Ok(c)
    }
}

#[async_trait(?Send)]
impl<H: HttpClient> ProfileSource for YahooProfileCollector<H> {
    fn name(&self) -> &'static str {
        SOURCE
    }

    async fn fetch_profile(&self, target: &SourceTarget) -> Result<CompanyProfile, CollectorError> {
        let crumb = self.crumb().await?;
        let body = self.http.get_text(&Self::profile_url(&target.symbol, &crumb)).await?;
        parse_asset_profile(&body)
    }
}

/// Parse a Yahoo `assetProfile` response into a [`CompanyProfile`]. Missing/empty
/// fields become `None`; a result-less response yields an empty profile.
fn parse_asset_profile(json: &str) -> Result<CompanyProfile, CollectorError> {
    let doc: Value = parse_json(json)?;
    let p = &doc["quoteSummary"]["result"][0]["assetProfile"];
    Ok(CompanyProfile {
        sector: nonempty(&p["sector"]),
        industry: nonempty(&p["industry"]),
        exchange: None,
        website: nonempty(&p["website"]),
        description: nonempty(&p["longBusinessSummary"]),
        employees: employee_count(&p["fullTimeEmployees"]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::FakeHttp;
    use chrono::{NaiveDate, TimeZone};

    const PROFILE: &str = include_str!("../../tests/fixtures/yahoo_assetprofile.json");

    #[test]
    fn parse_asset_profile_maps_fields() {
        let p = parse_asset_profile(PROFILE).unwrap();
        assert_eq!(p.sector.as_deref(), Some("Basic Materials"));
        assert_eq!(p.industry.as_deref(), Some("Building Materials"));
        assert_eq!(p.website.as_deref(), Some("https://www.vulcanmaterials.com"));
        assert!(p.description.unwrap().starts_with("Vulcan Materials Company produces"));
        assert_eq!(p.exchange, None);
        assert_eq!(p.employees, Some(10961));
    }

    #[test]
    fn parse_asset_profile_empty_result_is_blank() {
        let p = parse_asset_profile(r#"{"quoteSummary":{"result":[],"error":"x"}}"#).unwrap();
        assert_eq!(p, CompanyProfile::default());
    }

    #[test]
    fn parse_asset_profile_invalid_json_errors() {
        assert!(matches!(parse_asset_profile("nope").unwrap_err(), CollectorError::Parse(_)));
    }

    #[test]
    fn profile_url_uppercases_and_carries_crumb() {
        assert_eq!(
            YahooProfileCollector::<FakeHttp>::profile_url("vmc", "CR1"),
            "https://query1.finance.yahoo.com/v10/finance/quoteSummary/VMC?modules=assetProfile&crumb=CR1"
        );
    }

    #[tokio::test]
    async fn fetch_profile_rejects_a_garbage_crumb() {
        // Yahoo rate-limits with a "Too Many Requests" body instead of a crumb.
        let c = YahooProfileCollector::new(FakeHttp::routed(&[("getcrumb", "Too Many Requests")]));
        let target = SourceTarget { cik: "x".into(), symbol: "VMC".into() };
        assert!(matches!(c.fetch_profile(&target).await.unwrap_err(), CollectorError::Http(_)));
    }

    #[tokio::test]
    async fn fetch_profile_does_crumb_handshake_then_parses_and_caches() {
        let c = YahooProfileCollector::new(FakeHttp::routed(&[
            ("getcrumb", "CRUMB123"),
            ("quoteSummary", PROFILE),
        ]));
        let target = SourceTarget { cik: "x".into(), symbol: "VMC".into() };
        let p = c.fetch_profile(&target).await.unwrap();
        assert_eq!(p.sector.as_deref(), Some("Basic Materials"));
        assert_eq!(ProfileSource::name(&c), "yahoo");
        assert!(c.http.url().unwrap().contains("crumb=CRUMB123"));
        // second call reuses the cached crumb (no re-fetch); still resolves
        let p2 = c.fetch_profile(&target).await.unwrap();
        assert_eq!(p2.industry.as_deref(), Some("Building Materials"));
    }

    const FIXTURE: &str = include_str!("../../tests/fixtures/yahoo_vmc.json");

    #[test]
    fn chart_url_uppercases_symbol_and_spans_20y() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let url = YahooCollector::<FakeHttp>::chart_url("vmc", now, None);
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

    #[test]
    fn chart_url_starts_at_since_when_given() {
        let now = Utc.with_ymd_and_hms(2024, 3, 1, 12, 0, 0).unwrap();
        let since = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        let url = YahooCollector::<FakeHttp>::chart_url("aapl", now, Some(since));
        let midnight = since.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        assert!(url.contains(&format!("period1={midnight}")), "{url}");
        assert!(url.contains(&format!("period2={}", now.timestamp())));
    }

    #[tokio::test]
    async fn fetch_prices_hits_endpoint_then_parses() {
        let c = YahooCollector::new(FakeHttp::new(FIXTURE));
        let target = SourceTarget { cik: "x".into(), symbol: "VMC".into() };
        let p = c.fetch_prices(3, &target, Utc::now(), None).await.unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(c.name(), "yahoo");
        assert!(c.http.url().unwrap().contains("/chart/VMC"));
    }
}
