//! Real network transport. Thin I/O glue over `reqwest` with retry/backoff and
//! optional rate limiting; excluded from the coverage gate (cannot run offline).

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use reqwest::header::RETRY_AFTER;

use crate::collectors::{CollectorError, FetchOutcome, HttpClient};
use crate::net::{RateLimiter, RetryPolicy, Validators};

/// HTTP client backed by `reqwest`. SEC requires a descriptive User-Agent.
pub struct ReqwestClient {
    client: reqwest::Client,
    policy: RetryPolicy,
    limiter: Option<Arc<RateLimiter>>,
}

impl ReqwestClient {
    /// Client with default retry policy and no shared rate limiter.
    pub fn new(user_agent: &str) -> Self {
        Self::with_limiter(user_agent, RetryPolicy::default(), None)
    }

    /// Client sharing a rate limiter (so concurrent collectors stay polite).
    pub fn with_limiter(
        user_agent: &str,
        policy: RetryPolicy,
        limiter: Option<Arc<RateLimiter>>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            // Persist cookies across requests (needed for the Yahoo crumb handshake:
            // the cookie seeded from fc.yahoo.com authorizes the crumb + profile call).
            .cookie_store(true)
            .build()
            .expect("build reqwest client");
        Self {
            client,
            policy,
            limiter,
        }
    }
}

fn retry_after(resp: &reqwest::Response) -> Option<Duration> {
    resp.headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

fn header_string(resp: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<String> {
    resp.headers().get(name)?.to_str().ok().map(str::to_string)
}

impl ReqwestClient {
    /// GET with retry/backoff + rate limiting; success and 304 both return the
    /// response for the caller to interpret.
    async fn send_with_headers(
        &self,
        url: &str,
        headers: &[(&'static str, String)],
    ) -> Result<reqwest::Response, CollectorError> {
        let mut attempt = 0;
        loop {
            if let Some(rl) = &self.limiter {
                tokio::time::sleep(rl.reserve(Utc::now())).await;
            }
            let mut req = self.client.get(url);
            for (name, value) in headers {
                req = req.header(*name, value);
            }
            match req.send().await {
                Ok(resp)
                    if resp.status().is_success()
                        || resp.status() == reqwest::StatusCode::NOT_MODIFIED =>
                {
                    return Ok(resp);
                }
                Ok(resp) => {
                    let code = resp.status().as_u16();
                    if self.policy.should_retry(attempt, Some(code)) {
                        let wait = self.policy.delay_for(attempt, retry_after(&resp));
                        tokio::time::sleep(wait).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(CollectorError::Http(format!("status {code} from {url}")));
                }
                Err(e) => {
                    if self.policy.should_retry(attempt, None) {
                        tokio::time::sleep(self.policy.delay_for(attempt, None)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(CollectorError::Http(e.to_string()));
                }
            }
        }
    }
}

impl HttpClient for ReqwestClient {
    async fn get_text(&self, url: &str) -> Result<String, CollectorError> {
        let resp = self.send_with_headers(url, &[]).await?;
        resp.text().await.map_err(|e| CollectorError::Http(e.to_string()))
    }

    async fn get_text_with_validators(
        &self,
        url: &str,
        validators: &Validators,
    ) -> Result<FetchOutcome, CollectorError> {
        let resp = self.send_with_headers(url, &validators.request_headers()).await?;
        if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(FetchOutcome::NotModified);
        }
        let validators = Validators {
            etag: header_string(&resp, reqwest::header::ETAG),
            last_modified: header_string(&resp, reqwest::header::LAST_MODIFIED),
        };
        let body = resp.text().await.map_err(|e| CollectorError::Http(e.to_string()))?;
        Ok(FetchOutcome::Modified { body, validators })
    }
}
