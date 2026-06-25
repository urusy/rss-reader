use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::shared::error::AppResult;
use crate::shared::state::AppState;

/// Liveness: process is up.
pub async fn liveness() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// Readiness: database is reachable.
pub async fn readiness(State(state): State<AppState>) -> AppResult<Json<Value>> {
    sqlx::query("SELECT 1").execute(&state.db).await?;
    Ok(Json(json!({ "status": "ok", "db": "up" })))
}
