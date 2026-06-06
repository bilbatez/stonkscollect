//! Tiered scheduling + run-tracking. The cron expressions and next-fire
//! computation are pure and tested here; the live driver loop lives in the
//! binary (bootstrap glue).

use std::fmt::Display;
use std::future::Future;
use std::str::FromStr;

use chrono::{DateTime, Utc};

use crate::store::Store;

/// Collection cadence tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Daily after US market close.
    Price,
    /// Several times per day.
    News,
    /// Weekly (plus event-driven on new filings, handled elsewhere).
    Fundamentals,
}

impl Tier {
    /// All tiers, for registering jobs.
    pub fn all() -> [Tier; 3] {
        [Tier::Price, Tier::News, Tier::Fundamentals]
    }

    /// 6-field cron expression (sec min hour dom mon dow), UTC.
    pub fn cron(self) -> &'static str {
        match self {
            Tier::Price => "0 0 21 * * *",
            Tier::News => "0 0 0,6,12,18 * * *",
            Tier::Fundamentals => "0 0 6 * * Mon",
        }
    }

    /// Stable label used as the `source`/scope tag in collection runs.
    pub fn label(self) -> &'static str {
        match self {
            Tier::Price => "price",
            Tier::News => "news",
            Tier::Fundamentals => "fundamentals",
        }
    }

    /// Next scheduled fire strictly after `now`.
    pub fn next_after(self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        cron::Schedule::from_str(self.cron())
            .ok()
            .and_then(|s| s.after(&now).next())
    }
}

/// Run `task`, recording start/finish in `collection_runs`. Run-tracking is
/// best-effort observability: failures to record never alter or abort the
/// task; the task's own `Result` is returned unchanged.
pub async fn run_tracked<F, Fut, T, E>(
    store: &Store,
    source: &str,
    scope: Option<&str>,
    now: DateTime<Utc>,
    task: F,
) -> Result<T, E>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Display,
{
    // Best-effort observability: if the run can't be registered, still run the
    // task. When it is registered, record the outcome (ignoring record errors).
    match store.start_run(source, scope, now).await {
        Ok(id) => {
            let result = task().await;
            let (status, error) = match &result {
                Ok(_) => ("ok", None),
                Err(e) => ("error", Some(e.to_string())),
            };
            let _ = store.finish_run(id, status, now, error.as_deref()).await;
            result
        }
        Err(_) => task().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, 0, 0).unwrap()
    }

    #[test]
    fn tiers_expose_cron_and_label() {
        assert_eq!(Tier::Price.cron(), "0 0 21 * * *");
        assert_eq!(Tier::News.cron(), "0 0 0,6,12,18 * * *");
        assert_eq!(Tier::Fundamentals.cron(), "0 0 6 * * Mon");
        let labels: Vec<_> = Tier::all().iter().map(|t| t.label()).collect();
        assert_eq!(labels, ["price", "news", "fundamentals"]);
    }

    #[test]
    fn next_after_computes_each_tier() {
        // 2024-01-01 is a Monday.
        assert_eq!(Tier::Price.next_after(at(2024, 1, 1, 0)), Some(at(2024, 1, 1, 21)));
        assert_eq!(Tier::News.next_after(at(2024, 1, 1, 0)), Some(at(2024, 1, 1, 6)));
        assert_eq!(
            Tier::Fundamentals.next_after(at(2024, 1, 1, 0)),
            Some(at(2024, 1, 1, 6))
        );
    }

    async fn store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join("t.db").display());
        (Store::connect(&url).await.unwrap(), dir)
    }

    #[tokio::test]
    async fn run_tracked_records_success() {
        let (s, _d) = store().await;
        let now = at(2024, 1, 1, 0);
        let out: u32 = run_tracked(&s, "price", Some("AAPL"), now, || async { Ok::<_, String>(42) })
            .await
            .unwrap();
        assert_eq!(out, 42);
        let runs = s.recent_runs(1).await.unwrap();
        assert_eq!(runs[0].status, "ok");
        assert_eq!(runs[0].error, None);
    }

    #[tokio::test]
    async fn run_tracked_tolerates_recording_failure() {
        // With the store closed, start_run fails; the task must still run and
        // its result is returned unchanged (best-effort observability).
        let (s, _d) = store().await;
        s.close().await;
        let out: u32 = run_tracked(&s, "price", None, at(2024, 1, 1, 0), || async {
            Ok::<_, String>(7)
        })
        .await
        .unwrap();
        assert_eq!(out, 7);
    }

    #[tokio::test]
    async fn run_tracked_records_failure_and_returns_task_error() {
        let (s, _d) = store().await;
        let now = at(2024, 1, 1, 0);
        let err = run_tracked(&s, "news", None, now, || async {
            Err::<u32, String>("boom".to_string())
        })
        .await
        .unwrap_err();
        assert_eq!(err, "boom");
        let runs = s.recent_runs(1).await.unwrap();
        assert_eq!(runs[0].status, "error");
        assert_eq!(runs[0].error, Some("boom".into()));
    }
}
