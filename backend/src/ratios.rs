//! Derive financial ratios from stored facts. Pure, no I/O.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};

use crate::domain::{FinancialFact, Ratio};

/// Compute per-period ratios from a company's facts. Only ratios whose inputs
/// are present (and denominators non-zero) for a period are emitted.
pub fn compute(company_id: i64, facts: &[FinancialFact], now: DateTime<Utc>) -> Vec<Ratio> {
    // period_end -> { line_item -> value }
    let mut by_period: BTreeMap<NaiveDate, BTreeMap<&str, f64>> = BTreeMap::new();
    for f in facts {
        by_period
            .entry(f.period_end)
            .or_default()
            .insert(f.line_item.as_str(), f.value);
    }

    let mut ratios = Vec::new();
    for (period_end, items) in by_period {
        let mut push = |metric: &str, num: Option<&f64>, den: Option<&f64>| {
            if let (Some(&n), Some(&d)) = (num, den) {
                if d != 0.0 {
                    ratios.push(Ratio {
                        company_id,
                        period_end,
                        metric: metric.to_string(),
                        value: n / d,
                        computed_at: now,
                    });
                }
            }
        };
        let net_income = items.get("NetIncome");
        push("net_margin", net_income, items.get("Revenue"));
        push("roe", net_income, items.get("StockholdersEquity"));
        push(
            "debt_to_equity",
            items.get("TotalLiabilities"),
            items.get("StockholdersEquity"),
        );
    }
    ratios
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PeriodType, StatementKind};
    use crate::testutil::fixed_now;

    fn fact(item: &str, period: (i32, u32, u32), value: f64) -> FinancialFact {
        FinancialFact {
            company_id: 1,
            statement: StatementKind::Income,
            line_item: item.to_string(),
            period_type: PeriodType::Annual,
            period_end: NaiveDate::from_ymd_opt(period.0, period.1, period.2).unwrap(),
            value,
            source: "edgar".into(),
            fetched_at: fixed_now(),
        }
    }

    fn metric<'a>(ratios: &'a [Ratio], name: &str) -> Option<&'a Ratio> {
        ratios.iter().find(|r| r.metric == name)
    }

    #[test]
    fn computes_margins_and_leverage_when_inputs_present() {
        let p = (2023, 12, 31);
        let facts = vec![
            fact("Revenue", p, 100.0),
            fact("NetIncome", p, 25.0),
            fact("TotalLiabilities", p, 60.0),
            fact("StockholdersEquity", p, 40.0),
        ];
        let r = compute(1, &facts, fixed_now());
        assert_eq!(metric(&r, "net_margin").unwrap().value, 0.25);
        assert_eq!(metric(&r, "roe").unwrap().value, 0.625);
        assert_eq!(metric(&r, "debt_to_equity").unwrap().value, 1.5);
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn skips_ratios_with_missing_inputs_or_zero_denominator() {
        let p = (2023, 12, 31);
        // Revenue is zero (no net_margin); no equity (no roe / debt_to_equity).
        let facts = vec![fact("Revenue", p, 0.0), fact("NetIncome", p, 10.0)];
        let r = compute(1, &facts, fixed_now());
        assert!(r.is_empty());
    }

    #[test]
    fn groups_independently_by_period() {
        let facts = vec![
            fact("Revenue", (2023, 12, 31), 100.0),
            fact("NetIncome", (2023, 12, 31), 20.0),
            fact("Revenue", (2022, 12, 31), 80.0),
            fact("NetIncome", (2022, 12, 31), 8.0),
        ];
        let r = compute(1, &facts, fixed_now());
        let margins: Vec<f64> = r
            .iter()
            .filter(|x| x.metric == "net_margin")
            .map(|x| x.value)
            .collect();
        assert_eq!(margins, vec![0.1, 0.2]); // 2022 then 2023 (BTreeMap order)
    }
}
