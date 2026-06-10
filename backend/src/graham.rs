//! Benjamin Graham defensive-investor analysis. Pure, no I/O.
//!
//! Criteria follow *The Intelligent Investor*, adapted to the ~10–15y of annual
//! data EDGAR XBRL provides (windows degrade to available history; each
//! criterion reports the span it evaluated, and missing inputs fail as
//! "insufficient data" rather than silently passing).

use std::collections::BTreeMap;

use chrono::Datelike;
use serde::Serialize;

use crate::domain::{FinancialFact, PeriodType};

pub const PE_MAX: f64 = 15.0;
pub const PB_MAX: f64 = 1.5;
pub const PE_PB_MAX: f64 = 22.5;
pub const CURRENT_RATIO_MIN: f64 = 2.0;
pub const EPS_GROWTH_MIN: f64 = 0.33;
pub const NET_NET_FRACTION: f64 = 2.0 / 3.0;
/// Default "adequate size" revenue floor (modernized from Graham's ~$100M).
pub const DEFAULT_MIN_REVENUE: f64 = 500_000_000.0;
const MIN_STABILITY_YEARS: usize = 3;

/// Graham Number = √(22.5 · EPS · BVPS), when both are positive.
pub fn graham_number(eps: f64, bvps: f64) -> Option<f64> {
    if eps > 0.0 && bvps > 0.0 {
        let v = (PE_PB_MAX * eps * bvps).sqrt();
        v.is_finite().then_some(v)
    } else {
        None
    }
}

/// Net current asset value per share = (current assets − total liabilities) / shares.
pub fn ncav_per_share(current_assets: f64, total_liabilities: f64, shares: f64) -> Option<f64> {
    if shares > 0.0 {
        Some((current_assets - total_liabilities) / shares)
    } else {
        None
    }
}

/// One pass/fail check with a human-readable detail.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Criterion {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// Result of a Graham defensive-investor assessment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GrahamAssessment {
    pub criteria: Vec<Criterion>,
    pub score: u32,
    pub graham_number: Option<f64>,
    pub ncav_per_share: Option<f64>,
    pub margin_of_safety: Option<f64>,
    pub net_net: bool,
    pub passes_defensive: bool,
}

fn avg(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        None
    } else {
        Some(xs.iter().sum::<f64>() / xs.len() as f64)
    }
}

