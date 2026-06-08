//! StonksCollect backend library.

pub mod api;
pub mod auth;
pub mod collectors;
pub mod config;
pub mod domain;
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

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

use crate::store::Store;

/// Build the application router with all routes wired up.
pub fn app(store: Arc<Store>) -> Router {
    Router::new()
        .route("/health", get(health))
        .merge(api::routes())
        .with_state(store)
}

/// Liveness probe handler.
async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
