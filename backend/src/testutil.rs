//! Shared test helpers. Compiled only under `cfg(test)`.

use std::sync::Mutex;

use chrono::{DateTime, TimeZone, Utc};

use crate::collectors::{CollectorError, HttpClient};
use crate::store::Store;

/// An [`HttpClient`] that returns a canned body and records the requested URL.
/// Uses a `Mutex` (not `RefCell`) so it is `Send + Sync` for `dyn` use.
pub struct FakeHttp {
    body: String,
    /// `(url substring, body)` overrides — first match wins; else `body`.
    routes: Vec<(String, String)>,
    last_url: Mutex<Option<String>>,
}

impl FakeHttp {
    pub fn new(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            routes: Vec::new(),
            last_url: Mutex::new(None),
        }
    }

    /// A fake that returns different bodies per URL substring (for multi-call
    /// flows like the Yahoo crumb → assetProfile handshake). Unmatched URLs get
    /// an empty body.
    pub fn routed(routes: &[(&str, &str)]) -> Self {
        Self {
            body: String::new(),
            routes: routes.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            last_url: Mutex::new(None),
        }
    }

    /// The most recently requested URL, if any.
    pub fn url(&self) -> Option<String> {
        self.last_url.lock().unwrap().clone()
    }
}

impl HttpClient for FakeHttp {
    async fn get_text(&self, url: &str) -> Result<String, CollectorError> {
        *self.last_url.lock().unwrap() = Some(url.to_string());
        let body = self
            .routes
            .iter()
            .find(|(k, _)| url.contains(k.as_str()))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| self.body.clone());
        Ok(body)
    }
}

/// A fixed timestamp for deterministic tests (2024-01-01T00:00:00Z).
pub fn fixed_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
}

/// A fresh migrated store backed by a temp file. Keep the `TempDir` alive.
pub async fn temp_store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("test.db").display());
    (Store::connect(&url).await.unwrap(), dir)
}
