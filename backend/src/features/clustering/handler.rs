use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{Cluster, ClusterWithMembers};
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<ClusterWithMembers>>> {
    Ok(Json(service::list_clusters(&state).await?))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ClusterWithMembers>> {
    Ok(Json(service::get_cluster(&state, id).await?))
}

#[derive(Debug, Deserialize, Default)]
pub struct SummaryBody {
    pub target_lang: Option<String>,
}

pub async fn summarize(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Option<Json<SummaryBody>>,
) -> AppResult<Json<Cluster>> {
    let lang = body
        .and_then(|Json(b)| b.target_lang)
        .unwrap_or_else(|| state.config.cluster_summary_lang.clone());
    Ok(Json(service::summarize_cluster(&state, id, &lang).await?))
}

#[derive(serde::Serialize)]
pub struct ReclusterResult {
    pub clusters: usize,
}

pub async fn recluster(State(state): State<AppState>) -> AppResult<Json<ReclusterResult>> {
    let clusters = service::recluster(&state).await?;
    Ok(Json(ReclusterResult { clusters }))
}