/// Run the defensive-investor checks for one company.
pub fn assess(facts: &[FinancialFact], latest_price: Option<f64>, min_revenue: f64) -> GrahamAssessment {
    // Annual facts grouped by fiscal year: year -> { line_item -> value }.
    let mut by_year: BTreeMap<i32, BTreeMap<&str, f64>> = BTreeMap::new();
    for f in facts {
        if f.period_type == PeriodType::Annual {
            by_year
                .entry(f.period_end.year())
                .or_default()
                .insert(f.line_item.as_str(), f.value);
        }
    }
    let years: Vec<i32> = by_year.keys().copied().collect();
    let latest = years.last().and_then(|y| by_year.get(y));
    let get = |key: &str| -> Option<f64> { latest.and_then(|m| m.get(key).copied()) };
    let series = |key: &str| -> Vec<f64> {
        years.iter().filter_map(|y| by_year[y].get(key).copied()).collect()
    };

    let mut criteria = Vec::new();
    let mut add = |name: &str, passed: bool, detail: String| {
        criteria.push(Criterion { name: name.to_string(), passed, detail });
    };

    // 1. Adequate size.
    match get("Revenue") {
        Some(rev) => add(
            "Adequate size",
            rev >= min_revenue,
            format!("revenue {rev:.0} vs min {min_revenue:.0}"),
        ),
        None => add("Adequate size", false, "insufficient data".into()),
    }

    // 2. Current ratio >= 2.
    match (get("CurrentAssets"), get("CurrentLiabilities")) {
        (Some(ca), Some(cl)) if cl != 0.0 => {
            let r = ca / cl;
            add("Current ratio >= 2", r >= CURRENT_RATIO_MIN, format!("current ratio {r:.2}"))
        }
        _ => add("Current ratio >= 2", false, "insufficient data".into()),
    }

    // 3. Long-term debt <= working capital.
    match (get("CurrentAssets"), get("CurrentLiabilities"), get("LongTermDebt")) {
        (Some(ca), Some(cl), Some(ltd)) => {
            let wc = ca - cl;
            add(
                "Debt <= working capital",
                ltd <= wc,
                format!("long-term debt {ltd:.0} vs working capital {wc:.0}"),
            )
        }
        _ => add("Debt <= working capital", false, "insufficient data".into()),
    }

    // 4. Earnings stability: positive net income across available years.
    let net = series("NetIncome");
    if net.len() >= MIN_STABILITY_YEARS {
        let positive = net.iter().filter(|&&n| n > 0.0).count();
        add(
            "Earnings stability",
            positive == net.len(),
            format!("positive in {positive}/{} years", net.len()),
        );
    } else {
        add("Earnings stability", false, "insufficient data".into());
    }

    // 5. EPS growth >= 1/3 over the window (3-yr-average endpoints).
    let eps = series("Eps");
    let eps_growth = if eps.len() >= 2 {
        let head = avg(&eps[..eps.len().min(3)]);
        let tail = avg(&eps[eps.len().saturating_sub(3)..]);
        match (head, tail) {
            (Some(h), Some(t)) if h > 0.0 => Some((t - h) / h),
            _ => None,
        }
    } else {
        None
    };
    match eps_growth {
        Some(g) => add("EPS growth >= 33%", g >= EPS_GROWTH_MIN, format!("EPS growth {:.0}%", g * 100.0)),
        None => add("EPS growth >= 33%", false, "insufficient data".into()),
    }

    // Recent average EPS (last up to 3 years) for P/E.
    let recent_eps = if eps.is_empty() {
        None
    } else {
        avg(&eps[eps.len().saturating_sub(3)..])
    };
    let bvps = match (get("StockholdersEquity"), get("SharesOutstanding")) {
        (Some(e), Some(s)) if s != 0.0 => Some(e / s),
        _ => None,
    };

    // 6. P/E <= 15 (price / 3-yr avg EPS).
    let pe = match (latest_price, recent_eps) {
        (Some(p), Some(e)) if e > 0.0 => Some(p / e),
        _ => None,
    };
    match pe {
        Some(v) => add("P/E <= 15", v <= PE_MAX, format!("P/E {v:.1}")),
        None => add("P/E <= 15", false, "insufficient data".into()),
    }

    // 7. P/B <= 1.5, or P/E·P/B <= 22.5.
    let pb = match (latest_price, bvps) {
        (Some(p), Some(b)) if b > 0.0 => Some(p / b),
        _ => None,
    };
    match (pe, pb) {
        (Some(pe), Some(pb)) => add(
            "P/B <= 1.5 or P/E*P/B <= 22.5",
            pb <= PB_MAX || pe * pb <= PE_PB_MAX,
            format!("P/B {pb:.2}, P/E*P/B {:.1}", pe * pb),
        ),
        _ => add("P/B <= 1.5 or P/E*P/B <= 22.5", false, "insufficient data".into()),
    }

    // 8. Dividend record: paid every available year.
    let divs = series("DividendPerShare");
    if !divs.is_empty() {
        let paid = divs.iter().filter(|&&d| d > 0.0).count();
        add(
            "Dividend record",
            paid == divs.len(),
            format!("dividends in {paid}/{} years", divs.len()),
        );
    } else {
        add("Dividend record", false, "insufficient data".into());
    }

    // Valuation summary.
    let graham_number = match (eps.last().copied(), bvps) {
        (Some(e), Some(b)) => graham_number(e, b),
        _ => None,
    };
    let ncav = match (get("CurrentAssets"), get("TotalLiabilities"), get("SharesOutstanding")) {
        (Some(ca), Some(tl), Some(s)) => ncav_per_share(ca, tl, s),
        _ => None,
    };
    let margin_of_safety = match (graham_number, latest_price) {
        (Some(gn), Some(p)) if p > 0.0 => Some(gn / p - 1.0),
        _ => None,
    };
    let net_net = match (latest_price, ncav) {
        (Some(p), Some(n)) => n > 0.0 && p < NET_NET_FRACTION * n,
        _ => false,
    };

    let score = criteria.iter().filter(|c| c.passed).count() as u32;
    let passes_defensive = score as usize == criteria.len();

    GrahamAssessment {
        criteria,
        score,
        graham_number,
        ncav_per_share: ncav,
        margin_of_safety,
        net_net,
        passes_defensive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::StatementKind;
    use crate::testutil::fixed_now;
    use chrono::NaiveDate;

    fn fact(item: &str, year: i32, value: f64) -> FinancialFact {
        FinancialFact {
            company_id: 1,
            statement: StatementKind::Income,
            line_item: item.to_string(),
            period_type: PeriodType::Annual,
            period_end: NaiveDate::from_ymd_opt(year, 12, 31).unwrap(),
            value,
            source: "edgar".into(),
            fetched_at: fixed_now(),
        }
    }

    fn passed(a: &GrahamAssessment, name: &str) -> bool {
        a.criteria.iter().find(|c| c.name == name).unwrap().passed
    }

    /// A company that satisfies every defensive criterion.
    fn strong_company() -> Vec<FinancialFact> {
        let mut f = Vec::new();
        for (i, year) in (2018..=2023).enumerate() {
            f.push(fact("Revenue", year, 1_000_000_000.0));
            f.push(fact("NetIncome", year, 100_000_000.0));
            f.push(fact("Eps", year, 1.0 + i as f64 * 0.2)); // 1.0 -> 2.0, strong growth
            f.push(fact("DividendPerShare", year, 0.5));
        }
        // latest-year balance sheet
        f.push(fact("CurrentAssets", 2023, 400.0));
        f.push(fact("CurrentLiabilities", 2023, 100.0));
        f.push(fact("LongTermDebt", 2023, 50.0));
        f.push(fact("StockholdersEquity", 2023, 1000.0));
        f.push(fact("SharesOutstanding", 2023, 100.0)); // BVPS = 10
        f.push(fact("TotalLiabilities", 2023, 150.0));
        f
    }

    #[test]
    fn graham_number_and_ncav_guard_signs() {
        assert!(graham_number(2.0, 10.0).unwrap() > 0.0);
        assert_eq!(graham_number(-1.0, 10.0), None);
        assert_eq!(graham_number(2.0, 0.0), None);
        // astronomically large inputs → overflow to infinity → None, not Some(inf)
        assert_eq!(graham_number(f64::MAX, f64::MAX), None);
        assert_eq!(ncav_per_share(100.0, 40.0, 10.0), Some(6.0));
        assert_eq!(ncav_per_share(100.0, 40.0, 0.0), None);
    }

    #[test]
    fn strong_company_passes_all_defensive_criteria() {
        // price low enough for P/E and P/B to pass: EPS recent avg ~1.8, BVPS 10.
        let a = assess(&strong_company(), Some(12.0), 500_000_000.0);
        assert!(a.passes_defensive, "criteria: {:?}", a.criteria);
        assert_eq!(a.score as usize, a.criteria.len());
        assert!(a.graham_number.is_some());
        assert!(a.margin_of_safety.is_some());
        // serializes for the API
        assert!(serde_json::to_string(&a).unwrap().contains("\"criteria\""));
    }

    #[test]
    fn flags_individual_failures() {
        // Tiny size, weak balance sheet, losses, no dividends, sky-high price.
        let mut f = vec![
            fact("Revenue", 2021, 1_000.0),
            fact("NetIncome", 2021, -5.0),
            fact("NetIncome", 2022, 10.0),
            fact("NetIncome", 2023, 20.0),
            fact("Eps", 2021, 2.0),
            fact("Eps", 2023, 1.0), // shrinking
        ];
        f.push(fact("CurrentAssets", 2023, 100.0));
        f.push(fact("CurrentLiabilities", 2023, 100.0)); // ratio 1.0
        f.push(fact("LongTermDebt", 2023, 500.0)); // > working capital (0)
        f.push(fact("StockholdersEquity", 2023, 10.0));
        f.push(fact("SharesOutstanding", 2023, 10.0)); // BVPS 1
        let a = assess(&f, Some(100.0), 500_000_000.0);
        assert!(!passed(&a, "Adequate size"));
        assert!(!passed(&a, "Current ratio >= 2"));
        assert!(!passed(&a, "Debt <= working capital"));
        assert!(!passed(&a, "Earnings stability"));
        assert!(!passed(&a, "EPS growth >= 33%"));
        assert!(!passed(&a, "P/E <= 15"));
        assert!(!passed(&a, "Dividend record"));
        assert!(!a.passes_defensive);
    }

    #[test]
    fn missing_data_is_insufficient_not_passing() {
        let a = assess(&[], None, 500_000_000.0);
        assert_eq!(a.score, 0);
        assert!(a.criteria.iter().all(|c| !c.passed && c.detail == "insufficient data"));
        assert!(a.graham_number.is_none());
        assert!(!a.net_net);
    }

    #[test]
    fn detects_net_net_bargain() {
        let mut f = strong_company();
        // huge current assets, tiny liabilities, few shares -> high NCAV/share
        f.push(fact("CurrentAssets", 2023, 10_000.0));
        f.retain(|x| x.line_item != "TotalLiabilities");
        f.push(fact("TotalLiabilities", 2023, 100.0));
        // NCAV/share = (10000-100)/100 = 99; price 12 < 2/3*99
        let a = assess(&f, Some(12.0), 500_000_000.0);
        assert!(a.net_net);
    }
}
