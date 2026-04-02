use axum::{http::StatusCode, response::IntoResponse, Json};
use std::path::PathBuf;

pub async fn handle_health() -> impl IntoResponse {
    let mut status = "ok";
    let mut code = StatusCode::OK;

    // Check Registry Mount
    let registry_path = std::env::var("REGISTRY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/data/registry"));

    if !registry_path.exists() {
        status = "degraded";
        code = StatusCode::SERVICE_UNAVAILABLE;
    }

    (
        code,
        Json(serde_json::json!({
            "status": status,
            "region": std::env::var("FLY_REGION").unwrap_or_else(|_| "local".to_string()),
            "registry_mounted": registry_path.exists()
        })),
    )
}
