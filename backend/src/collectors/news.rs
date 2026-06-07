//! News collectors: RSS feeds + Finnhub company-news. Title + description only,
//! deduplicated by normalized headline so the same story from multiple sources
//! collapses to one row.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use crate::collectors::{CollectorError, HttpClient};
use crate::domain::NewsItem;

/// Normalize a headline for dedup: collapse whitespace, lowercase.
fn normalize(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Stable hex hash of a normalized headline, used to dedup across sources.
pub fn dedup_hash(title: &str) -> String {
    let mut hasher = DefaultHasher::new();
    normalize(title).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Parse an RSS 2.0 feed into news items. Items without a title are skipped;
/// items without a date fall back to `now`.
fn parse_rss(
    company_id: i64,
    xml: &str,
    source_label: &str,
    now: DateTime<Utc>,
) -> Result<Vec<NewsItem>, CollectorError> {
    let channel =
        rss::Channel::read_from(xml.as_bytes()).map_err(|e| CollectorError::Parse(e.to_string()))?;
    let mut items = Vec::new();
    for item in channel.items() {
        let Some(title) = item.title() else {
            continue;
        };
        let published_at = item
            .pub_date()
            .and_then(|d| DateTime::parse_from_rfc2822(d).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or(now);
        items.push(NewsItem {
            company_id,
            title: title.to_string(),
            description: item.description().map(str::to_string),
            url: item.link().unwrap_or("").to_string(),
            source: source_label.to_string(),
            published_at,
            dedup_hash: dedup_hash(title),
        });
    }
    Ok(items)
}

#[derive(Deserialize)]
struct FinnhubItem {
    datetime: i64,
    headline: String,
    summary: String,
    source: String,
    url: String,
}

/// Parse Finnhub company-news JSON into news items.
fn parse_finnhub(
    company_id: i64,
    json: &str,
    now: DateTime<Utc>,
) -> Result<Vec<NewsItem>, CollectorError> {
    let rows: Vec<FinnhubItem> =
        serde_json::from_str(json).map_err(|e| CollectorError::Parse(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let published_at = Utc.timestamp_opt(r.datetime, 0).single().unwrap_or(now);
            let description = if r.summary.is_empty() {
                None
            } else {
                Some(r.summary)
            };
            let dedup_hash = dedup_hash(&r.headline);
            NewsItem {
                company_id,
                title: r.headline,
                description,
                url: r.url,
                source: r.source,
                published_at,
                dedup_hash,
            }
        })
        .collect())
}

/// Collects headlines from an RSS feed.
pub struct RssCollector<H: HttpClient> {
    http: H,
}

impl<H: HttpClient> RssCollector<H> {
    pub fn new(http: H) -> Self {
        Self { http }
    }

    pub async fn collect(
        &self,
        company_id: i64,
        feed_url: &str,
        source_label: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<NewsItem>, CollectorError> {
        let body = self.http.get_text(feed_url).await?;
        parse_rss(company_id, &body, source_label, now)
    }
}

/// Collects headlines from the Finnhub company-news API.
pub struct FinnhubCollector<H: HttpClient> {
    http: H,
    api_key: String,
}

impl<H: HttpClient> FinnhubCollector<H> {
    pub fn new(http: H, api_key: String) -> Self {
        Self { http, api_key }
    }

    pub fn news_url(symbol: &str, from: &str, to: &str, token: &str) -> String {
        format!("https://finnhub.io/api/v1/company-news?symbol={symbol}&from={from}&to={to}&token={token}")
    }

    pub async fn collect(
        &self,
        company_id: i64,
        symbol: &str,
        from: &str,
        to: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<NewsItem>, CollectorError> {
        let url = Self::news_url(symbol, from, to, &self.api_key);
        let body = self.http.get_text(&url).await?;
        parse_finnhub(company_id, &body, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::CollectorError;
    use crate::testutil::{fixed_now as now, FakeHttp};
    use chrono::{TimeZone, Utc};

    const RSS: &str = include_str!("../../tests/fixtures/news_rss.xml");
    const FINNHUB: &str = include_str!("../../tests/fixtures/news_finnhub.json");

    #[test]
    fn dedup_hash_normalizes_case_and_whitespace() {
        assert_eq!(dedup_hash("Apple  Hits   HIGH "), dedup_hash("apple hits high"));
        assert_ne!(dedup_hash("Apple up"), dedup_hash("Apple down"));
    }

    #[test]
    fn parse_rss_maps_items_and_skips_titleless() {
        let items = parse_rss(7, RSS, "reuters", now()).unwrap();
        assert_eq!(items.len(), 2);
        let first = &items[0];
        assert_eq!(first.title, "Apple reaches record high");
        assert_eq!(first.description, Some("Shares climbed after strong results.".into()));
        assert_eq!(first.url, "https://example.com/a");
        assert_eq!(first.source, "reuters");
        assert_eq!(first.company_id, 7);
        assert_eq!(first.published_at, Utc.with_ymd_and_hms(2024, 1, 2, 12, 0, 0).unwrap());
        assert_eq!(first.dedup_hash, dedup_hash("Apple reaches record high"));
        // minimal item: no description, no link, no date -> falls back to `now`
        let minimal = &items[1];
        assert_eq!(minimal.title, "Minimal item");
        assert_eq!(minimal.description, None);
        assert_eq!(minimal.url, "");
        assert_eq!(minimal.published_at, now());
    }

    #[test]
    fn parse_rss_invalid_errors() {
        assert!(matches!(parse_rss(7, "<not-rss", "x", now()).unwrap_err(), CollectorError::Parse(_)));
    }

    #[test]
    fn parse_finnhub_maps_items() {
        let items = parse_finnhub(7, FINNHUB, now()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Apple hits new high");
        assert_eq!(items[0].description, Some("Strong quarter.".into()));
        assert_eq!(items[0].source, "Reuters");
        assert_eq!(items[0].published_at, Utc.with_ymd_and_hms(2024, 1, 2, 12, 0, 0).unwrap());
        // empty summary becomes None
        assert_eq!(items[1].description, None);
        assert_eq!(items[1].dedup_hash, dedup_hash("Analysts weigh in"));
    }

    #[test]
    fn parse_finnhub_invalid_errors() {
        assert!(matches!(parse_finnhub(7, "nope", now()).unwrap_err(), CollectorError::Parse(_)));
    }

    #[test]
    fn finnhub_url_includes_symbol_range_and_token() {
        let url = FinnhubCollector::<FakeHttp>::news_url("AAPL", "2024-01-01", "2024-01-31", "KEY");
        assert!(url.contains("symbol=AAPL"));
        assert!(url.contains("from=2024-01-01"));
        assert!(url.contains("to=2024-01-31"));
        assert!(url.ends_with("token=KEY"));
    }

    #[tokio::test]
    async fn rss_collector_fetches_then_parses() {
        let c = RssCollector::new(FakeHttp::new(RSS));
        let items = c.collect(7, "https://feed", "reuters", now()).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(c.http.url().as_deref(), Some("https://feed"));
    }

    #[tokio::test]
    async fn finnhub_collector_fetches_then_parses() {
        let c = FinnhubCollector::new(FakeHttp::new(FINNHUB), "KEY".into());
        let items = c.collect(7, "AAPL", "2024-01-01", "2024-01-31", now()).await.unwrap();
        assert_eq!(items.len(), 2);
        assert!(c.http.url().unwrap().contains("symbol=AAPL"));
    }
}
