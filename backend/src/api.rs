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
    select_movers, CollectionRun, Company, Discrepancy, FinancialFact, GrahamScore, Movers,
    NewsItem, PricePoint, Ratio, ShareCount, UserSettings, WatchGroup,
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
/// Default rows per movers bucket (gainers / losers / most active).
const MOVERS_LIMIT: usize = 10;

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
        .route("/auth/profile", axum::routing::put(update_profile))
        .route("/auth/password", axum::routing::put(change_password))
        .route("/auth/settings", get(user_settings).put(update_settings))
        .route("/api/watchlist", get(watchlist).post(watch_add))
        .route("/api/watchlist/quotes", get(watchlist_quotes))
        .route("/api/watchlist/groups", get(list_groups).post(create_group))
        .route(
            "/api/watchlist/groups/:id",
            axum::routing::put(rename_group).delete(delete_group),
        )
        .route("/api/watchlist/:ticker", delete(watch_remove))
        .route("/api/watchlist/:ticker/groups", post(tag_watch))
        .route("/api/watchlist/:ticker/groups/:id", delete(untag_watch))
        .route("/api/companies", get(companies))
        .route("/api/companies/:ticker", get(company))
        .route("/api/companies/:ticker/status", axum::routing::put(set_status))
        .route("/api/companies/:ticker/prices", get(prices))
        .route("/api/companies/:ticker/facts", get(facts))
        .route("/api/companies/:ticker/ratios", get(ratios))
        .route("/api/companies/:ticker/news", get(news))
        .route("/api/companies/:ticker/holders", get(holders))
        .route("/api/companies/:ticker/discrepancies", get(discrepancies))
        .route("/api/companies/:ticker/errors", get(source_errors))
        .route("/api/companies/:ticker/graham", get(graham_assessment))
        .route("/api/companies/:ticker/summary", get(summary))
        .route("/api/companies/:ticker/peers", get(peers))
        .route("/api/companies/:ticker/note", get(get_note).put(save_note).delete(delete_note))
        .route("/api/screen", get(screen))
        .route("/api/sectors", get(sectors))
        .route("/api/movers", get(movers))
        .route("/api/markets/summary", get(markets_summary))
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
    ticker: Option<String>,
    name: Option<String>,
    industry: Option<String>,
    sort_by: Option<String>,
    sort_dir: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    include_delisted: Option<bool>,
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
    let mut filters: Vec<(&str, &str)> = Vec::new();
    if let Some(v) = p.ticker.as_deref() {
        filters.push(("ticker", v));
    }
    if let Some(v) = p.name.as_deref() {
        filters.push(("name", v));
    }
    if let Some(v) = p.industry.as_deref() {
        filters.push(("industry", v));
    }
    let (rows, total) = store
        .list_companies(
            p.q.as_deref(),
            &filters,
            p.sort_by.as_deref(),
            p.sort_dir.as_deref(),
            p.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            p.offset.unwrap_or(0),
            p.include_delisted.unwrap_or(false),
        )
        .await
        .map_err(internal)?;
    Ok(Json(Page {
        rows: rows.into_iter().map(|(company, score)| CompanyRow { company, score }).collect(),
        total,
    }))
}

/// Body for the manual listing-status override endpoint.
#[derive(Deserialize)]
struct StatusUpdate {
    status: String,
}

