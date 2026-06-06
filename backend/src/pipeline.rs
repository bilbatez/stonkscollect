//! Ingest orchestration: reconcile multi-source facts and persist the
//! canonical values plus any flagged discrepancies.

use chrono::{DateTime, Utc};

use crate::domain::FinancialFact;
use crate::reconcile::reconcile;
use crate::store::{Store, StoreError};

/// Reconcile `facts` (from any mix of sources) and persist the canonical value
/// per period plus flagged discrepancies. Returns
/// `(facts_written, discrepancies_written)`.
pub async fn persist_facts(
    store: &Store,
    facts: &[FinancialFact],
    threshold: f64,
    now: DateTime<Utc>,
) -> Result<(usize, usize), StoreError> {
    let result = reconcile(facts, threshold, now);
    for fact in &result.canonical {
        store.upsert_fact(fact).await?;
    }
    for discrepancy in &result.discrepancies {
        store.insert_discrepancy(discrepancy).await?;
    }
    Ok((result.canonical.len(), result.discrepancies.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{NewCompany, PeriodType, StatementKind};
    use chrono::{NaiveDate, TimeZone};

    async fn store_with_company() -> (Store, i64, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join("t.db").display());
        let store = Store::connect(&url).await.unwrap();
        let id = store
            .insert_company(&NewCompany {
                cik: "1".into(),
                ticker: "AAPL".into(),
                name: "Apple".into(),
                exchange: None,
                sector: None,
                industry: None,
            })
            .await
            .unwrap();
        (store, id, dir)
    }

    fn fact(company_id: i64, source: &str, item: &str, value: f64) -> FinancialFact {
        FinancialFact {
            company_id,
            statement: StatementKind::Income,
            line_item: item.to_string(),
            period_type: PeriodType::Annual,
            period_end: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            value,
            source: source.to_string(),
            fetched_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    #[tokio::test]
    async fn persists_canonical_facts_and_flags_discrepancies() {
        let (store, id, _d) = store_with_company().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let facts = vec![
            fact(id, "edgar", "Revenue", 100.0),
            fact(id, "fmp", "Revenue", 130.0), // diverges -> discrepancy
            fact(id, "edgar", "NetIncome", 20.0),
            fact(id, "fmp", "NetIncome", 20.0), // agrees
        ];
        let (facts_written, discrepancies) = persist_facts(&store, &facts, 0.05, now).await.unwrap();
        assert_eq!(facts_written, 2);
        assert_eq!(discrepancies, 1);

        let stored = store.get_facts(id).await.unwrap();
        let revenue = stored.iter().find(|f| f.line_item == "Revenue").unwrap();
        assert_eq!(revenue.value, 100.0); // canonical = EDGAR
        assert_eq!(store.get_discrepancies(id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn surfaces_store_errors() {
        let (store, id, _d) = store_with_company().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        store.close().await;
        let err = persist_facts(&store, &[fact(id, "edgar", "Revenue", 1.0)], 0.05, now).await;
        assert!(err.is_err());
    }
}
