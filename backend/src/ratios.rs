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

        // Efficiency / returns.
        let total_assets = items.get("TotalAssets");
        add("roa", ratio(net_income, total_assets));
        add(
            "roce",
            match (items.get("OperatingIncome"), total_assets, items.get("CurrentLiabilities")) {
                (Some(&oi), Some(&ta), Some(&cl)) if ta - cl != 0.0 => Some(oi / (ta - cl)),
                _ => None,
            },
        );
        add("asset_turnover", ratio(revenue, total_assets));

        // EBITDA = OperatingIncome + DepreciationAmortization (both required).
        let ebitda = match (items.get("OperatingIncome"), items.get("DepreciationAmortization")) {
            (Some(&oi), Some(&da)) => Some(oi + da),
            _ => None,
        };
        add("ebitda", ebitda);

        // Liquidity.
        let current_liabilities = items.get("CurrentLiabilities");
        add(
            "quick_ratio",
            match (items.get("CurrentAssets"), current_liabilities) {
                (Some(&ca), Some(&cl)) if cl != 0.0 => {
                    let inv = items.get("Inventories").copied().unwrap_or(0.0);
                    Some((ca - inv) / cl)
                }
                _ => None,
            },
        );
        add("cash_ratio", ratio(items.get("CashAndEquivalents"), current_liabilities));

        // Leverage.
        add("debt_to_assets", ratio(items.get("TotalLiabilities"), total_assets));
        add("interest_coverage", ratio(items.get("OperatingIncome"), items.get("InterestExpense")));

        // Net debt = (short + long debt) − cash; needs at least one debt leg.
        let net_debt = match (items.get("ShortTermDebt"), items.get("LongTermDebt")) {
            (None, None) => None,
            (st, lt) => {
                let debt = st.copied().unwrap_or(0.0) + lt.copied().unwrap_or(0.0);
                let cash = items.get("CashAndEquivalents").copied().unwrap_or(0.0);
                Some(debt - cash)
            }
        };
        add("net_debt", net_debt);
        add(
            "debt_to_ebitda",
            match (items.get("ShortTermDebt"), items.get("LongTermDebt"), ebitda) {
                (st, lt, Some(e)) if (st.is_some() || lt.is_some()) && e != 0.0 => {
                    let debt = st.copied().unwrap_or(0.0) + lt.copied().unwrap_or(0.0);
                    Some(debt / e)
                }
                _ => None,
            },
        );

        // Price-based valuation: price × shares ÷ {Revenue, FCF}; dividend yield.
        let market_cap = match (price, share_count(&items)) {
            (Some(p), Some(s)) => Some(p * s),
            _ => None,
        };
        add(
            "price_to_sales",
            match (market_cap, revenue) {
                (Some(mc), Some(&r)) if r != 0.0 => Some(mc / r),
                _ => None,
            },
        );
        add(
            "price_to_fcf",
            match (market_cap, fcf) {
                (Some(mc), Some(f)) if f > 0.0 => Some(mc / f),
                _ => None,
            },
        );
        add(
            "dividend_yield",
            match (items.get("DividendPerShare"), price) {
                (Some(&dps), Some(p)) if p != 0.0 => Some(dps / p),
                _ => None,
            },
        );
    }

    // Second pass: annual-only YoY growth + window CAGR for Revenue and Eps.
    // Build the (metric, period_end, value) tuples, then materialize as Ratios.
    let mut growth: Vec<(String, NaiveDate, f64)> = Vec::new();
    for (line_item, prefix) in [("Revenue", "revenue"), ("Eps", "eps")] {
        // Sorted annual series for this line item: period_end -> value.
        let mut series: BTreeMap<NaiveDate, f64> = BTreeMap::new();
        for f in facts {
            if f.period_type == PeriodType::Annual && f.line_item == line_item {
                series.insert(f.period_end, f.value);
            }
        }
        let points: Vec<(NaiveDate, f64)> = series.into_iter().collect();
        // YoY growth on consecutive periods.
        for w in points.windows(2) {
            let (_, prev) = w[0];
            let (end, cur) = w[1];
            if prev != 0.0 {
                growth.push((format!("{prefix}_growth"), end, (cur - prev) / prev.abs()));
            }
        }
        // CAGR across the available window, at the latest period only.
        if points.len() >= 2 {
            let (_, first) = points[0];
            let (last_end, last) = points[points.len() - 1];
            let years = (points.len() - 1) as f64;
            if first > 0.0 {
                growth.push((format!("{prefix}_cagr"), last_end, (last / first).powf(1.0 / years) - 1.0));
            }
        }
    }
    for (metric, period_end, value) in growth {
        ratios.push(Ratio {
            company_id,
            period_end,
            period_type: PeriodType::Annual,
            metric,
            value,
            computed_at: now,
        });
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
    fn computes_efficiency_and_return_ratios() {
        let p = (2023, 12, 31);
        let facts = vec![
            fact("Revenue", p, 100.0),
            fact("NetIncome", p, 20.0),
            fact("OperatingIncome", p, 30.0),
            fact("TotalAssets", p, 200.0),
            fact("CurrentLiabilities", p, 50.0),
        ];
        let r = compute(1, &facts, &[], fixed_now());
        assert_eq!(metric(&r, "roa").unwrap().value, 0.1); // 20 / 200
        assert_eq!(metric(&r, "roce").unwrap().value, 0.2); // 30 / (200 - 50)
        assert_eq!(metric(&r, "asset_turnover").unwrap().value, 0.5); // 100 / 200
    }

    #[test]
    fn computes_ebitda_and_debt_to_ebitda_and_net_debt() {
        let p = (2023, 12, 31);
        let facts = vec![
            fact("OperatingIncome", p, 30.0),
            fact("DepreciationAmortization", p, 10.0),
            fact("ShortTermDebt", p, 15.0),
            fact("LongTermDebt", p, 25.0),
            fact("CashAndEquivalents", p, 5.0),
        ];
        let r = compute(1, &facts, &[], fixed_now());
        assert_eq!(metric(&r, "ebitda").unwrap().value, 40.0); // 30 + 10
        assert_eq!(metric(&r, "net_debt").unwrap().value, 35.0); // (15+25) - 5
        assert_eq!(metric(&r, "debt_to_ebitda").unwrap().value, 1.0); // 40 / 40
    }

    #[test]
    fn net_debt_treats_missing_debt_leg_as_zero_but_needs_one_leg() {
        let p = (2023, 12, 31);
        // only long-term debt present + cash → still emitted
        let facts = vec![fact("LongTermDebt", p, 30.0), fact("CashAndEquivalents", p, 10.0)];
        let r = compute(1, &facts, &[], fixed_now());
        assert_eq!(metric(&r, "net_debt").unwrap().value, 20.0); // (0+30) - 10
        // no debt legs at all → not emitted even with cash
        let facts = vec![fact("CashAndEquivalents", p, 10.0)];
        let r = compute(1, &facts, &[], fixed_now());
        assert!(metric(&r, "net_debt").is_none());
    }

    #[test]
    fn ebitda_requires_both_inputs() {
        let p = (2023, 12, 31);
        let r = compute(1, &[fact("OperatingIncome", p, 30.0)], &[], fixed_now());
        assert!(metric(&r, "ebitda").is_none());
        let r = compute(1, &[fact("DepreciationAmortization", p, 10.0)], &[], fixed_now());
        assert!(metric(&r, "ebitda").is_none());
    }

    #[test]
    fn computes_liquidity_and_leverage_ratios() {
        let p = (2023, 12, 31);
        let facts = vec![
            fact("CurrentAssets", p, 100.0),
            fact("Inventories", p, 40.0),
            fact("CurrentLiabilities", p, 50.0),
            fact("CashAndEquivalents", p, 25.0),
            fact("TotalLiabilities", p, 120.0),
            fact("TotalAssets", p, 200.0),
            fact("OperatingIncome", p, 30.0),
            fact("InterestExpense", p, 6.0),
        ];
        let r = compute(1, &facts, &[], fixed_now());
        assert_eq!(metric(&r, "quick_ratio").unwrap().value, 1.2); // (100-40)/50
        assert_eq!(metric(&r, "cash_ratio").unwrap().value, 0.5); // 25/50
        assert_eq!(metric(&r, "debt_to_assets").unwrap().value, 0.6); // 120/200
        assert_eq!(metric(&r, "interest_coverage").unwrap().value, 5.0); // 30/6
    }

    #[test]
    fn quick_ratio_treats_absent_inventory_as_zero() {
        let p = (2023, 12, 31);
        let facts = vec![fact("CurrentAssets", p, 100.0), fact("CurrentLiabilities", p, 50.0)];
        let r = compute(1, &facts, &[], fixed_now());
        assert_eq!(metric(&r, "quick_ratio").unwrap().value, 2.0); // (100-0)/50
    }

    #[test]
    fn computes_price_based_ratios_and_dividend_yield() {
        let p = (2023, 12, 31);
        let facts = vec![
            fact("Revenue", p, 100.0),
            fact("SharesOutstanding", p, 10.0),
            fact("OperatingCashFlow", p, 30.0),
            fact("CapEx", p, 10.0), // FCF = 20
            fact("DividendPerShare", p, 2.0),
        ];
        let price = PricePoint {
            company_id: 1,
            date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            open: None,
            high: None,
            low: None,
            close: 50.0,
            volume: None,
            source: "fmp".into(),
        };
        let r = compute(1, &facts, &[price], fixed_now());
        // price*shares = 500; /Revenue 100 = 5
        assert_eq!(metric(&r, "price_to_sales").unwrap().value, 5.0);
        // 500 / FCF 20 = 25
        assert_eq!(metric(&r, "price_to_fcf").unwrap().value, 25.0);
        // DPS 2 / price 50 = 0.04
        assert_eq!(metric(&r, "dividend_yield").unwrap().value, 0.04);
    }

    #[test]
    fn price_to_fcf_skipped_when_fcf_not_positive() {
        let p = (2023, 12, 31);
        let facts = vec![
            fact("SharesOutstanding", p, 10.0),
            fact("OperatingCashFlow", p, 10.0),
            fact("CapEx", p, 30.0), // FCF = -20
        ];
        let price = PricePoint {
            company_id: 1,
            date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            open: None,
            high: None,
            low: None,
            close: 50.0,
            volume: None,
            source: "fmp".into(),
        };
        let r = compute(1, &facts, &[price], fixed_now());
        assert!(metric(&r, "price_to_fcf").is_none());
    }

    #[test]
    fn computes_annual_growth_and_cagr() {
        let facts = vec![
            fact("Revenue", (2021, 12, 31), 100.0),
            fact("Eps", (2021, 12, 31), 4.0),
            fact("Revenue", (2022, 12, 31), 120.0),
            fact("Eps", (2022, 12, 31), 5.0),
            fact("Revenue", (2023, 12, 31), 150.0),
            fact("Eps", (2023, 12, 31), 6.0),
        ];
        let r = compute(1, &facts, &[], fixed_now());
        let g = |metric_name: &str, year: (i32, u32, u32)| {
            let pe = NaiveDate::from_ymd_opt(year.0, year.1, year.2).unwrap();
            r.iter()
                .find(|x| x.metric == metric_name && x.period_end == pe)
                .map(|x| x.value)
        };
        // 2022 revenue growth (120-100)/100 = 0.2
        assert!((g("revenue_growth", (2022, 12, 31)).unwrap() - 0.2).abs() < 1e-9);
        // 2023 revenue growth (150-120)/120 = 0.25
        assert!((g("revenue_growth", (2023, 12, 31)).unwrap() - 0.25).abs() < 1e-9);
        // eps growth 2022 (5-4)/4 = 0.25
        assert!((g("eps_growth", (2022, 12, 31)).unwrap() - 0.25).abs() < 1e-9);
        // no growth for the first year
        assert!(g("revenue_growth", (2021, 12, 31)).is_none());
        // CAGR only at latest period: (150/100)^(1/2) - 1
        let expected_rev_cagr = (150.0_f64 / 100.0).powf(1.0 / 2.0) - 1.0;
        assert!((g("revenue_cagr", (2023, 12, 31)).unwrap() - expected_rev_cagr).abs() < 1e-9);
        let expected_eps_cagr = (6.0_f64 / 4.0).powf(1.0 / 2.0) - 1.0;
        assert!((g("eps_cagr", (2023, 12, 31)).unwrap() - expected_eps_cagr).abs() < 1e-9);
        // CAGR not on earlier periods
        assert!(g("revenue_cagr", (2022, 12, 31)).is_none());
    }

    #[test]
    fn growth_uses_abs_prior_and_skips_quarterly_and_nonpositive_first() {
        // quarterly series must not emit growth
        let mut q = vec![
            fact("Revenue", (2022, 12, 31), 100.0),
            fact("Revenue", (2023, 12, 31), 120.0),
            fact("NetIncome", (2023, 12, 31), 24.0),
        ];
        for f in &mut q {
            f.period_type = PeriodType::Quarterly;
        }
        let r = compute(1, &q, &[], fixed_now());
        // a quarterly net_margin is produced, but never revenue_growth
        assert!(r.iter().any(|x| x.metric == "net_margin"));
        assert!(r.iter().all(|x| x.metric != "revenue_growth"));
        // negative prior → uses |prior|: (10 - (-5)) / 5 = 3.0
        let facts = vec![
            fact("Eps", (2022, 12, 31), -5.0),
            fact("Eps", (2023, 12, 31), 10.0),
        ];
        let r = compute(1, &facts, &[], fixed_now());
        let g = r
            .iter()
            .find(|x| {
                x.metric == "eps_growth"
                    && x.period_end == NaiveDate::from_ymd_opt(2023, 12, 31).unwrap()
            })
            .map(|x| x.value)
            .unwrap();
        assert!((g - 3.0).abs() < 1e-9);
        // CAGR skipped when first value is not > 0
        assert!(r.iter().all(|x| x.metric != "eps_cagr"));
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
