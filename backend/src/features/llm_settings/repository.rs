//! llm_settings リポジトリ: singleton 行（id=1）の読み書き。
//! read_later_settings と同じ upsert 方式。

use sqlx::PgPool;

use super::domain::{LlmSettingsPatch, LlmSettingsRow};
use crate::shared::error::AppResult;

/// override 行を取得。行が無ければ全 None（＝すべて既定）の既定値を返す。
pub async fn get(pool: &PgPool) -> AppResult<LlmSettingsRow> {
    let row = sqlx::query_as::<_, LlmSettingsRow>(
        "SELECT summarize_model, summarize_prompt, translate_model, translate_prompt
         FROM llm_settings WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.unwrap_or_default())
}

/// override をまとめて upsert（singleton）。None はそのまま NULL 保存＝override 解除。
pub async fn upsert(pool: &PgPool, patch: &LlmSettingsPatch) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO llm_settings
             (id, summarize_model, summarize_prompt, translate_model, translate_prompt, updated_at)
           VALUES (1, $1, $2, $3, $4, now())
           ON CONFLICT (id) DO UPDATE SET
             summarize_model  = EXCLUDED.summarize_model,
             summarize_prompt = EXCLUDED.summarize_prompt,
             translate_model  = EXCLUDED.translate_model,
             translate_prompt = EXCLUDED.translate_prompt,
             updated_at = now()"#,
    )
    .bind(&patch.summarize_model)
    .bind(&patch.summarize_prompt)
    .bind(&patch.translate_model)
    .bind(&patch.translate_prompt)
    .execute(pool)
    .await?;
    Ok(())
}
