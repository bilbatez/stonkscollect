//! StonksCollect backend library.

pub mod api;
pub mod auth;
pub mod collectors;
pub mod config;
pub mod domain;
pub mod graham;
pub mod http;
pub mod net;
pub mod pipeline;
pub mod ratios;
pub mod reconcile;
pub mod scheduler;
pub mod store;

#[cfg(test)]
mod testutil;

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::store::Store;

/// Reject request bodies larger than this (all our payloads are tiny JSON).
const MAX_BODY_BYTES: usize = 64 * 1024;
/// Abort a request that runs longer than this, freeing the worker.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Build the application router with all routes wired up.
pub fn app(store: Arc<Store>) -> Router {
    Router::new()
        .route("/health", get(health))
        .merge(api::routes())
        // Innermost first: cap body size, bound request time, log everything.
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, REQUEST_TIMEOUT))
        .layer(TraceLayer::new_for_http())
        .with_state(store)
}

/// Liveness probe handler.
async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
