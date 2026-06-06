//! Binary entrypoint. Thin bootstrap only — all logic lives in the library.

use std::net::SocketAddr;
use std::sync::Arc;

use stonkscollect_backend::store::Store;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:///data/stonks.db".into());
    let store = Arc::new(Store::connect(&database_url).await.expect("open database"));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind listener");
    tracing::info!("listening on {addr}");

    axum::serve(listener, stonkscollect_backend::app(store))
        .await
        .expect("server error");
}
