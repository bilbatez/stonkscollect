//! Real network transport. This is thin I/O glue over `reqwest` and is
//! excluded from the coverage gate (it cannot run offline in tests).

use crate::collectors::{CollectorError, HttpClient};

/// HTTP client backed by `reqwest`. SEC requires a descriptive User-Agent.
pub struct ReqwestClient {
    client: reqwest::Client,
}

impl ReqwestClient {
    /// Build a client with the given `User-Agent` (e.g. "stonkscollect you@example.com").
    pub fn new(user_agent: &str) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .build()
            .expect("build reqwest client");
        Self { client }
    }
}

impl HttpClient for ReqwestClient {
    async fn get_text(&self, url: &str) -> Result<String, CollectorError> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| CollectorError::Http(e.to_string()))?
            .error_for_status()
            .map_err(|e| CollectorError::Http(e.to_string()))?;
        resp.text()
            .await
            .map_err(|e| CollectorError::Http(e.to_string()))
    }
}
