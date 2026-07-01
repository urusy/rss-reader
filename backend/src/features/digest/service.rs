use chrono::{NaiveDate, Utc};

use super::domain::{build_digest_input, Digest, EMPTY_DIGEST_MD};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{DigestRequest, LlmClient};
use crate::shared::state::AppState;

const WINDOW_HOURS: i32 = 24;

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

pub async fn get_latest(state: &AppState) -> AppResult<Digest> {
    repository::get_latest(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn get_by_date(state: &AppState, date: NaiveDate) -> AppResult<Digest> {
    repository::get_by_date(&state.db, date)
        .await?
        .ok_or(AppError::NotFound)
}

/// Generate and store the digest for a date (overwrite = idempotent). Order:
/// LLM gate → material → empty-note (no call) or Claude → save → optional email.
pub async fn generate_for_date(state: &AppState, date: NaiveDate) -> AppResult<Digest> {
    let client = llm_client(state)?; // gate first (503 if no key)

    let sources = repository::recent_unread(&state.db, WINDOW_HOURS).await?;
    let count = sources.len() as i32;

    let (markdown, model) = if sources.is_empty() {
        (EMPTY_DIGEST_MD.to_string(), "(none)".to_string())
    } else {
        let items = build_digest_input(&sources);
        let md = client
            .digest(DigestRequest {
                items,
                target_lang: state.config.digest_lang.clone(),
            })
            .await?;
        (md, state.config.anthropic_model.clone())
    };

    repository::upsert(&state.db, date, &markdown, &model, count).await?;

    if count > 0 {
        if let Err(e) = super::email::maybe_send(state, date, &markdown).await {
            tracing::warn!(error = %e, "digest email send failed (non-fatal)");
        }
    }

    repository::get_by_date(&state.db, date)
        .await?
        .ok_or(AppError::NotFound)
}

/// Scheduler helper: generate today's digest only if it doesn't exist yet.
pub async fn ensure_today(state: &AppState) -> AppResult<()> {
    let today = Utc::now().date_naive();
    if repository::get_by_date(&state.db, today).await?.is_some() {
        return Ok(());
    }
    generate_for_date(state, today).await.map(|_| ())
}
