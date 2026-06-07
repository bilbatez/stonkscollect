//! Runtime configuration, parsed from a key getter (env in production).

/// Application configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub user_agent: String,
    pub fmp_api_key: Option<String>,
    pub finnhub_api_key: Option<String>,
    /// Tickers to collect (from `TICKERS`, comma-separated).
    pub tickers: Vec<String>,
    /// Collect the entire bootstrapped US universe (ignores `tickers`).
    pub collect_all: bool,
    /// Milliseconds to wait between companies in bulk collection (politeness).
    pub request_delay_ms: u64,
    /// Relative threshold above which cross-source values are flagged.
    pub reconcile_threshold: f64,
}

impl Config {
    /// Parse configuration from a key lookup function (e.g. `std::env::var`).
    /// Missing keys fall back to sensible defaults.
    pub fn parse<F: Fn(&str) -> Option<String>>(get: F) -> Self {
        Config {
            database_url: get("DATABASE_URL")
                .unwrap_or_else(|| "sqlite:///data/stonks.db".to_string()),
            port: get("PORT").and_then(|p| p.parse().ok()).unwrap_or(8080),
            user_agent: get("USER_AGENT")
                .unwrap_or_else(|| "stonkscollect (contact@example.com)".to_string()),
            fmp_api_key: get("FMP_API_KEY"),
            finnhub_api_key: get("FINNHUB_API_KEY"),
            tickers: get("TICKERS").map(|s| parse_tickers(&s)).unwrap_or_default(),
            collect_all: get("COLLECT_ALL").is_some_and(|v| is_truthy(&v)),
            request_delay_ms: get("REQUEST_DELAY_MS")
                .and_then(|d| d.parse().ok())
                .unwrap_or(150),
            reconcile_threshold: get("RECONCILE_THRESHOLD")
                .and_then(|t| t.parse().ok())
                .unwrap_or(0.05),
        }
    }
}

/// Interpret common truthy strings.
fn is_truthy(v: &str) -> bool {
    matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes")
}

/// Split a comma-separated list into trimmed, non-empty, upper-cased tickers.
fn parse_tickers(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|t| t.trim().to_uppercase())
        .filter(|t| !t.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn getter(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn parses_a_full_configuration() {
        let cfg = Config::parse(getter(&[
            ("DATABASE_URL", "sqlite://x.db"),
            ("PORT", "9000"),
            ("USER_AGENT", "me"),
            ("FMP_API_KEY", "fk"),
            ("FINNHUB_API_KEY", "nk"),
            ("TICKERS", "aapl, msft ,"),
            ("COLLECT_ALL", "TRUE"),
            ("REQUEST_DELAY_MS", "200"),
            ("RECONCILE_THRESHOLD", "0.1"),
        ]));
        assert_eq!(cfg.database_url, "sqlite://x.db");
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.user_agent, "me");
        assert_eq!(cfg.fmp_api_key, Some("fk".into()));
        assert_eq!(cfg.finnhub_api_key, Some("nk".into()));
        assert_eq!(cfg.tickers, vec!["AAPL", "MSFT"]);
        assert!(cfg.collect_all);
        assert_eq!(cfg.request_delay_ms, 200);
        assert_eq!(cfg.reconcile_threshold, 0.1);
        assert_eq!(cfg.clone(), cfg);
    }

    #[test]
    fn applies_defaults_when_keys_absent() {
        let cfg = Config::parse(|_| None);
        assert_eq!(cfg.database_url, "sqlite:///data/stonks.db");
        assert_eq!(cfg.port, 8080);
        assert!(cfg.user_agent.contains("stonkscollect"));
        assert_eq!(cfg.fmp_api_key, None);
        assert_eq!(cfg.finnhub_api_key, None);
        assert!(cfg.tickers.is_empty());
        assert!(!cfg.collect_all);
        assert_eq!(cfg.request_delay_ms, 150);
        assert_eq!(cfg.reconcile_threshold, 0.05);
    }

    #[test]
    fn invalid_numeric_values_fall_back_to_defaults() {
        let cfg = Config::parse(getter(&[
            ("PORT", "nope"),
            ("REQUEST_DELAY_MS", "x"),
            ("COLLECT_ALL", "maybe"),
            ("RECONCILE_THRESHOLD", "x"),
        ]));
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.request_delay_ms, 150);
        assert!(!cfg.collect_all); // "maybe" is not truthy
        assert_eq!(cfg.reconcile_threshold, 0.05);
    }
}