/// Manually set a company's listing status (`active`/`delisted`). 400 on a
/// value outside the allowed set.
async fn set_status(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
    Json(s): Json<StatusUpdate>,
) -> Result<StatusCode, ApiError> {
    if s.status != "active" && s.status != "delisted" {
        return Err((StatusCode::BAD_REQUEST, "status must be active or delisted".into()));
    }
    let c = resolve(&store, &ticker).await?;
    store.set_company_status(c.id, &s.status).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn graham_assessment(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    user: AuthUser,
) -> ApiResult<graham::GrahamAssessment> {
    let c = resolve(&store, &ticker).await?;
    let facts = store.get_facts(c.id).await.map_err(internal)?;
    let price = store.latest_price(c.id).await.map_err(internal)?;
    // Assess against the requesting user's configured Graham thresholds.
    let cfg = store.get_settings(user.user_id).await.map_err(internal)?.graham;
    Ok(Json(graham::assess(&facts, price, &cfg)))
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

#[derive(Deserialize)]
struct MoversParams {
    limit: Option<usize>,
}

/// Market movers: top gainers / losers / most-active by the latest daily move.
async fn movers(
    State(store): State<Arc<Store>>,
    Query(p): Query<MoversParams>,
    _user: AuthUser,
) -> ApiResult<Movers> {
    let rows = store.day_changes().await.map_err(internal)?;
    Ok(Json(select_movers(rows, p.limit.unwrap_or(MOVERS_LIMIT))))
}

/// Market summary: each tracked index's latest close and day change.
async fn markets_summary(
    State(store): State<Arc<Store>>,
    _user: AuthUser,
) -> ApiResult<Vec<crate::domain::MoverRow>> {
    Ok(Json(store.index_changes().await.map_err(internal)?))
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
    display_name: String,
}

#[derive(Deserialize)]
struct UpdateProfile {
    email: String,
    display_name: String,
}

#[derive(Deserialize)]
struct ChangePassword {
    old_password: String,
    new_password: String,
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
    let (email, display_name) = store
        .user_profile(user.user_id)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "no such user".into()))?;
    Ok(Json(Me { email, display_name }))
}

