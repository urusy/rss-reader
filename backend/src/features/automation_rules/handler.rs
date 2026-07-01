use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{Action, Conditions};
use super::service::{self, Rule};
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RuleBody {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub position: i32,
    pub conditions: Conditions,
    pub actions: Vec<Action>,
}
fn default_true() -> bool {
    true
}

pub async fn list(State(s): State<AppState>) -> AppResult<Json<Vec<Rule>>> {
    Ok(Json(service::list_rules(&s).await?))
}

pub async fn get_one(State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Json<Rule>> {
    Ok(Json(service::get_rule(&s, id).await?))
}

pub async fn create(
    State(s): State<AppState>,
    Json(b): Json<RuleBody>,
) -> AppResult<(StatusCode, Json<Rule>)> {
    let rule =
        service::create_rule(&s, b.name, b.enabled, b.position, b.conditions, b.actions).await?;
    Ok((StatusCode::CREATED, Json(rule)))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(b): Json<RuleBody>,
) -> AppResult<Json<Rule>> {
    Ok(Json(
        service::update_rule(
            &s,
            id,
            b.name,
            b.enabled,
            b.position,
            b.conditions,
            b.actions,
        )
        .await?,
    ))
}

pub async fn delete(State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    service::delete_rule(&s, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Serialize)]
pub struct TestResult {
    pub matched_count: usize,
    pub matched_ids: Vec<Uuid>,
}

pub async fn test(State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Json<TestResult>> {
    let ids = service::test_rule(&s, id, 200).await?;
    Ok(Json(TestResult {
        matched_count: ids.len(),
        matched_ids: ids,
    }))
}

#[derive(Debug, serde::Serialize)]
pub struct ApplyResult {
    pub processed: usize,
}

pub async fn apply(State(s): State<AppState>) -> AppResult<Json<ApplyResult>> {
    let n = service::apply_all(&s).await?;
    Ok(Json(ApplyResult { processed: n }))
}
