//! llm_settings ユースケース。設定 CRUD ＋ articles スライスが呼ぶ「実効値」解決。

use super::domain::{LlmSettingsPatch, LlmSettingsRow, LlmSettingsView};
use super::repository;
use crate::shared::error::AppResult;
use crate::shared::llm::{DEFAULT_SUMMARIZE_PROMPT, DEFAULT_TRANSLATE_PROMPT};
use crate::shared::state::AppState;

/// 保存 override 行に既定値（config モデル・組込みプロンプト）を添えて View 化。
fn to_view(state: &AppState, row: LlmSettingsRow) -> LlmSettingsView {
    LlmSettingsView {
        summarize_model: row.summarize_model,
        summarize_prompt: row.summarize_prompt,
        translate_model: row.translate_model,
        translate_prompt: row.translate_prompt,
        default_model: state.config.anthropic_model.clone(),
        default_summarize_prompt: DEFAULT_SUMMARIZE_PROMPT.to_string(),
        default_translate_prompt: DEFAULT_TRANSLATE_PROMPT.to_string(),
    }
}

pub async fn get_view(state: &AppState) -> AppResult<LlmSettingsView> {
    let row = repository::get(&state.db).await?;
    Ok(to_view(state, row))
}

pub async fn update(state: &AppState, patch: LlmSettingsPatch) -> AppResult<LlmSettingsView> {
    repository::upsert(&state.db, &patch).await?;
    let row = repository::get(&state.db).await?;
    Ok(to_view(state, row))
}

/// 要約の実効値: (モデル, プロンプト override)。モデルは override→無ければ config 既定。
/// プロンプト None はアダプタ側で組込み既定にフォールバックする。
pub async fn resolve_summarize(state: &AppState) -> AppResult<(String, Option<String>)> {
    let row = repository::get(&state.db).await?;
    let model = row
        .summarize_model
        .unwrap_or_else(|| state.config.anthropic_model.clone());
    Ok((model, row.summarize_prompt))
}

/// 翻訳の実効値。resolve_summarize と同型。
pub async fn resolve_translate(state: &AppState) -> AppResult<(String, Option<String>)> {
    let row = repository::get(&state.db).await?;
    let model = row
        .translate_model
        .unwrap_or_else(|| state.config.anthropic_model.clone());
    Ok((model, row.translate_prompt))
}
