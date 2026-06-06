//! StonksCollect backend library.

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

/// Build the application router with all routes wired up.
pub fn app() -> Router {
    Router::new().route("/health", get(health))
}

/// Liveness probe handler.
async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
