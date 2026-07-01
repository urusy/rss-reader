use super::domain::{
    parse_tag_suggestions, ArticleTag, RawSuggestion, Tag, TagId, TagName, TagWithCount,
};
use super::repository;
use crate::features::articles::domain::ArticleId;
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{LlmClient, SuggestTagsRequest};
use crate::shared::state::AppState;

const MAX_SUGGESTIONS: usize = 6;

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

pub async fn list_tags(state: &AppState) -> AppResult<Vec<TagWithCount>> {
    repository::list_tags(&state.db).await
}

pub async fn create_tag(state: &AppState, name: TagName, color: Option<String>) -> AppResult<Tag> {
    repository::upsert_tag(&state.db, name.as_str(), color.as_deref(), "user").await
}

pub async fn update_tag(
    state: &AppState,
    id: TagId,
    name: TagName,
    color: Option<String>,
) -> AppResult<Tag> {
    repository::update_tag(&state.db, id, name.as_str(), color.as_deref()).await
}

pub async fn delete_tag(state: &AppState, id: TagId) -> AppResult<()> {
    if repository::delete_tag(&state.db, id).await? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn list_article_tags(
    state: &AppState,
    article_id: ArticleId,
) -> AppResult<Vec<ArticleTag>> {
    repository::list_article_tags(&state.db, article_id).await
}

pub async fn set_article_tags(
    state: &AppState,
    article_id: ArticleId,
    tag_ids: &[TagId],
) -> AppResult<Vec<ArticleTag>> {
    repository::set_article_tags(&state.db, article_id, tag_ids).await?;
    repository::list_article_tags(&state.db, article_id).await
}

pub async fn detach_tag(state: &AppState, article_id: ArticleId, tag_id: TagId) -> AppResult<()> {
    if repository::detach_tag(&state.db, article_id, tag_id).await? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// AI tag suggestion (cached). Order: cache → article exists → LLM gate → call.
pub async fn suggest_tags(
    state: &AppState,
    article_id: ArticleId,
    refresh: bool,
) -> AppResult<Vec<RawSuggestion>> {
    if !refresh {
        if let Some(cached) = repository::get_cached_suggestions(&state.db, article_id).await? {
            return Ok(cached);
        }
    }

    let article = repository::get_article_text(&state.db, article_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let client = llm_client(state)?;
    let vocabulary = repository::vocabulary(&state.db).await?;

    let raw = client
        .suggest_tags(SuggestTagsRequest {
            title: article.title,
            content: article.content,
            vocabulary,
            max_tags: MAX_SUGGESTIONS,
        })
        .await?;

    let suggestions = parse_tag_suggestions(&raw, MAX_SUGGESTIONS)
        .map_err(|e| AppError::Upstream(format!("could not parse LLM tag output: {e}")))?;

    repository::save_suggestions(
        &state.db,
        article_id,
        &suggestions,
        &state.config.anthropic_model,
    )
    .await?;
    Ok(suggestions)
}
