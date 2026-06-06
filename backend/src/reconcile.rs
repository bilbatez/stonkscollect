//! Cross-source reconciliation. Pure logic, no I/O.
//!
//! Given financial facts for one company from multiple sources, pick a
//! canonical value per (statement, line item, period type, period end) —
//! preferring SEC EDGAR — and flag any source that disagrees beyond a
//! relative threshold.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::domain::{Discrepancy, FinancialFact};

/// The canonical source whose value wins when present.
const CANONICAL_SOURCE: &str = "edgar";

/// Outcome of reconciling facts from multiple sources.
#[derive(Debug, Clone, PartialEq)]
pub struct ReconcileResult {
    /// One canonical fact per (statement, line item, period type, period end).
    pub canonical: Vec<FinancialFact>,
    /// Cross-source mismatches exceeding the threshold.
    pub discrepancies: Vec<Discrepancy>,
}

/// Relative difference of two values, normalized to the larger magnitude.
/// Returns 0.0 when both are zero.
fn relative_diff(a: f64, b: f64) -> f64 {
    let denom = a.abs().max(b.abs());
    if denom == 0.0 {
        0.0
    } else {
        (a - b).abs() / denom
    }
}

/// Reconcile `facts`, preferring [`CANONICAL_SOURCE`], flagging any source
/// that differs from the canonical value by more than `threshold` (relative).
pub fn reconcile(facts: &[FinancialFact], threshold: f64, now: DateTime<Utc>) -> ReconcileResult {
    // Group by the natural fact key. BTreeMap gives deterministic ordering.
    let mut groups: BTreeMap<(&str, &str, &str, chrono::NaiveDate), Vec<&FinancialFact>> =
        BTreeMap::new();
    for f in facts {
        let key = (
            f.statement.as_str(),
            f.period_type.as_str(),
            f.line_item.as_str(),
            f.period_end,
        );
        groups.entry(key).or_default().push(f);
    }

    let mut canonical = Vec::new();
    let mut discrepancies = Vec::new();

    for group in groups.values() {
        // Pick canonical: EDGAR if present, else the lexicographically smallest source.
        let chosen = group
            .iter()
            .find(|f| f.source == CANONICAL_SOURCE)
            .copied()
            .unwrap_or_else(|| {
                group
                    .iter()
                    .min_by(|a, b| a.source.cmp(&b.source))
                    .copied()
                    .expect("non-empty group")
            });
        canonical.push((*chosen).clone());

        for other in group {
            if std::ptr::eq(*other, chosen) {
                continue;
            }
            let pct_diff = relative_diff(chosen.value, other.value);
            if pct_diff > threshold {
                discrepancies.push(Discrepancy {
                    company_id: chosen.company_id,
                    field: chosen.line_item.clone(),
                    period: Some(chosen.period_end.to_string()),
                    source_a: chosen.source.clone(),
                    value_a: chosen.value,
                    source_b: other.source.clone(),
                    value_b: other.value,
                    pct_diff,
                    flagged_at: now,
                });
            }
        }
    }

    ReconcileResult {
        canonical,
        discrepancies,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FinancialFact, PeriodType, StatementKind};
    use chrono::{NaiveDate, TimeZone, Utc};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
    }

    fn fact(source: &str, item: &str, value: f64) -> FinancialFact {
        FinancialFact {
            company_id: 1,
            statement: StatementKind::Income,
            line_item: item.to_string(),
            period_type: PeriodType::Annual,
            period_end: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            value,
            source: source.to_string(),
            fetched_at: now(),
        }
    }

    #[test]
    fn single_source_is_canonical_without_discrepancies() {
        let facts = vec![fact("edgar", "Revenue", 100.0)];
        let result = reconcile(&facts, 0.05, now());
        assert_eq!(result.canonical.len(), 1);
        assert_eq!(result.canonical[0].value, 100.0);
        assert!(result.discrepancies.is_empty());
    }

    #[test]
    fn agreeing_sources_produce_no_discrepancy() {
        let facts = vec![fact("edgar", "Revenue", 100.0), fact("fmp", "Revenue", 102.0)];
        let result = reconcile(&facts, 0.05, now());
        // canonical is edgar's value
        assert_eq!(result.canonical.len(), 1);
        assert_eq!(result.canonical[0].source, "edgar");
        assert_eq!(result.canonical[0].value, 100.0);
        assert!(result.discrepancies.is_empty());
    }

    #[test]
    fn disagreeing_sources_flag_discrepancy_against_edgar() {
        let facts = vec![fact("edgar", "Revenue", 100.0), fact("fmp", "Revenue", 130.0)];
        let result = reconcile(&facts, 0.05, now());
        assert_eq!(result.canonical[0].value, 100.0);
        assert_eq!(result.discrepancies.len(), 1);
        let d = &result.discrepancies[0];
        assert_eq!(d.field, "Revenue");
        assert_eq!(d.source_a, "edgar");
        assert_eq!(d.value_a, 100.0);
        assert_eq!(d.source_b, "fmp");
        assert_eq!(d.value_b, 130.0);
        assert!((d.pct_diff - 30.0 / 130.0).abs() < 1e-9);
        assert_eq!(d.period, Some("2023-12-31".to_string()));
        assert_eq!(d.flagged_at, now());
    }

    #[test]
    fn without_edgar_lowest_source_name_is_canonical() {
        let facts = vec![fact("scrape", "Revenue", 90.0), fact("fmp", "Revenue", 90.0)];
        let result = reconcile(&facts, 0.05, now());
        // "fmp" < "scrape" lexicographically
        assert_eq!(result.canonical[0].source, "fmp");
        assert!(result.discrepancies.is_empty());
    }

    #[test]
    fn both_zero_values_do_not_flag() {
        let facts = vec![fact("edgar", "Revenue", 0.0), fact("fmp", "Revenue", 0.0)];
        let result = reconcile(&facts, 0.05, now());
        assert!(result.discrepancies.is_empty());
        assert_eq!(result.canonical[0].value, 0.0);
    }

    #[test]
    fn canonical_zero_other_nonzero_flags_full_difference() {
        let facts = vec![fact("edgar", "Revenue", 0.0), fact("fmp", "Revenue", 50.0)];
        let result = reconcile(&facts, 0.05, now());
        assert_eq!(result.discrepancies.len(), 1);
        assert!((result.discrepancies[0].pct_diff - 1.0).abs() < 1e-9);
    }

    #[test]
    fn distinct_periods_and_items_are_grouped_independently() {
        let facts = vec![
            fact("edgar", "Revenue", 100.0),
            fact("fmp", "Revenue", 100.0),
            fact("edgar", "NetIncome", 20.0),
            fact("fmp", "NetIncome", 40.0),
        ];
        let result = reconcile(&facts, 0.05, now());
        assert_eq!(result.canonical.len(), 2);
        assert_eq!(result.discrepancies.len(), 1);
        assert_eq!(result.discrepancies[0].field, "NetIncome");
    }

    #[test]
    fn result_derives_are_exercised() {
        let result = reconcile(&[fact("edgar", "Revenue", 1.0)], 0.05, now());
        assert_eq!(result.clone(), result);
        assert!(format!("{result:?}").contains("Revenue"));
    }
}
