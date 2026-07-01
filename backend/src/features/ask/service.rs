use uuid::Uuid;

use super::domain::{build_system_multi, build_system_single, validate_conversation, AskMessage};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{ChatMessage, ChatRequest, LlmClient};
use crate::shared::state::AppState;

/// Same NotEnabled gate as articles/service (duplicated, not imported, to keep
/// the slice independent — it's a 3-line helper).
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

fn to_chat_messages(messages: &[AskMessage]) -> Vec<ChatMessage> {
    messages
        .iter()
        .map(|m| ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect()
}

/// Single-article Ask. Order: LLM gate → validate → article exists → chat →
/// optional save (last user question + assistant answer).
pub async fn ask_article(
    state: &AppState,
    article_id: Uuid,
    messages: Vec<AskMessage>,
    save: bool,
) -> AppResult<String> {
    let client = llm_client(state)?;
    validate_conversation(&messages).map_err(AppError::Validation)?;

    let ctx = repository::get_article_context(&state.db, article_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let system = build_system_single(&ctx);
    let answer = client
        .chat(ChatRequest {
            system,
            messages: to_chat_messages(&messages),
            max_tokens: None,
        })
        .await?;

    if save {
        let mut to_save = Vec::new();
        if let Some(u) = messages.last().cloned() {
            to_save.push(u);
        }
        to_save.push(AskMessage {
            role: "assistant".into(),
            content: answer.clone(),
        });
        repository::save_notes(&state.db, article_id, &to_save).await?;
    }

    Ok(answer)
}

/// Cross-article Ask. NotFound if no requested article exists.
pub async fn ask_articles(
    state: &AppState,
    ids: Vec<Uuid>,
    messages: Vec<AskMessage>,
) -> AppResult<String> {
    let client = llm_client(state)?;
    if ids.is_empty() {
        return Err(AppError::Validation("ids must not be empty".into()));
    }
    validate_conversation(&messages).map_err(AppError::Validation)?;

    let ctxs = repository::get_article_contexts(&state.db, &ids).await?;
    if ctxs.is_empty() {
        return Err(AppError::NotFound);
    }

    let system = build_system_multi(&ctxs);
    client
        .chat(ChatRequest {
            system,
            messages: to_chat_messages(&messages),
            max_tokens: None,
        })
        .await
}

pub async fn get_notes(state: &AppState, article_id: Uuid) -> AppResult<Vec<AskMessage>> {
    repository::list_notes(&state.db, article_id).await
}
