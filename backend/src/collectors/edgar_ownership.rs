//! Insider ownership from SEC EDGAR Form 4 filings (keyless). Discovers a
//! company's Form 4 filings via the submissions feed, fetches each filing's
//! XML, and extracts the reporting owners and their resulting share counts.
//!
//! 13F institutional holdings are intentionally out of scope here: they are
//! filer-centric (one institution lists many issuers), so answering "who holds
//! X?" needs a full-text-search inversion that warrants its own collector.

use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::Value;

use super::{parse_json, CollectorError, HolderSource, HttpClient, SourceTarget, ISO_DATE};
use crate::domain::OwnershipHolding;

/// Cap on Form 4 filings fetched per company (EDGAR is rate-limited).
const MAX_FORM4: usize = 25;
const FORM4_SOURCE: &str = "edgar-form4";

/// Collects insider positions from a company's recent Form 4 filings.
pub struct OwnershipCollector<H: HttpClient> {
    http: H,
}

impl<H: HttpClient> OwnershipCollector<H> {
    pub fn new(http: H) -> Self {
        Self { http }
    }

    /// The submissions feed URL listing a company's recent filings.
    fn submissions_url(cik: &str) -> String {
        format!("https://data.sec.gov/submissions/CIK{cik:0>10}.json")
    }

    /// Raw-XML URL for one filing. Strips EDGAR's human-readable `xslF345X05/`
    /// wrapper (the styled view) to reach the machine-readable document.
    fn filing_url(cik: &str, accession: &str, doc: &str) -> String {
        let cik_int = cik.trim_start_matches('0');
        let acc = accession.replace('-', "");
        let doc = doc.rsplit('/').next().unwrap_or(doc);
        format!("https://www.sec.gov/Archives/edgar/data/{cik_int}/{acc}/{doc}")
    }
}

#[async_trait(?Send)]
impl<H: HttpClient> HolderSource for OwnershipCollector<H> {
    fn name(&self) -> &'static str {
        "edgar-form4"
    }

    async fn fetch_holders(
        &self,
        company_id: i64,
        target: &SourceTarget,
    ) -> Result<Vec<OwnershipHolding>, CollectorError> {
        let subs = self.http.get_text(&Self::submissions_url(&target.cik)).await?;
        let filings = parse_form4_filings(&subs)?;
        let mut out = Vec::new();
        for (accession, doc) in filings.into_iter().take(MAX_FORM4) {
            let url = Self::filing_url(&target.cik, &accession, &doc);
            let xml = self.http.get_text(&url).await?;
            if let Some(f) = parse_form4(&xml) {
                for holder in f.owners {
                    out.push(OwnershipHolding {
                        company_id,
                        holder,
                        kind: "insider".into(),
                        shares: f.shares,
                        as_of: f.as_of,
                        source: FORM4_SOURCE.into(),
                    });
                }
            }
        }
        Ok(out)
    }
}

/// The (accession, primaryDocument) pairs of the `form == "4"` filings in a
/// submissions feed. Missing/garbled arrays yield an empty list (not an error).
fn parse_form4_filings(json: &str) -> Result<Vec<(String, String)>, CollectorError> {
    let doc: Value = parse_json(json)?;
    let recent = &doc["filings"]["recent"];
    let (Some(forms), Some(accs), Some(docs)) = (
        recent["form"].as_array(),
        recent["accessionNumber"].as_array(),
        recent["primaryDocument"].as_array(),
    ) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (i, form) in forms.iter().enumerate() {
        if form.as_str() != Some("4") {
            continue;
        }
        if let (Some(acc), Some(d)) = (
            accs.get(i).and_then(Value::as_str),
            docs.get(i).and_then(Value::as_str),
        ) {
            out.push((acc.to_string(), d.to_string()));
        }
    }
    Ok(out)
}

/// One parsed Form 4: its reporting owners, their resulting share count, and the
/// report period.
struct Form4 {
    owners: Vec<String>,
    shares: f64,
    as_of: NaiveDate,
}

/// Extract reporting owners, the latest post-transaction share count, and the
/// report date from a Form 4 XML body. `None` if any of those is absent.
fn parse_form4(xml: &str) -> Option<Form4> {
    let owners: Vec<String> = inner_all(xml, "<rptOwnerName>", "</rptOwnerName>")
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if owners.is_empty() {
        return None;
    }
    let as_of = inner(xml, "<periodOfReport>", "</periodOfReport>")
        .and_then(|s| NaiveDate::parse_from_str(s.trim(), ISO_DATE).ok())?;
    let shares = last_shares(xml)?;
    Some(Form4 { owners, shares, as_of })
}

/// Text between the first `open`/`close` tag pair, if present.
fn inner<'a>(xml: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = xml.find(open)? + open.len();
    let rest = &xml[start..];
    let end = rest.find(close)?;
    Some(&rest[..end])
}

/// Text of every `open`/`close` tag pair, in document order.
fn inner_all<'a>(xml: &'a str, open: &str, close: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(s) = xml[pos..].find(open) {
        let start = pos + s + open.len();
        let Some(e) = xml[start..].find(close) else {
            break;
        };
        out.push(&xml[start..start + e]);
        pos = start + e + close.len();
    }
    out
}

