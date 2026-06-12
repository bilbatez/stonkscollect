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
    ShareCount,
};
use crate::graham;
use crate::store::{ScreenFilter, Store, StoreError};

type ApiResult<T> = Result<Json<T>, (StatusCode, String)>;
type ApiError = (StatusCode, String);

/// Default page size when a list request omits `limit`.
const DEFAULT_PAGE_LIMIT: i64 = 50;
/// Max sector peers returned for a company.
const PEERS_LIMIT: i64 = 20;
/// Session lifetime before re-authentication is required.
const SESSION_TTL_DAYS: i64 = 30;
/// Number of recent collection runs surfaced by `/api/runs`.
const RECENT_RUNS_LIMIT: i64 = 50;

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
        .route("/api/companies", get(companies))
        .route("/api/companies/:ticker", get(company))
        .route("/api/companies/:ticker/prices", get(prices))
        .route("/api/companies/:ticker/facts", get(facts))
        .route("/api/companies/:ticker/ratios", get(ratios))
        .route("/api/companies/:ticker/news", get(news))
        .route("/api/companies/:ticker/discrepancies", get(discrepancies))
        .route("/api/companies/:ticker/errors", get(source_errors))
        .route("/api/companies/:ticker/graham", get(graham_assessment))
        .route("/api/companies/:ticker/summary", get(summary))
        .route("/api/companies/:ticker/peers", get(peers))
        .route("/api/companies/:ticker/note", get(get_note).put(save_note).delete(delete_note))
        .route("/api/screen", get(screen))
        .route("/api/sectors", get(sectors))
        .route("/api/runs", get(runs))
}

