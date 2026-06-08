//! REST API handlers. Thin layer over [`Store`]; resolves a ticker to a
//! company then returns its records as JSON.

use std::sync::Arc;

use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::domain::{
    CollectionRun, Company, Discrepancy, FinancialFact, GrahamScore, NewsItem, PricePoint, Ratio,
};
use crate::graham;
use crate::store::{Store, StoreError};

type ApiResult<T> = Result<Json<T>, (StatusCode, String)>;
type ApiError = (StatusCode, String);

/// Authenticated user, extracted from a `Authorization: Bearer <token>` header.
pub struct AuthUser {
    pub user_id: i64,
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

#[axum::async_trait]
impl FromRequestParts<Arc<Store>> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, store: &Arc<Store>) -> Result<Self, ApiError> {
        let token = bearer(&parts.headers)
            .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token".into()))?;
        let user_id = store
            .session_user(&auth::hash_token(token), Utc::now())
            .await
            .map_err(internal)?
            .ok_or((StatusCode::UNAUTHORIZED, "invalid or expired token".into()))?;
        Ok(AuthUser { user_id })
    }
}

/// Optional `?from=YYYY-MM-DD&to=YYYY-MM-DD&limit=N` range params.
#[derive(Deserialize)]
struct RangeParams {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    limit: Option<i64>,
}

/// All API routes, parameterized over a shared [`Store`].
pub fn routes() -> Router<Arc<Store>> {
    Router::new()
        .route("/auth/signup", post(signup))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/api/watchlist", get(watchlist).post(watch_add))
        .route("/api/watchlist/:ticker", delete(watch_remove))
        .route("/api/companies/:ticker", get(company))
        .route("/api/companies/:ticker/prices", get(prices))
        .route("/api/companies/:ticker/facts", get(facts))
        .route("/api/companies/:ticker/ratios", get(ratios))
        .route("/api/companies/:ticker/news", get(news))
        .route("/api/companies/:ticker/discrepancies", get(discrepancies))
        .route("/api/companies/:ticker/graham", get(graham_assessment))
        .route("/api/companies/:ticker/summary", get(summary))
        .route("/api/screen", get(screen))
        .route("/api/runs", get(runs))
}

#[derive(Deserialize)]
struct ScreenParams {
    defensive: Option<bool>,
    min_score: Option<i64>,
    limit: Option<i64>,
}

#[derive(Serialize)]
struct ScreenRow {
    company: Company,
    score: GrahamScore,
}

async fn graham_assessment(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
) -> ApiResult<graham::GrahamAssessment> {
    let c = resolve(&store, &ticker).await?;
    let facts = store.get_facts(c.id).await.map_err(internal)?;
    let price = store.latest_price(c.id).await.map_err(internal)?;
    Ok(Json(graham::assess(&facts, price, graham::DEFAULT_MIN_REVENUE)))
}

#[derive(Serialize)]
struct CompanySummary {
    company: Company,
    ratios: Vec<Ratio>,
    graham: Option<GrahamScore>,
}

/// One round trip for the dashboard header: company + ratios + Graham score.
async fn summary(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
) -> ApiResult<CompanySummary> {
    let company = resolve(&store, &ticker).await?;
    let ratios = store.get_ratios(company.id).await.map_err(internal)?;
    let graham = store.get_graham_score(company.id).await.map_err(internal)?;
    Ok(Json(CompanySummary { company, ratios, graham }))
}

async fn screen(
    State(store): State<Arc<Store>>,
    Query(p): Query<ScreenParams>,
    _user: AuthUser,
) -> ApiResult<Vec<ScreenRow>> {
    let rows = store
        .screen(p.defensive.unwrap_or(false), p.min_score.unwrap_or(0), p.limit.unwrap_or(100))
        .await
        .map_err(internal)?;
    Ok(Json(rows.into_iter().map(|(company, score)| ScreenRow { company, score }).collect()))
}

fn internal(e: StoreError) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct TokenResponse {
    token: String,
}

#[derive(Serialize)]
struct Me {
    email: String,
}

#[derive(Deserialize)]
struct WatchRequest {
    ticker: String,
}

/// Create a 30-day session for a user, returning the raw token.
async fn issue_session(store: &Store, user_id: i64) -> Result<String, ApiError> {
    let (token, token_hash) = auth::new_token();
    store
        .create_session(&token_hash, user_id, Utc::now() + Duration::days(30))
        .await
        .map_err(internal)?;
    Ok(token)
}

async fn signup(
    State(store): State<Arc<Store>>,
    Json(c): Json<Credentials>,
) -> Result<(StatusCode, Json<TokenResponse>), ApiError> {
    let user_id = store
        .create_user(&c.email, &auth::hash_password(&c.password))
        .await
        .map_err(|_| (StatusCode::CONFLICT, "email already registered".into()))?;
    let token = issue_session(&store, user_id).await?;
    Ok((StatusCode::CREATED, Json(TokenResponse { token })))
}