/// The `<value>` of the last `sharesOwnedFollowingTransaction` block — the
/// owner's most recently reported resulting position.
fn last_shares(xml: &str) -> Option<f64> {
    let anchor = "<sharesOwnedFollowingTransaction>";
    let mut last = None;
    let mut pos = 0;
    while let Some(s) = xml[pos..].find(anchor) {
        let start = pos + s + anchor.len();
        if let Some(v) = inner(&xml[start..], "<value>", "</value>") {
            last = v.trim().parse::<f64>().ok();
        }
        pos = start;
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::FakeHttp;

    const SUBMISSIONS: &str = include_str!("../../tests/fixtures/edgar_ownership_submissions.json");
    const FORM4_SINGLE: &str = include_str!("../../tests/fixtures/edgar_form4_single.xml");
    const FORM4_JOINT: &str = include_str!("../../tests/fixtures/edgar_form4_joint.xml");

    #[test]
    fn submissions_url_zero_pads_cik() {
        assert_eq!(
            OwnershipCollector::<FakeHttp>::submissions_url("320193"),
            "https://data.sec.gov/submissions/CIK0000320193.json"
        );
    }

    #[test]
    fn filing_url_strips_xsl_wrapper_and_dashes() {
        assert_eq!(
            OwnershipCollector::<FakeHttp>::filing_url("0000320193", "0000320193-24-000010", "xslF345X05/wf-form4_001.xml"),
            "https://www.sec.gov/Archives/edgar/data/320193/000032019324000010/wf-form4_001.xml"
        );
        // a doc with no wrapper segment is used verbatim
        assert_eq!(
            OwnershipCollector::<FakeHttp>::filing_url("320193", "0000320193-24-000011", "wf-form4_002.xml"),
            "https://www.sec.gov/Archives/edgar/data/320193/000032019324000011/wf-form4_002.xml"
        );
    }

    #[test]
    fn parse_form4_filings_keeps_only_form_4() {
        let f = parse_form4_filings(SUBMISSIONS).unwrap();
        assert_eq!(f.len(), 2); // the 10-K is skipped
        assert_eq!(f[0].0, "0000320193-24-000010");
        assert_eq!(f[0].1, "xslF345X05/wf-form4_001.xml");
        assert_eq!(f[1].1, "wf-form4_002.xml");
    }

    #[test]
    fn parse_form4_filings_tolerates_missing_arrays_and_bad_json() {
        assert!(parse_form4_filings("{}").unwrap().is_empty());
        assert!(parse_form4_filings("not json").is_err());
    }

    #[test]
    fn parse_form4_reads_owner_latest_shares_and_period() {
        let f = parse_form4(FORM4_SINGLE).unwrap();
        assert_eq!(f.owners, vec!["Cook Timothy D".to_string()]);
        assert_eq!(f.shares, 3_250_000.0); // last post-transaction amount, not 3_280_000
        assert_eq!(f.as_of, NaiveDate::from_ymd_opt(2024, 2, 1).unwrap());
    }

    #[test]
    fn parse_form4_handles_multiple_owners() {
        let f = parse_form4(FORM4_JOINT).unwrap();
        assert_eq!(f.owners, vec!["Williams Jeffrey E".to_string(), "Maestri Luca".to_string()]);
        assert_eq!(f.shares, 500_000.0);
    }

    #[test]
    fn parse_form4_returns_none_when_incomplete() {
        assert!(parse_form4("<ownershipDocument></ownershipDocument>").is_none()); // no owner
        // owner present but no shares / no period
        assert!(parse_form4("<rptOwnerName>X</rptOwnerName><periodOfReport>2024-01-01</periodOfReport>").is_none());
        // owner + shares but unparseable period
        assert!(parse_form4(
            "<rptOwnerName>X</rptOwnerName><periodOfReport>nope</periodOfReport>\
             <sharesOwnedFollowingTransaction><value>1</value></sharesOwnedFollowingTransaction>"
        )
        .is_none());
    }

    #[tokio::test]
    async fn fetch_holders_collects_owners_across_form4_filings() {
        let http = FakeHttp::routed(&[
            ("submissions", SUBMISSIONS),
            ("wf-form4_001", FORM4_SINGLE),
            ("wf-form4_002", FORM4_JOINT),
        ]);
        let collector = OwnershipCollector::new(http);
        let target = SourceTarget { cik: "320193".into(), symbol: "AAPL".into() };
        let holders = collector.fetch_holders(7, &target).await.unwrap();
        assert_eq!(collector.name(), "edgar-form4");
        // 1 owner from the single filing + 2 from the joint filing
        assert_eq!(holders.len(), 3);
        let cook = holders.iter().find(|h| h.holder == "Cook Timothy D").unwrap();
        assert_eq!(cook.company_id, 7);
        assert_eq!(cook.shares, 3_250_000.0);
        assert_eq!(cook.kind, "insider");
        assert_eq!(cook.source, "edgar-form4");
        assert!(holders.iter().any(|h| h.holder == "Maestri Luca" && h.shares == 500_000.0));
    }
}