#[derive(Deserialize)]
struct ScreenParams {
    defensive: Option<bool>,
    net_net: Option<bool>,
    min_score: Option<i64>,
    sector: Option<String>,
    min_pe: Option<f64>,
    max_pe: Option<f64>,
    min_roe: Option<f64>,
    max_de: Option<f64>,
    min_margin: Option<f64>,
    sort_by: Option<String>,
    sort_dir: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct ScreenRow {
    company: Company,
    score: GrahamScore,
}

/// A page of results plus the total match count (for client pagination).
#[derive(Serialize)]
struct Page<T> {
    rows: Vec<T>,
    total: i64,
}

#[derive(Deserialize)]
struct CompaniesParams {
    q: Option<String>,
    sort_by: Option<String>,
    sort_dir: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct CompanyRow {
    company: Company,
    score: Option<GrahamScore>,
}

/// Paginated, optionally-searched directory of all companies, each with its
/// Graham score when computed.
async fn companies(
    State(store): State<Arc<Store>>,
    Query(p): Query<CompaniesParams>,
    _user: AuthUser,
) -> ApiResult<Page<CompanyRow>> {
    let (rows, total) = store
        .list_companies(
            p.q.as_deref(),
            p.sort_by.as_deref(),
            p.sort_dir.as_deref(),
            p.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            p.offset.unwrap_or(0),
        )
        .await
        .map_err(internal)?;
    Ok(Json(Page {
        rows: rows.into_iter().map(|(company, score)| CompanyRow { company, score }).collect(),
        total,
    }))
}

async fn graham_assessment(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
) -> ApiResult<graham::GrahamAssessment> {
    let c = resolve(&store, &ticker).await?;
    let facts = store.get_facts(c.id).await.map_err(internal)?;
    let price = store.latest_price(c.id).await.map_err(internal)?;
    Ok(Json(graham::assess(&facts, price, store.policy().graham_min_revenue)))
}

#[derive(Serialize)]
struct CompanySummary {
    company: Company,
    ratios: Vec<Ratio>,
    graham: Option<GrahamScore>,
    shares: Option<ShareCount>,
}

/// One round trip for the dashboard header: company + ratios + Graham score
/// + latest known share count.
async fn summary(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
) -> ApiResult<CompanySummary> {
    let company = resolve(&store, &ticker).await?;
    let ratios = store.get_ratios(company.id, None).await.map_err(internal)?;
    let graham = store.get_graham_score(company.id).await.map_err(internal)?;
    let shares = store.latest_shares(company.id).await.map_err(internal)?;
    Ok(Json(CompanySummary { company, ratios, graham, shares }))
}

async fn screen(
    State(store): State<Arc<Store>>,
    Query(p): Query<ScreenParams>,
    _user: AuthUser,
) -> ApiResult<Page<ScreenRow>> {
    let (rows, total) = store.screen(&p.into()).await.map_err(internal)?;
    Ok(Json(Page {
        rows: rows.into_iter().map(|(company, score)| ScreenRow { company, score }).collect(),
        total,
    }))
}

impl From<ScreenParams> for ScreenFilter {
    fn from(p: ScreenParams) -> Self {
        ScreenFilter {
            defensive_only: p.defensive.unwrap_or(false),
            net_net_only: p.net_net.unwrap_or(false),
            min_score: p.min_score.unwrap_or(0),
            sector: p.sector,
            min_pe: p.min_pe,
            max_pe: p.max_pe,
            min_roe: p.min_roe,
            max_de: p.max_de,
            min_margin: p.min_margin,
            sort_by: p.sort_by,
            sort_dir: p.sort_dir,
            limit: p.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            offset: p.offset.unwrap_or(0),
        }
    }
}

async fn sectors(
    State(store): State<Arc<Store>>,
    _user: AuthUser,
) -> ApiResult<Vec<crate::domain::SectorStats>> {
    Ok(Json(store.get_sectors().await.map_err(internal)?))
}

#[derive(Serialize)]
struct PeerRow {
    company: Company,
    score: Option<GrahamScore>,
}

async fn peers(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
) -> ApiResult<Vec<PeerRow>> {
    let c = resolve(&store, &ticker).await?;
    let rows = store.get_peers(c.id, c.sector.as_deref(), PEERS_LIMIT).await.map_err(internal)?;
    Ok(Json(
        rows.into_iter().map(|(company, score)| PeerRow { company, score }).collect(),
    ))
}

#[derive(Serialize)]
struct NoteResponse {
    body: Option<String>,
}

#[derive(Deserialize)]
struct NoteRequest {
    body: String,
}

async fn get_note(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    user: AuthUser,
) -> ApiResult<NoteResponse> {
    let c = resolve(&store, &ticker).await?;
    let body = store.get_note(user.user_id, c.id).await.map_err(internal)?;
    Ok(Json(NoteResponse { body }))
}

async fn save_note(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    user: AuthUser,
    Json(req): Json<NoteRequest>,
) -> Result<StatusCode, ApiError> {
    let c = resolve(&store, &ticker).await?;
    store.save_note(user.user_id, c.id, &req.body, Utc::now()).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_note(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    user: AuthUser,
) -> Result<StatusCode, ApiError> {
    let c = resolve(&store, &ticker).await?;
    store.delete_note(user.user_id, c.id).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Log the real cause server-side; never surface store/SQL detail to the client.
fn internal(e: StoreError) -> ApiError {
    tracing::error!(error = %e, "store error");
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
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
        .create_session(&token_hash, user_id, Utc::now() + Duration::days(SESSION_TTL_DAYS))
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
    let now = Utc::now();
    let throttle = store.login_throttle();
    if !throttle.allowed(&c.email, now) {
        return Err((StatusCode::TOO_MANY_REQUESTS, "too many login attempts".into()));
    }
    let bad = || (StatusCode::UNAUTHORIZED, "invalid credentials".to_string());
    match store.user_credentials(&c.email).await.map_err(internal)? {
        Some((user_id, hash)) if auth::verify_password(&hash, &c.password) => {
            throttle.clear(&c.email);
            Ok(Json(TokenResponse { token: issue_session(&store, user_id).await? }))
        }
        _ => {
            throttle.record_failure(&c.email, now);
            Err(bad())
        }
    }
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
    Ok(Json(store.get_ratios(c.id, None).await.map_err(internal)?))
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

/// Recent per-source collection failures for a company ("why is this ticker's
/// data stale / partial?").
async fn source_errors(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
) -> ApiResult<Vec<crate::domain::SourceError>> {
    let c = resolve(&store, &ticker).await?;
    Ok(Json(store.recent_source_errors(c.id, RECENT_RUNS_LIMIT).await.map_err(internal)?))
}

async fn runs(State(store): State<Arc<Store>>, _user: AuthUser) -> ApiResult<Vec<CollectionRun>> {
    Ok(Json(store.recent_runs(RECENT_RUNS_LIMIT).await.map_err(internal)?))
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
                open: None,
                high: None,
                low: None,
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
        for (metric, value) in [("pe", 28.5_f64), ("roe", 0.15), ("debt_to_equity", 0.5), ("net_margin", 0.10)] {
            store
                .upsert_ratio(&Ratio {
                    company_id: id,
                    period_end: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    period_type: PeriodType::Annual,
                    metric: metric.into(),
                    value,
                    computed_at: now,
                })
                .await
                .unwrap();
        }
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
    async fn graham_endpoint_uses_configured_min_revenue() {
        use crate::store::Policy;
        let (store, _dir) = crate::testutil::temp_store().await;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let id = store
            .insert_company(&NewCompany {
                cik: "0000320193".into(),
                ticker: "AAPL".into(),
                name: "Apple Inc.".into(),
                exchange: None,
                sector: None,
                industry: None,
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
        let uid = store.create_user("u@e.com", &auth::hash_password("pw")).await.unwrap();
        let (token, token_hash) = auth::new_token();
        store.create_session(&token_hash, uid, Utc::now() + Duration::days(1)).await.unwrap();
        // A min-revenue below the seeded 1.0 revenue makes "Adequate size" pass;
        // the default (500M) would fail it. Passing proves the endpoint reads the
        // configured policy rather than graham::DEFAULT_MIN_REVENUE.
        let store = Arc::new(store.with_policy(Policy { graham_min_revenue: 0.5 }));
        let (status, body) = get(store, &token, "/api/companies/AAPL/graham").await;
        assert_eq!(status, StatusCode::OK);
        let size = body["criteria"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "Adequate size")
            .unwrap();
        assert_eq!(size["passed"], true);
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
        assert_eq!(json["total"], 1);
        let rows = json["rows"].as_array().unwrap();
        assert_eq!(rows[0]["company"]["ticker"], "AAPL");
        assert_eq!(rows[0]["score"]["score"], 6);

        // paginated companies directory: AAPL present with its score; search works
        let (status, json) = get(store.clone(), &t, "/api/companies?limit=10").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["total"], 1);
        assert_eq!(json["rows"][0]["company"]["ticker"], "AAPL");
        assert_eq!(json["rows"][0]["score"]["score"], 6);
        let (_s, json) = get(store.clone(), &t, "/api/companies?q=zzz").await;
        assert_eq!(json["total"], 0);
        // net-net filter (none qualify) returns an empty page
        let (_s, json) = get(store.clone(), &t, "/api/screen?net_net=true").await;
        assert_eq!(json["total"], 0);

        // ratio filters: pe=28.5, roe=0.15, debt_to_equity=0.5, net_margin=0.10
        // max_pe=30 should include AAPL; max_pe=20 should exclude
        let (_s, json) = get(store.clone(), &t, "/api/screen?max_pe=30").await;
        assert_eq!(json["total"], 1);
        let (_s, json) = get(store.clone(), &t, "/api/screen?max_pe=20").await;
        assert_eq!(json["total"], 0);
        // min_pe=28 should include; min_pe=29 should exclude
        let (_s, json) = get(store.clone(), &t, "/api/screen?min_pe=28").await;
        assert_eq!(json["total"], 1);
        let (_s, json) = get(store.clone(), &t, "/api/screen?min_pe=29").await;
        assert_eq!(json["total"], 0);
        // min_roe=0.10 includes; min_roe=0.20 excludes
        let (_s, json) = get(store.clone(), &t, "/api/screen?min_roe=0.10").await;
        assert_eq!(json["total"], 1);
        let (_s, json) = get(store.clone(), &t, "/api/screen?min_roe=0.20").await;
        assert_eq!(json["total"], 0);
        // max_de=0.6 includes; max_de=0.3 excludes
        let (_s, json) = get(store.clone(), &t, "/api/screen?max_de=0.6").await;
        assert_eq!(json["total"], 1);
        let (_s, json) = get(store.clone(), &t, "/api/screen?max_de=0.3").await;
        assert_eq!(json["total"], 0);
        // min_margin=0.05 includes; min_margin=0.20 excludes
        let (_s, json) = get(store.clone(), &t, "/api/screen?min_margin=0.05").await;
        assert_eq!(json["total"], 1);
        let (_s, json) = get(store.clone(), &t, "/api/screen?min_margin=0.20").await;
        assert_eq!(json["total"], 0);

        // aggregate summary
        let (status, json) = get(store.clone(), &t, "/api/companies/AAPL/summary").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["company"]["ticker"], "AAPL");
        assert_eq!(json["graham"]["score"], 6);
        assert!(json["ratios"].is_array());
        assert!(json["shares"].is_null()); // none collected yet

        // once a share count exists, the summary carries the latest one
        let id = store.get_company("AAPL").await.unwrap().unwrap().id;
        store
            .save_shares(&[crate::domain::ShareCount {
                company_id: id,
                as_of: chrono::NaiveDate::from_ymd_opt(2023, 9, 30).unwrap(),
                shares: 15_550_061_000.0,
                source: "edgar".into(),
            }])
            .await
            .unwrap();
        let (_s, json) = get(store, &t, "/api/companies/AAPL/summary").await;
        assert_eq!(json["shares"]["shares"], 15_550_061_000.0);
        assert_eq!(json["shares"]["as_of"], "2023-09-30");
    }

    #[tokio::test]
    async fn company_source_errors_endpoint_lists_recent() {
        let (store, _d, t) = seeded().await;
        let id = store.get_company("AAPL").await.unwrap().unwrap().id;
        store
            .save_source_errors(id, &[("edgar".into(), "503".into())], chrono::Utc::now())
            .await
            .unwrap();
        let (status, json) = get(store.clone(), &t, "/api/companies/AAPL/errors").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json[0]["source"], "edgar");
        assert_eq!(json[0]["message"], "503");
        let (status, _) = get(store, &t, "/api/companies/NOPE/errors").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn companies_and_screen_sort_params() {
        let (store, _d, t) = seeded().await;
        // sort_by=score&sort_dir=desc: AAPL has score 6, it should appear in the results
        let (_s, json) = get(store.clone(), &t, "/api/companies?sort_by=score&sort_dir=desc&limit=10").await;
        assert_eq!(json["total"], 1);
        assert_eq!(json["rows"][0]["company"]["ticker"], "AAPL");
        // unknown sort_by falls back gracefully (no 500)
        let (status, _) = get(store.clone(), &t, "/api/companies?sort_by=bogus&sort_dir=desc&limit=10").await;
        assert_eq!(status, StatusCode::OK);
        // screen with sort_by=ticker asc
        let (status, json) = get(store.clone(), &t, "/api/screen?sort_by=ticker&sort_dir=asc").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["rows"][0]["company"]["ticker"], "AAPL");
    }

    #[tokio::test]
    async fn enum_fields_serialize_lowercase() {
        // The frontend filters facts/ratios on lowercase period_type/statement,
        // so the JSON must be lowercase (not the PascalCase enum variant names).
        let (store, _d, t) = seeded().await;
        let (_s, facts) = get(store.clone(), &t, "/api/companies/AAPL/facts").await;
        assert_eq!(facts[0]["period_type"], "annual");
        assert_eq!(facts[0]["statement"], "income");
        let (_s, ratios) = get(store, &t, "/api/companies/AAPL/ratios").await;
        assert_eq!(ratios[0]["period_type"], "annual");
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
    async fn internal_error_body_does_not_leak_details() {
        let (store, _d, t) = seeded().await;
        store.close().await;
        let resp = app(store)
            .oneshot(req("GET", "/api/companies/AAPL/prices", Some(&t), None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], b"internal error");
    }

    #[tokio::test]
    async fn oversize_request_body_is_rejected() {
        let (store, _d, _t) = seeded().await;
        // A body well past the configured limit must be refused before handling.
        let big = json!({"email": "x".repeat(200_000), "password": "pw"});
        let (status, _) = send(store, req("POST", "/auth/signup", None, Some(big))).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
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
    async fn login_is_throttled_after_repeated_failures() {
        use crate::net::LOGIN_MAX_ATTEMPTS;
        let (store, _d, _t) = seeded().await;
        // Seeded user is u@e.com / "pw". Exhaust the attempt budget with bad ones.
        let bad = json!({"email": "u@e.com", "password": "wrong"});
        for _ in 0..LOGIN_MAX_ATTEMPTS {
            let (s, _) = send(store.clone(), req("POST", "/auth/login", None, Some(bad.clone()))).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED);
        }
        // Now even the correct password is throttled.
        let good = json!({"email": "u@e.com", "password": "pw"});
        let (s, _) = send(store, req("POST", "/auth/login", None, Some(good))).await;
        assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn successful_login_clears_throttle_counter() {
        use crate::net::LOGIN_MAX_ATTEMPTS;
        let (store, _d, _t) = seeded().await;
        let bad = json!({"email": "u@e.com", "password": "wrong"});
        // One short of the limit, then a success resets the counter.
        for _ in 0..(LOGIN_MAX_ATTEMPTS - 1) {
            send(store.clone(), req("POST", "/auth/login", None, Some(bad.clone()))).await;
        }
        let good = json!({"email": "u@e.com", "password": "pw"});
        let (s, _) = send(store.clone(), req("POST", "/auth/login", None, Some(good.clone()))).await;
        assert_eq!(s, StatusCode::OK);
        // After the reset, another full run of failures is needed to block again.
        for _ in 0..(LOGIN_MAX_ATTEMPTS - 1) {
            send(store.clone(), req("POST", "/auth/login", None, Some(bad.clone()))).await;
        }
        let (s, _) = send(store, req("POST", "/auth/login", None, Some(good))).await;
        assert_eq!(s, StatusCode::OK); // still allowed: counter was cleared
    }

    #[tokio::test]
    async fn peers_and_notes_endpoints() {
        let (store, _d, t) = seeded().await;
        // Give AAPL a sector so get_peers runs the SQL path.
        let aapl = store.get_company("AAPL").await.unwrap().unwrap();
        store
            .update_company_profile(
                aapl.id,
                &crate::domain::CompanyProfile {
                    sector: Some("Technology".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        // Insert a peer in the same sector.
        store
            .insert_company(&crate::domain::NewCompany {
                cik: "0000789019".into(),
                ticker: "MSFT".into(),
                name: "Microsoft Corp.".into(),
                exchange: Some("NASDAQ".into()),
                sector: Some("Technology".into()),
                industry: None,
            })
            .await
            .unwrap();

        // peers: AAPL now has sector=Technology; MSFT is in same sector
        let (status, json) = get(store.clone(), &t, "/api/companies/AAPL/peers").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap()[0]["company"]["ticker"], "MSFT");

        // note: none yet
        let (status, json) = get(store.clone(), &t, "/api/companies/AAPL/note").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["body"], Value::Null);

        // save note
        let body = json!({"body": "interesting stock"});
        let (status, _) = send(store.clone(), req("PUT", "/api/companies/AAPL/note", Some(&t), Some(body))).await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        // retrieve saved note
        let (status, json) = get(store.clone(), &t, "/api/companies/AAPL/note").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["body"], "interesting stock");

        // delete note
        let (status, _) = send(store.clone(), req("DELETE", "/api/companies/AAPL/note", Some(&t), None)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        // gone after delete
        let (_, json) = get(store.clone(), &t, "/api/companies/AAPL/note").await;
        assert_eq!(json["body"], Value::Null);

        // screen with sector filter (AAPL now has sector=Technology)
        let (status, json) = get(store.clone(), &t, "/api/screen?sector=Technology").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["total"], 1);
        // screen with a different sector -> empty
        let (status, json) = get(store.clone(), &t, "/api/screen?sector=Healthcare").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["total"], 0);
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

    #[tokio::test]
    async fn sectors_endpoint() {
        let (store, _d, t) = seeded().await;
        // AAPL has no sector yet — sectors endpoint should return empty
        let (status, json) = get(store.clone(), &t, "/api/sectors").await;
        assert_eq!(status, StatusCode::OK);
        assert!(json.as_array().unwrap().is_empty());

        // Give AAPL a sector and re-check
        let aapl = store.get_company("AAPL").await.unwrap().unwrap();
        store
            .update_company_profile(
                aapl.id,
                &crate::domain::CompanyProfile {
                    sector: Some("Technology".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let (status, json) = get(store.clone(), &t, "/api/sectors").await;
        assert_eq!(status, StatusCode::OK);
        let rows = json.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["sector"], "Technology");
        assert_eq!(rows[0]["company_count"], 1);
        assert!(rows[0]["avg_score"].as_f64().unwrap() > 0.0);
        assert_eq!(rows[0]["top_ticker"], "AAPL");
    }
}
