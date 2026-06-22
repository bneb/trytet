use axum::extract::State;
use axum::{http::StatusCode, response::IntoResponse, Json};
use std::sync::Arc;

use crate::api::AppState;

pub async fn handle_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let config = &state.config;

    let mut status = "ok";
    let mut code = StatusCode::OK;

    let registry_path = &config.registry_path;
    if !registry_path.exists() {
        status = "degraded";
        code = StatusCode::SERVICE_UNAVAILABLE;
    }

    (
        code,
        Json(serde_json::json!({
            "status": status,
            "region": config.fly_region,
            "registry_mounted": registry_path.exists()
        })),
    )
}