/// Update the authenticated user's email + display name. 409 on duplicate email.
async fn update_profile(
    State(store): State<Arc<Store>>,
    user: AuthUser,
    Json(p): Json<UpdateProfile>,
) -> Result<StatusCode, ApiError> {
    store
        .update_profile(user.user_id, &p.email, &p.display_name)
        .await
        .map_err(|_| (StatusCode::CONFLICT, "email already registered".into()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Change the authenticated user's password after verifying the old one.
async fn change_password(
    State(store): State<Arc<Store>>,
    user: AuthUser,
    Json(p): Json<ChangePassword>,
) -> Result<StatusCode, ApiError> {
    let hash = store
        .user_password_hash(user.user_id)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "no such user".into()))?;
    if !auth::verify_password(&hash, &p.old_password) {
        return Err((StatusCode::UNAUTHORIZED, "incorrect password".into()));
    }
    store
        .update_password_hash(user.user_id, &auth::hash_password(&p.new_password))
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The authenticated user's settings (theme + Graham thresholds).
async fn user_settings(State(store): State<Arc<Store>>, user: AuthUser) -> ApiResult<UserSettings> {
    Ok(Json(store.get_settings(user.user_id).await.map_err(internal)?))
}

/// Replace the authenticated user's settings.
async fn update_settings(
    State(store): State<Arc<Store>>,
    user: AuthUser,
    Json(s): Json<UserSettings>,
) -> Result<StatusCode, ApiError> {
    store.save_settings(user.user_id, &s).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn watchlist(State(store): State<Arc<Store>>, user: AuthUser) -> ApiResult<Vec<Company>> {
    Ok(Json(store.list_watch(user.user_id).await.map_err(internal)?))
}

/// The watchlist with each company's latest daily quote (price + change).
async fn watchlist_quotes(
    State(store): State<Arc<Store>>,
    user: AuthUser,
) -> ApiResult<Vec<crate::domain::WatchQuote>> {
    Ok(Json(store.watch_quotes(user.user_id).await.map_err(internal)?))
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

#[derive(Deserialize)]
struct GroupBody {
    name: String,
}

#[derive(Deserialize)]
struct TagBody {
    group_id: i64,
}

#[derive(Serialize)]
struct CreatedId {
    id: i64,
}

/// The authenticated user's watch groups.
async fn list_groups(State(store): State<Arc<Store>>, user: AuthUser) -> ApiResult<Vec<WatchGroup>> {
    Ok(Json(store.list_groups(user.user_id).await.map_err(internal)?))
}

/// Create a new watch group. 409 if the user already has one with that name.
async fn create_group(
    State(store): State<Arc<Store>>,
    user: AuthUser,
    Json(b): Json<GroupBody>,
) -> Result<(StatusCode, Json<CreatedId>), ApiError> {
    let id = store
        .create_group(user.user_id, &b.name)
        .await
        .map_err(|_| (StatusCode::CONFLICT, "group name already exists".into()))?;
    Ok((StatusCode::CREATED, Json(CreatedId { id })))
}

/// Rename one of the user's watch groups.
async fn rename_group(
    State(store): State<Arc<Store>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(b): Json<GroupBody>,
) -> Result<StatusCode, ApiError> {
    store.rename_group(user.user_id, id, &b.name).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Delete one of the user's watch groups (its memberships cascade away).
async fn delete_group(
    State(store): State<Arc<Store>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    store.delete_group(user.user_id, id).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Tag a watched company into one of the user's groups.
async fn tag_watch(
    State(store): State<Arc<Store>>,
    user: AuthUser,
    Path(ticker): Path<String>,
    Json(b): Json<TagBody>,
) -> Result<StatusCode, ApiError> {
    let c = resolve(&store, &ticker).await?;
    store.add_to_group(user.user_id, b.group_id, c.id).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Untag a watched company from one of the user's groups.
async fn untag_watch(
    State(store): State<Arc<Store>>,
    user: AuthUser,
    Path((ticker, id)): Path<(String, i64)>,
) -> Result<StatusCode, ApiError> {
    let c = resolve(&store, &ticker).await?;
    store.remove_from_group(user.user_id, id, c.id).await.map_err(internal)?;
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

async fn holders(
    State(store): State<Arc<Store>>,
    Path(ticker): Path<String>,
    _user: AuthUser,
) -> ApiResult<Vec<crate::domain::OwnershipHolding>> {
    let c = resolve(&store, &ticker).await?;
    Ok(Json(store.get_ownership(c.id).await.map_err(internal)?))
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
        // user's saved Graham config rather than graham::DEFAULT_MIN_REVENUE.
        store
            .save_settings(
                uid,
                &UserSettings {
                    theme: "system".into(),
                    graham: crate::graham::GrahamConfig { min_revenue: 0.5, ..Default::default() },
                },
            )
            .await
            .unwrap();
        let store = Arc::new(store);
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
    async fn settings_endpoints_round_trip_theme_and_graham() {
        let (store, _d, t) = seeded().await;
        // defaults
        let (status, json) = get(store.clone(), &t, "/auth/settings").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["theme"], "system");
        assert_eq!(json["graham"]["pe_max"], 15.0);
        // update theme + a Graham knob
        let body = json!({
            "theme": "dark",
            "graham": {
                "min_revenue": 1.0, "pe_max": 20.0, "pb_max": 1.5,
                "pe_pb_max": 22.5, "current_ratio_min": 2.0, "eps_growth_min": 0.33
            }
        });
        let (status, _) = send(store.clone(), req("PUT", "/auth/settings", Some(&t), Some(body))).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_s, json) = get(store, &t, "/auth/settings").await;
        assert_eq!(json["theme"], "dark");
        assert_eq!(json["graham"]["pe_max"], 20.0);
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
    async fn holders_endpoint_returns_saved_ownership() {
        let (store, _d, t) = seeded().await;
        let c = store.get_company("AAPL").await.unwrap().unwrap();
        store
            .save_ownership(&[crate::domain::OwnershipHolding {
                company_id: c.id,
                holder: "Tim Cook".into(),
                kind: "insider".into(),
                shares: 3_280_000.0,
                as_of: NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
                source: "edgar".into(),
            }])
            .await
            .unwrap();
        let (status, json) = get(store.clone(), &t, "/api/companies/AAPL/holders").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json[0]["holder"], "Tim Cook");
        assert_eq!(json[0]["shares"], 3_280_000.0);
        // unknown ticker -> 404
        let (status, _) = get(store, &t, "/api/companies/NOPE/holders").await;
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
    async fn movers_endpoint_ranks_gainers_losers_and_most_active() {
        // seeded() AAPL has a single price day -> excluded (no previous close).
        let (store, _d, t) = seeded().await;
        let day1 = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        let day2 = NaiveDate::from_ymd_opt(2024, 2, 2).unwrap();
        let mut ids = std::collections::HashMap::new();
        for ticker in ["UP", "DOWN", "ZERO"] {
            let id = store
                .insert_company(&NewCompany {
                    cik: ticker.into(),
                    ticker: ticker.into(),
                    name: ticker.into(),
                    exchange: None,
                    sector: None,
                    industry: None,
                })
                .await
                .unwrap();
            ids.insert(ticker, id);
        }
        let price = |id, date, close: f64, volume, source: &str| PricePoint {
            company_id: id,
            date,
            open: None,
            high: None,
            low: None,
            close,
            volume,
            source: source.into(),
        };
        store
            .save_prices(&[
                price(ids["UP"], day1, 100.0, None, "yahoo"),
                price(ids["UP"], day2, 110.0, Some(50), "yahoo"),
                price(ids["DOWN"], day1, 100.0, None, "yahoo"),
                price(ids["DOWN"], day2, 80.0, Some(900), "yahoo"),
                // an fmp row on the same date must lose to yahoo's
                price(ids["DOWN"], day2, 9_999.0, Some(1), "fmp"),
                // zero previous close -> excluded (change % undefined)
                price(ids["ZERO"], day1, 0.0, None, "yahoo"),
                price(ids["ZERO"], day2, 5.0, Some(7), "yahoo"),
            ])
            .await
            .unwrap();

        let (status, json) = get(store, &t, "/api/movers?limit=2").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["gainers"][0]["company"]["ticker"], "UP");
        assert_eq!(json["gainers"][0]["last_close"], 110.0);
        assert_eq!(json["gainers"][0]["change"], 10.0);
        assert!((json["gainers"][0]["change_pct"].as_f64().unwrap() - 0.1).abs() < 1e-9);
        assert_eq!(json["gainers"][0]["as_of"], "2024-02-02");
        assert_eq!(json["losers"][0]["company"]["ticker"], "DOWN");
        assert_eq!(json["losers"][0]["last_close"], 80.0); // yahoo, not fmp's 9999
        assert_eq!(json["most_active"][0]["company"]["ticker"], "DOWN");
        assert_eq!(json["most_active"][0]["volume"], 900);
        // AAPL (single day) and ZERO are excluded everywhere
        for bucket in ["gainers", "losers", "most_active"] {
            let tickers: Vec<_> = json[bucket]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["company"]["ticker"].as_str().unwrap().to_string())
                .collect();
            assert!(!tickers.contains(&"AAPL".to_string()), "{bucket}");
            assert!(!tickers.contains(&"ZERO".to_string()), "{bucket}");
        }
    }

    #[tokio::test]
    async fn markets_summary_returns_index_day_changes() {
        let (store, _d, t) = seeded().await; // AAPL equity (excluded from indices)
        let idx = store.upsert_index("^GSPC", "S&P 500").await.unwrap();
        let day1 = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        let day2 = NaiveDate::from_ymd_opt(2024, 2, 2).unwrap();
        store
            .save_prices(&[
                PricePoint { company_id: idx, date: day1, open: None, high: None, low: None, close: 4000.0, volume: None, source: "yahoo".into() },
                PricePoint { company_id: idx, date: day2, open: None, high: None, low: None, close: 4200.0, volume: None, source: "yahoo".into() },
            ])
            .await
            .unwrap();
        let (status, json) = get(store, &t, "/api/markets/summary").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap().len(), 1);
        assert_eq!(json[0]["company"]["ticker"], "^GSPC");
        assert_eq!(json[0]["last_close"], 4200.0);
        assert_eq!(json[0]["change"], 200.0);
        assert!((json[0]["change_pct"].as_f64().unwrap() - 0.05).abs() < 1e-9);
    }

    #[tokio::test]
    async fn watchlist_quotes_carry_day_change_and_tolerate_unpriced() {
        let (store, _d, t) = seeded().await; // AAPL: one price day (no change computable)
        store
            .insert_company(&NewCompany {
                cik: "2".into(),
                ticker: "BARE".into(),
                name: "Bare Co".into(),
                exchange: None,
                sector: None,
                industry: None,
            })
            .await
            .unwrap();
        let moved = store
            .insert_company(&NewCompany {
                cik: "3".into(),
                ticker: "MOVE".into(),
                name: "Move Co".into(),
                exchange: None,
                sector: None,
                industry: None,
            })
            .await
            .unwrap();
        store
            .save_prices(&[
                PricePoint {
                    company_id: moved,
                    date: NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
                    open: None,
                    high: None,
                    low: None,
                    close: 50.0,
                    volume: None,
                    source: "yahoo".into(),
                },
                PricePoint {
                    company_id: moved,
                    date: NaiveDate::from_ymd_opt(2024, 2, 2).unwrap(),
                    open: None,
                    high: None,
                    low: None,
                    close: 55.0,
                    volume: Some(10),
                    source: "yahoo".into(),
                },
            ])
            .await
            .unwrap();
        for ticker in ["AAPL", "BARE", "MOVE"] {
            let body = serde_json::json!({ "ticker": ticker });
            let (status, _) = send(
                store.clone(),
                req("POST", "/api/watchlist", Some(&t), Some(body)),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
        }

        let (status, json) = get(store, &t, "/api/watchlist/quotes").await;
        assert_eq!(status, StatusCode::OK);
        let rows = json.as_array().unwrap();
        assert_eq!(rows.len(), 3); // ordered by ticker: AAPL, BARE, MOVE
        // AAPL has a last close but only one priced day -> no change fields
        assert_eq!(rows[0]["company"]["ticker"], "AAPL");
        assert_eq!(rows[0]["last_close"], 185.0);
        assert!(rows[0]["change_pct"].is_null());
        // BARE has no prices at all but still appears
        assert_eq!(rows[1]["company"]["ticker"], "BARE");
        assert!(rows[1]["last_close"].is_null());
        // MOVE: +10% day change
        assert_eq!(rows[2]["company"]["ticker"], "MOVE");
        assert_eq!(rows[2]["last_close"], 55.0);
        assert_eq!(rows[2]["change"], 5.0);
        assert!((rows[2]["change_pct"].as_f64().unwrap() - 0.1).abs() < 1e-9);
        assert_eq!(rows[2]["as_of"], "2024-02-02");
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
    async fn companies_per_column_filters() {
        let (store, _d, t) = seeded().await;
        store
            .insert_company(&NewCompany {
                cik: "0000000002".into(),
                ticker: "MSFT".into(),
                name: "Microsoft".into(),
                exchange: Some("NASDAQ".into()),
                sector: None,
                industry: Some("Software".into()),
            })
            .await
            .unwrap();

        // ?ticker= narrows to a single company
        let (status, json) = get(store.clone(), &t, "/api/companies?ticker=MSFT").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["total"], 1);
        assert_eq!(json["rows"][0]["company"]["ticker"], "MSFT");

        // ?industry= narrows on the industry column
        let (_s, json) = get(store.clone(), &t, "/api/companies?industry=Software").await;
        assert_eq!(json["total"], 1);
        assert_eq!(json["rows"][0]["company"]["ticker"], "MSFT");

        // ?name= narrows on the name column
        let (_s, json) = get(store.clone(), &t, "/api/companies?name=Apple").await;
        assert_eq!(json["total"], 1);
        assert_eq!(json["rows"][0]["company"]["ticker"], "AAPL");

        // non-matching filter returns an empty page
        let (_s, json) = get(store.clone(), &t, "/api/companies?industry=Nonexistent").await;
        assert_eq!(json["total"], 0);
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
        assert_eq!(json["display_name"], "");
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
    async fn company_status_override_and_delisted_visibility() {
        let (store, _d, t) = seeded().await; // AAPL active by default
        // default directory shows AAPL
        let (_s, json) = get(store.clone(), &t, "/api/companies?limit=10").await;
        assert_eq!(json["total"], 1);
        // mark delisted via the manual endpoint
        let (status, _) = send(
            store.clone(),
            req("PUT", "/api/companies/AAPL/status", Some(&t), Some(json!({"status":"delisted"}))),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        // now hidden by default, visible with include_delisted
        let (_s, json) = get(store.clone(), &t, "/api/companies?limit=10").await;
        assert_eq!(json["total"], 0);
        let (_s, json) = get(store.clone(), &t, "/api/companies?limit=10&include_delisted=true").await;
        assert_eq!(json["total"], 1);
        assert_eq!(json["rows"][0]["company"]["status"], "delisted");
        // bad status value -> 400
        let (status, _) = send(
            store.clone(),
            req("PUT", "/api/companies/AAPL/status", Some(&t), Some(json!({"status":"bogus"}))),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        // unknown ticker -> 404
        let (status, _) = send(
            store,
            req("PUT", "/api/companies/NOPE/status", Some(&t), Some(json!({"status":"active"}))),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn profile_and_password_can_be_edited() {
        let (store, _d, t) = seeded().await; // seeded user u@e.com / "pw"
        // edit profile: email + display name
        let (status, _) = send(
            store.clone(),
            req("PUT", "/auth/profile", Some(&t), Some(json!({"email":"u2@e.com","display_name":"Uma"}))),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_s, json) = get(store.clone(), &t, "/auth/me").await;
        assert_eq!(json["email"], "u2@e.com");
        assert_eq!(json["display_name"], "Uma");

        // duplicate email -> 409 (signup another, then collide)
        let (_s, _) = send(
            store.clone(),
            req("POST", "/auth/signup", None, Some(json!({"email":"taken@e.com","password":"pw"}))),
        )
        .await;
        let (status, _) = send(
            store.clone(),
            req("PUT", "/auth/profile", Some(&t), Some(json!({"email":"taken@e.com","display_name":"X"}))),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);

        // change password: wrong old password -> 401
        let (status, _) = send(
            store.clone(),
            req("PUT", "/auth/password", Some(&t), Some(json!({"old_password":"nope","new_password":"fresh"}))),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // correct old password -> 204, and the new password logs in
        let (status, _) = send(
            store.clone(),
            req("PUT", "/auth/password", Some(&t), Some(json!({"old_password":"pw","new_password":"fresh"}))),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = send(
            store.clone(),
            req("POST", "/auth/login", None, Some(json!({"email":"u2@e.com","password":"fresh"}))),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
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
    async fn watch_groups_endpoints() {
        let (store, _d, t) = seeded().await;
        // watch AAPL
        send(store.clone(), req("POST", "/api/watchlist", Some(&t), Some(json!({"ticker":"AAPL"})))).await;
        // create a group
        let (status, json) =
            send(store.clone(), req("POST", "/api/watchlist/groups", Some(&t), Some(json!({"name":"Tech"})))).await;
        assert_eq!(status, StatusCode::CREATED);
        let gid = json["id"].as_i64().unwrap();
        // duplicate name -> 409
        let (status, _) =
            send(store.clone(), req("POST", "/api/watchlist/groups", Some(&t), Some(json!({"name":"Tech"})))).await;
        assert_eq!(status, StatusCode::CONFLICT);
        // list groups
        let (_s, json) = get(store.clone(), &t, "/api/watchlist/groups").await;
        assert_eq!(json[0]["name"], "Tech");
        // tag AAPL into the group -> shows in quotes' group_ids
        let (status, _) = send(
            store.clone(),
            req("POST", "/api/watchlist/AAPL/groups", Some(&t), Some(json!({"group_id": gid}))),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_s, json) = get(store.clone(), &t, "/api/watchlist/quotes").await;
        assert_eq!(json[0]["group_ids"][0], gid);
        // rename
        let (status, _) = send(
            store.clone(),
            req("PUT", &format!("/api/watchlist/groups/{gid}"), Some(&t), Some(json!({"name":"Technology"}))),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        // untag
        let (status, _) = send(
            store.clone(),
            req("DELETE", &format!("/api/watchlist/AAPL/groups/{gid}"), Some(&t), None),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_s, json) = get(store.clone(), &t, "/api/watchlist/quotes").await;
        assert!(json[0]["group_ids"].as_array().unwrap().is_empty());
        // delete group
        let (status, _) =
            send(store.clone(), req("DELETE", &format!("/api/watchlist/groups/{gid}"), Some(&t), None)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_s, json) = get(store, &t, "/api/watchlist/groups").await;
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
