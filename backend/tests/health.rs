use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use stonkscollect_backend::app;
use tower::ServiceExt;

#[tokio::test]
async fn get_health_returns_200_with_status_ok() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["status"], "ok");
}
