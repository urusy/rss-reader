use super::domain::Stats;
use super::repository;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn get_stats(state: &AppState) -> AppResult<Stats> {
    repository::fetch(&state.db).await
}
