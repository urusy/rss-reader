use std::collections::HashSet;

use super::domain::{
    build_profile, parse_relevance_scores, profile_fingerprint, ProfileView, RelevanceScore,
    ScoreResult,
};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{LlmClient, ScorableArticle, ScoreRelevanceRequest};
use crate::shared::state::AppState;

const TOP_TAGS: i64 = 20;
const READ_TITLES: i64 = 30;
const MAX_BATCH: i64 = 40;

fn llm_client(state: &AppState) -> AppResult<AnthropicClient> {
    let key = state
        .config
        .anthropic_api_key
        .clone()
        .ok_or_else(|| AppError::NotEnabled("ANTHROPIC_API_KEY is not set".into()))?;
    Ok(AnthropicClient::new(
        state.http.clone(),
        key,
        state.config.anthropic_model.clone(),
    ))
}

async fn current_profile(state: &AppState) -> AppResult<(String, usize, usize)> {
    let tags = repository::top_tags(&state.db, TOP_TAGS).await?;
    let read_titles = repository::recent_read_titles(&state.db, READ_TITLES).await?;
    let profile = build_profile(&tags, &read_titles);
    Ok((profile, tags.len(), read_titles.len()))
}

pub async fn profile_view(state: &AppState) -> AppResult<ProfileView> {
    let (profile, tag_count, read_count) = current_profile(state).await?;
    let hash = profile_fingerprint(&profile);
    Ok(ProfileView {
        profile,
        hash,
        tag_count,
        read_count,
    })
}

pub async fn list_scores(state: &AppState) -> AppResult<Vec<RelevanceScore>> {
    repository::list_scores(&state.db).await
}

/// Score unread articles. Order: LLM gate → profile → candidates → diff → call →
/// save → return all cached scores.
pub async fn score_unread(state: &AppState, refresh: bool) -> AppResult<ScoreResult> {
    let client = llm_client(state)?;

    let (profile, _tc, _rc) = current_profile(state).await?;
    let profile_hash = profile_fingerprint(&profile);

    let candidates = repository::unread_candidates(&state.db, MAX_BATCH).await?;

    let fresh: HashSet<_> = if refresh {
        HashSet::new()
    } else {
        repository::fresh_scored_ids(&state.db, &profile_hash).await?
    };
    let to_score: Vec<_> = candidates
        .into_iter()
        .filter(|c| !fresh.contains(&c.id))
        .collect();

    let mut scored_count = 0usize;
    if !to_score.is_empty() {
        let valid_ids: HashSet<_> = to_score.iter().map(|c| c.id).collect();
        let articles: Vec<ScorableArticle> = to_score
            .iter()
            .map(|c| ScorableArticle {
                id: c.id.to_string(),
                title: c.title.clone(),
                snippet: c.snippet.clone(),
            })
            .collect();

        let raw = client
            .score_relevance(ScoreRelevanceRequest { profile, articles })
            .await?;

        let parsed = parse_relevance_scores(&raw, &valid_ids)
            .map_err(|e| AppError::Upstream(format!("could not parse LLM score output: {e}")))?;

        repository::save_scores(
            &state.db,
            &parsed,
            &profile_hash,
            &state.config.anthropic_model,
        )
        .await?;
        scored_count = parsed.len();
    }

    let scores = repository::list_scores(&state.db).await?;
    Ok(ScoreResult {
        scored_count,
        profile_hash,
        scores,
    })
}
