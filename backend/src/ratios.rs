//! Derive financial ratios from stored facts. Pure, no I/O.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};

use crate::domain::{share_count, FinancialFact, PeriodType, PricePoint, Ratio};

/// Compute per-period ratios from a company's facts (and `prices`, for the
/// historical P/E and P/B series). Only ratios whose inputs are present (and
/// denominators non-zero) for a period are emitted.
pub fn compute(
    company_id: i64,
    facts: &[FinancialFact],
    prices: &[PricePoint],
    now: DateTime<Utc>,
) -> Vec<Ratio> {
    // Close on/just before a date — the price to value that period at.
    let price_at = |d: NaiveDate| -> Option<f64> {
        prices.iter().filter(|p| p.date <= d).max_by_key(|p| p.date).map(|p| p.close)
    };
    // (period_end, period_type) -> { line_item -> value }. Keying on period_type
    // too keeps annual and Q4 (same end date) from colliding.
    let mut by_period: BTreeMap<(NaiveDate, PeriodType), BTreeMap<&str, f64>> = BTreeMap::new();
    for f in facts {
        by_period
            .entry((f.period_end, f.period_type))
            .or_default()
            .insert(f.line_item.as_str(), f.value);
    }

    let mut ratios = Vec::new();
    for ((period_end, period_type), items) in by_period {
        // num/den, guarding a zero denominator.
        let ratio = |num: Option<&f64>, den: Option<&f64>| -> Option<f64> {
            match (num, den) {
                (Some(&n), Some(&d)) if d != 0.0 => Some(n / d),
                _ => None,
            }
        };
        let mut add = |metric: &str, value: Option<f64>| {
            if let Some(v) = value {
                ratios.push(Ratio {
                    company_id,
                    period_end,
                    period_type,
                    metric: metric.to_string(),
                    value: v,
                    computed_at: now,
                });
            }
        };

        let net_income = items.get("NetIncome");
        let revenue = items.get("Revenue");
        add("net_margin", ratio(net_income, revenue));
        add("roe", ratio(net_income, items.get("StockholdersEquity")));
        add(
            "debt_to_equity",
            ratio(items.get("TotalLiabilities"), items.get("StockholdersEquity")),
        );
        add("gross_margin", ratio(items.get("GrossProfit"), revenue));
        add("operating_margin", ratio(items.get("OperatingIncome"), revenue));
        add(
            "current_ratio",
            ratio(items.get("CurrentAssets"), items.get("CurrentLiabilities")),
        );
        let bvps = ratio(items.get("StockholdersEquity"), share_count(&items).as_ref());
        add("book_value_per_share", bvps);
        add("payout_ratio", ratio(items.get("DividendPerShare"), items.get("Eps")));
        // Historical valuation: price at the period end ÷ EPS / BVPS.
        let price = price_at(period_end);
        add(
            "pe",
            match (price, items.get("Eps")) {
                (Some(p), Some(&e)) if e > 0.0 => Some(p / e),
                _ => None,
            },
        );
        add(
            "pb",
            match (price, bvps) {
                (Some(p), Some(b)) if b > 0.0 => Some(p / b),
                _ => None,
            },
        );
        // Working capital is a difference, not a ratio.
        let working_capital = match (items.get("CurrentAssets"), items.get("CurrentLiabilities")) {
            (Some(&ca), Some(&cl)) => Some(ca - cl),
            _ => None,
        };
        add("working_capital", working_capital);
        // Free cash flow = operating cash flow − capital expenditure.
        let fcf = match (items.get("OperatingCashFlow"), items.get("CapEx")) {
            (Some(&ocf), Some(&capex)) => Some(ocf - capex),
            _ => None,
        };
        add("free_cash_flow", fcf);
        add(
            "fcf_margin",
            match (fcf, revenue) {
                (Some(f), Some(&r)) if r != 0.0 => Some(f / r),
                _ => None,
            },
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
        let r = compute(1, &facts, &[], fixed_now());
        assert_eq!(metric(&r, "net_margin").unwrap().value, 0.25);
        assert_eq!(metric(&r, "roe").unwrap().value, 0.625);
        assert_eq!(metric(&r, "debt_to_equity").unwrap().value, 1.5);
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn computes_graham_inputs() {
        let p = (2023, 12, 31);
        let facts = vec![
            fact("Revenue", p, 100.0),
            fact("GrossProfit", p, 40.0),
            fact("OperatingIncome", p, 30.0),
            fact("CurrentAssets", p, 60.0),
            fact("CurrentLiabilities", p, 24.0),
            fact("StockholdersEquity", p, 80.0),
            fact("SharesOutstanding", p, 8.0),
        ];
        let r = compute(1, &facts, &[], fixed_now());
        assert_eq!(metric(&r, "gross_margin").unwrap().value, 0.4);
        assert_eq!(metric(&r, "operating_margin").unwrap().value, 0.3);
        assert_eq!(metric(&r, "current_ratio").unwrap().value, 2.5);
        assert_eq!(metric(&r, "working_capital").unwrap().value, 36.0);
        assert_eq!(metric(&r, "book_value_per_share").unwrap().value, 10.0);
    }

    #[test]
    fn computes_cash_flow_and_payout_metrics() {
        let p = (2023, 12, 31);
        let facts = vec![
            fact("Revenue", p, 100.0),
            fact("OperatingCashFlow", p, 30.0),
            fact("CapEx", p, 12.0),
            fact("DividendPerShare", p, 2.0),
            fact("Eps", p, 8.0),
        ];
        let r = compute(1, &facts, &[], fixed_now());
        assert_eq!(metric(&r, "free_cash_flow").unwrap().value, 18.0); // 30 - 12
        assert_eq!(metric(&r, "fcf_margin").unwrap().value, 0.18);
        assert_eq!(metric(&r, "payout_ratio").unwrap().value, 0.25); // 2 / 8
    }

    #[test]
    fn computes_pe_and_pb_from_price_at_period_end() {
        let p = (2023, 12, 31);
        let facts = vec![
            fact("Eps", p, 5.0),
            fact("StockholdersEquity", p, 100.0),
            fact("SharesOutstanding", p, 10.0), // BVPS = 10
        ];
        let price = |d: &str, c: f64| PricePoint {
            company_id: 1,
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            open: None,
            high: None,
            low: None,
            close: c,
            volume: None,
            source: "fmp".into(),
        };
        // latest close on/before 2023-12-31 is the 12-29 bar (150), not the 2024 one.
        let prices = vec![price("2023-12-29", 150.0), price("2024-01-05", 999.0)];
        let r = compute(1, &facts, &prices, fixed_now());
        assert_eq!(metric(&r, "pe").unwrap().value, 30.0); // 150 / 5
        assert_eq!(metric(&r, "pb").unwrap().value, 15.0); // 150 / 10
    }

    #[test]
    fn bvps_falls_back_to_balance_then_dei_shares() {
        let p = (2023, 12, 31);
        let from_balance = vec![
            fact("StockholdersEquity", p, 100.0),
            fact("SharesOutstandingBalance", p, 10.0),
        ];
        let r = compute(1, &from_balance, &[], fixed_now());
        assert_eq!(metric(&r, "book_value_per_share").unwrap().value, 10.0);

        let from_dei = vec![
            fact("StockholdersEquity", p, 100.0),
            fact("SharesOutstandingDei", p, 4.0),
        ];
        let r = compute(1, &from_dei, &[], fixed_now());
        assert_eq!(metric(&r, "book_value_per_share").unwrap().value, 25.0);

        // weighted income-statement figure wins over the balance-sheet one
        let both = vec![
            fact("StockholdersEquity", p, 100.0),
            fact("SharesOutstanding", p, 20.0),
            fact("SharesOutstandingBalance", p, 10.0),
        ];
        let r = compute(1, &both, &[], fixed_now());
        assert_eq!(metric(&r, "book_value_per_share").unwrap().value, 5.0);
    }

    #[test]
    fn skips_ratios_with_missing_inputs_or_zero_denominator() {
        let p = (2023, 12, 31);
        // Revenue is zero (no net_margin); no equity (no roe / debt_to_equity).
        let facts = vec![fact("Revenue", p, 0.0), fact("NetIncome", p, 10.0)];
        let r = compute(1, &facts, &[], fixed_now());
        assert!(r.is_empty());
    }

    #[test]
    fn annual_and_quarterly_same_date_do_not_collide() {
        let p = (2023, 12, 31);
        let mut annual = vec![fact("Revenue", p, 100.0), fact("NetIncome", p, 25.0)];
        let mut q4 = vec![fact("Revenue", p, 30.0), fact("NetIncome", p, 3.0)];
        for f in &mut q4 {
            f.period_type = PeriodType::Quarterly;
        }
        annual.extend(q4);
        let r = compute(1, &annual, &[], fixed_now());
        let nm: Vec<(PeriodType, f64)> = r
            .iter()
            .filter(|x| x.metric == "net_margin")
            .map(|x| (x.period_type, x.value))
            .collect();
        // both period types present and distinct (annual 0.25, quarterly 0.1)
        assert!(nm.contains(&(PeriodType::Annual, 0.25)));
        assert!(nm.contains(&(PeriodType::Quarterly, 0.1)));
    }

    #[test]
    fn groups_independently_by_period() {
        let facts = vec![
            fact("Revenue", (2023, 12, 31), 100.0),
            fact("NetIncome", (2023, 12, 31), 20.0),
            fact("Revenue", (2022, 12, 31), 80.0),
            fact("NetIncome", (2022, 12, 31), 8.0),
        ];
        let r = compute(1, &facts, &[], fixed_now());
        let margins: Vec<f64> = r
            .iter()
            .filter(|x| x.metric == "net_margin")
            .map(|x| x.value)
            .collect();
        assert_eq!(margins, vec![0.1, 0.2]); // 2022 then 2023 (BTreeMap order)
    }
}