async fn login(
    State(store): State<Arc<Store>>,
    Json(c): Json<Credentials>,
) -> ApiResult<TokenResponse> {
    let bad = || (StatusCode::UNAUTHORIZED, "invalid credentials".to_string());
    let (user_id, hash) = store.user_credentials(&c.email).await.map_err(internal)?.ok_or_else(bad)?;
    if !auth::verify_password(&hash, &c.password) {
        return Err(bad());
    }
    Ok(Json(TokenResponse {
        token: issue_session(&store, user_id).await?,
    }))
}

async fn logout(State(store): State<Arc<Store>>, headers: HeaderMap) -> StatusCode {
    if let Some(token) = bearer(&headers) {
        let _ = store.delete_session(&auth::hash_token(token)).await;
    }
    StatusCode::NO_CONTENT
}

async fn me(State(store): State<Arc<Store>>, user: AuthUser) -> ApiResult<Me> {
    let email = store
        .user_email(user.user_id)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "no such user".into()))?;
    Ok(Json(Me { email }))
}

async fn watchlist(State(store): State<Arc<Store>>, user: AuthUser) -> ApiResult<Vec<Company>> {
    Ok(Json(store.list_watch(user.user_id).await.map_err(internal)?))
}

async fn watch_add(
    State(store): State<Arc<Store>>,
    user: AuthUser,
    Json(w): Json<WatchRequest>,
) -> Result<StatusCode, ApiError> {
    let c = resolve(&store, &w.ticker).await?;
    store.add_watch(user.user_id, c.id).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn watch_remove(
    State(store): State<Arc<Store>>,
    user: AuthUser,
    Path(ticker): Path<String>,
) -> Result<StatusCode, ApiError> {
    let c = resolve(&store, &ticker).await?;
    store.remove_watch(user.user_id, c.id).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn resolve(store: &Store, ticker: &str) -> Result<Company, (StatusCode, String)> {
    store
        .get_company(ticker)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, format!("unknown ticker: {ticker}")))
}

async fn company(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
) -> ApiResult<Company> {
    Ok(Json(resolve(&store, &ticker).await?))
}

async fn prices(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    Query(rp): Query<RangeParams>,
    _user: AuthUser,
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
    _user: AuthUser,
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
    _user: AuthUser,
) -> ApiResult<Vec<Ratio>> {
    let c = resolve(&store, &ticker).await?;
    Ok(Json(store.get_ratios(c.id).await.map_err(internal)?))
}

async fn news(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
) -> ApiResult<Vec<NewsItem>> {
    let c = resolve(&store, &ticker).await?;
    Ok(Json(store.get_news(c.id).await.map_err(internal)?))
}

async fn discrepancies(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
) -> ApiResult<Vec<Discrepancy>> {
    let c = resolve(&store, &ticker).await?;
    Ok(Json(store.get_discrepancies(c.id).await.map_err(internal)?))
}

async fn runs(State(store): State<Arc<Store>>, _user: AuthUser) -> ApiResult<Vec<CollectionRun>> {
    Ok(Json(store.recent_runs(50).await.map_err(internal)?))
}

#[cfg(test)]
mod tests {
    use crate::app;
    use crate::auth;
    use crate::domain::*;
    use crate::store::Store;
    use axum::body::Body;
    use axum::http::header::AUTHORIZATION;
    use axum::http::{Request, StatusCode};
    use chrono::{Duration, NaiveDate, TimeZone, Utc};
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tower::ServiceExt;

    // Returns the store, its temp dir, and a valid bearer token for a seeded user.
    async fn seeded() -> (Arc<Store>, tempfile::TempDir, String) {
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
        store
            .save_graham_score(&GrahamScore {
                company_id: id,
                score: 6,
                passes_defensive: true,
                graham_number: Some(150.0),
                ncav_per_share: None,
                margin_of_safety: Some(0.1),
                net_net: false,
                computed_at: now,
            })
            .await
            .unwrap();

        // A user + valid (far-future, real-time) session token.
        let uid = store
            .create_user("u@e.com", &auth::hash_password("pw"))
            .await
            .unwrap();
        let (token, token_hash) = auth::new_token();
        store
            .create_session(&token_hash, uid, Utc::now() + Duration::days(1))
            .await
            .unwrap();
        (Arc::new(store), dir, token)
    }

