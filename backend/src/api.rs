//! REST API handlers. Thin layer over [`Store`]; resolves a ticker to a
//! company then returns its records as JSON.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::domain::{
    CollectionRun, Company, Discrepancy, FinancialFact, NewsItem, PricePoint, Ratio,
};
use crate::store::{Store, StoreError};

type ApiResult<T> = Result<Json<T>, (StatusCode, String)>;

/// Optional `?from=YYYY-MM-DD&to=YYYY-MM-DD&limit=N` range params.
#[derive(Debug, Default, Deserialize)]
struct RangeParams {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    limit: Option<i64>,
}

/// All API routes, parameterized over a shared [`Store`].
pub fn routes() -> Router<Arc<Store>> {
    Router::new()
        .route("/api/companies/:ticker", get(company))
        .route("/api/companies/:ticker/prices", get(prices))
        .route("/api/companies/:ticker/facts", get(facts))
        .route("/api/companies/:ticker/ratios", get(ratios))
        .route("/api/companies/:ticker/news", get(news))
        .route("/api/companies/:ticker/discrepancies", get(discrepancies))
        .route("/api/runs", get(runs))
}

fn internal(e: StoreError) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

async fn resolve(store: &Store, ticker: &str) -> Result<Company, (StatusCode, String)> {
    store
        .get_company(ticker)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, format!("unknown ticker: {ticker}")))
}

async fn company(State(store): State<Arc<Store>>, Path(ticker): Path<String>) -> ApiResult<Company> {
    Ok(Json(resolve(&store, &ticker).await?))
}

async fn prices(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    Query(rp): Query<RangeParams>,
) -> ApiResult<Vec<PricePoint>> {
    let c = resolve(&store, &ticker).await?;
    Ok(Json(
        store
            .get_prices_range(c.id, rp.from, rp.to, rp.limit)
            .await
            .map_err(internal)?,
    ))
}

async fn facts(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    Query(rp): Query<RangeParams>,
) -> ApiResult<Vec<FinancialFact>> {
    let c = resolve(&store, &ticker).await?;
    Ok(Json(
        store
            .get_facts_range(c.id, rp.from, rp.to, rp.limit)
            .await
            .map_err(internal)?,
    ))
}

async fn ratios(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
) -> ApiResult<Vec<Ratio>> {
    let c = resolve(&store, &ticker).await?;
    Ok(Json(store.get_ratios(c.id).await.map_err(internal)?))
}

async fn news(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
) -> ApiResult<Vec<NewsItem>> {
    let c = resolve(&store, &ticker).await?;
    Ok(Json(store.get_news(c.id).await.map_err(internal)?))
}

async fn discrepancies(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
) -> ApiResult<Vec<Discrepancy>> {
    let c = resolve(&store, &ticker).await?;
    Ok(Json(store.get_discrepancies(c.id).await.map_err(internal)?))
}

async fn runs(State(store): State<Arc<Store>>) -> ApiResult<Vec<CollectionRun>> {
    Ok(Json(store.recent_runs(50).await.map_err(internal)?))
}

#[cfg(test)]
mod tests {
    use crate::app;
    use crate::domain::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::{NaiveDate, TimeZone, Utc};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::store::Store;

    async fn seeded() -> (Arc<Store>, tempfile::TempDir) {
        let (store, dir) = crate::testutil::temp_store().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let id = store
            .insert_company(&NewCompany {
                cik: "0000320193".into(),
                ticker: "AAPL".into(),
                name: "Apple Inc.".into(),
                exchange: Some("NASDAQ".into()),
                sector: None,
                industry: None,
            })
            .await
            .unwrap();
        store
            .upsert_price(&PricePoint {
                company_id: id,
                date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                close: 185.0,
                volume: Some(1),
                source: "fmp".into(),
            })
            .await
            .unwrap();
        store
            .upsert_fact(&FinancialFact {
                company_id: id,
                statement: StatementKind::Income,
                line_item: "Revenue".into(),
                period_type: PeriodType::Annual,
                period_end: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                value: 1.0,
                source: "edgar".into(),
                fetched_at: now,
            })
            .await
            .unwrap();
        store
            .upsert_ratio(&Ratio {
                company_id: id,
                period_end: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                metric: "pe".into(),
                value: 28.5,
                computed_at: now,
            })
            .await
            .unwrap();
        store
            .insert_news(&NewsItem {
                company_id: id,
                title: "Hi".into(),
                description: None,
                url: "http://a".into(),
                source: "reuters".into(),
                published_at: now,
                dedup_hash: "h".into(),
            })
            .await
            .unwrap();
        store
            .insert_discrepancy(&Discrepancy {
                company_id: id,
                field: "Revenue".into(),
                period: None,
                source_a: "edgar".into(),
                value_a: 1.0,
                source_b: "fmp".into(),
                value_b: 2.0,
                pct_diff: 0.5,
                flagged_at: now,
            })
            .await
            .unwrap();
        let rid = store.start_run("edgar", Some("AAPL"), now).await.unwrap();
        store.finish_run(rid, "ok", now, None).await.unwrap();
        (Arc::new(store), dir)
    }

    async fn call(store: Arc<Store>, uri: &str) -> (StatusCode, Value) {
        let resp = app(store)
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn company_endpoint_returns_company() {
        let (store, _d) = seeded().await;
        let (status, json) = call(store, "/api/companies/AAPL").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["ticker"], "AAPL");
        assert_eq!(json["exchange"], "NASDAQ");
    }

    #[tokio::test]
    async fn unknown_ticker_returns_404() {
        let (store, _d) = seeded().await;
        let (status, _) = call(store, "/api/companies/NOPE").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn prices_and_facts_accept_range_params() {
        let (store, _d) = seeded().await;
        // limit keeps the (single) seeded price
        let (status, json) = call(store.clone(), "/api/companies/AAPL/prices?limit=5").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap().len(), 1);
        // from far in the future filters everything out
        let (_s, json) = call(store, "/api/companies/AAPL/facts?from=2099-01-01").await;
        assert!(json.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_endpoints_return_records() {
        let (store, _d) = seeded().await;
        for (uri, key) in [
            ("/api/companies/AAPL/prices", "close"),
            ("/api/companies/AAPL/facts", "line_item"),
            ("/api/companies/AAPL/ratios", "metric"),
            ("/api/companies/AAPL/news", "title"),
            ("/api/companies/AAPL/discrepancies", "field"),
        ] {
            let (status, json) = call(store.clone(), uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            assert!(json.as_array().unwrap()[0].get(key).is_some(), "{uri}");
        }
    }

    #[tokio::test]
    async fn runs_endpoint_returns_runs() {
        let (store, _d) = seeded().await;
        let (status, json) = call(store, "/api/runs").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap()[0]["status"], "ok");
    }

    #[tokio::test]
    async fn list_endpoint_surfaces_store_error_as_500() {
        let (store, _d) = seeded().await;
        store.close().await;
        let (status, _) = call(store, "/api/companies/AAPL/prices").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