    fn req(method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> Request<Body> {
        let mut b = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            b = b.header(AUTHORIZATION, format!("Bearer {t}"));
        }
        match body {
            Some(v) => b
                .header("content-type", "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        }
    }

    async fn send(store: Arc<Store>, request: Request<Body>) -> (StatusCode, Value) {
        let resp = app(store).oneshot(request).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json)
    }

    async fn get(store: Arc<Store>, token: &str, uri: &str) -> (StatusCode, Value) {
        send(store, req("GET", uri, Some(token), None)).await
    }

    #[tokio::test]
    async fn requires_a_bearer_token() {
        let (store, _d, _t) = seeded().await;
        // missing token
        let (s1, _) = send(store.clone(), req("GET", "/api/companies/AAPL", None, None)).await;
        assert_eq!(s1, StatusCode::UNAUTHORIZED);
        // invalid token
        let (s2, _) = get(store, "garbage", "/api/companies/AAPL").await;
        assert_eq!(s2, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn company_and_list_endpoints_return_records_when_authed() {
        let (store, _d, t) = seeded().await;
        let (status, json) = get(store.clone(), &t, "/api/companies/AAPL").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["ticker"], "AAPL");

        for (uri, key) in [
            ("/api/companies/AAPL/prices", "close"),
            ("/api/companies/AAPL/facts", "line_item"),
            ("/api/companies/AAPL/ratios", "metric"),
            ("/api/companies/AAPL/news", "title"),
            ("/api/companies/AAPL/discrepancies", "field"),
        ] {
            let (status, json) = get(store.clone(), &t, uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            assert!(json.as_array().unwrap()[0].get(key).is_some(), "{uri}");
        }
        let (status, json) = get(store.clone(), &t, "/api/runs").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap()[0]["status"], "ok");

        let (status, _) = get(store, &t, "/api/companies/NOPE").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn graham_and_screen_endpoints() {
        let (store, _d, t) = seeded().await;
        let (status, json) = get(store.clone(), &t, "/api/companies/AAPL/graham").await;
        assert_eq!(status, StatusCode::OK);
        assert!(json.get("criteria").is_some());
        assert!(json.get("score").is_some());

        let (status, json) = get(store.clone(), &t, "/api/screen?defensive=true&min_score=1").await;
        assert_eq!(status, StatusCode::OK);
        let rows = json.as_array().unwrap();
        assert_eq!(rows[0]["company"]["ticker"], "AAPL");
        assert_eq!(rows[0]["score"]["score"], 6);

        // aggregate summary
        let (status, json) = get(store, &t, "/api/companies/AAPL/summary").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["company"]["ticker"], "AAPL");
        assert_eq!(json["graham"]["score"], 6);
        assert!(json["ratios"].is_array());
    }

    #[tokio::test]
    async fn range_params_filter() {
        let (store, _d, t) = seeded().await;
        let (status, json) = get(store.clone(), &t, "/api/companies/AAPL/prices?limit=5").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap().len(), 1);
        let (_s, json) = get(store, &t, "/api/companies/AAPL/facts?from=2099-01-01").await;
        assert!(json.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn store_error_surfaces_as_500() {
        let (store, _d, t) = seeded().await;
        store.close().await;
        let (status, _) = get(store, &t, "/api/companies/AAPL/prices").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn signup_login_me_and_logout() {
        let (store, _d, _t) = seeded().await;
        // signup
        let body = json!({"email": "new@e.com", "password": "pw"});
        let (status, json) = send(store.clone(), req("POST", "/auth/signup", None, Some(body.clone()))).await;
        assert_eq!(status, StatusCode::CREATED);
        let token = json["token"].as_str().unwrap().to_string();
        // duplicate email -> 409
        let (status, _) = send(store.clone(), req("POST", "/auth/signup", None, Some(body))).await;
        assert_eq!(status, StatusCode::CONFLICT);
        // me with the signup token
        let (status, json) = get(store.clone(), &token, "/auth/me").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["email"], "new@e.com");
        // login
        let (status, json) = send(
            store.clone(),
            req("POST", "/auth/login", None, Some(json!({"email":"new@e.com","password":"pw"}))),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let token2 = json["token"].as_str().unwrap().to_string();
        // wrong password / unknown email -> 401
        let (s_bad, _) = send(
            store.clone(),
            req("POST", "/auth/login", None, Some(json!({"email":"new@e.com","password":"x"}))),
        )
        .await;
        assert_eq!(s_bad, StatusCode::UNAUTHORIZED);
        let (s_unknown, _) = send(
            store.clone(),
            req("POST", "/auth/login", None, Some(json!({"email":"no@e.com","password":"pw"}))),
        )
        .await;
        assert_eq!(s_unknown, StatusCode::UNAUTHORIZED);
        // logout invalidates token2
        let (status, _) = send(store.clone(), req("POST", "/auth/logout", Some(&token2), None)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = get(store, &token2, "/auth/me").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn watchlist_add_list_remove() {
        let (store, _d, t) = seeded().await;
        // add
        let (status, _) = send(
            store.clone(),
            req("POST", "/api/watchlist", Some(&t), Some(json!({"ticker":"AAPL"}))),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        // list
        let (status, json) = get(store.clone(), &t, "/api/watchlist").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap()[0]["ticker"], "AAPL");
        // remove
        let (status, _) = send(store.clone(), req("DELETE", "/api/watchlist/AAPL", Some(&t), None)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_s, json) = get(store, &t, "/api/watchlist").await;
        assert!(json.as_array().unwrap().is_empty());
    }
}
