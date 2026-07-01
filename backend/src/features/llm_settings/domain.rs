//! llm_settings ドメイン: 要約/翻訳のモデル・プロンプト override。純粋な検証/正規化。
//!
//! override は「無し(None) = 既定にフォールバック」を意味する。空文字は None に
//! 畳む（＝設定画面で欄を空にすると既定へ戻る）。

use serde::{Deserialize, Serialize};

const MODEL_MAX: usize = 100;
const PROMPT_MAX: usize = 4000;

/// 保存されている override 行（singleton）。NULL 列 = override 無し。
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct LlmSettingsRow {
    pub summarize_model: Option<String>,
    pub summarize_prompt: Option<String>,
    pub translate_model: Option<String>,
    pub translate_prompt: Option<String>,
}

/// GET 応答: 保存された override ＋ UI がプレースホルダ/「既定に戻す」で使う既定値。
#[derive(Debug, Clone, Serialize)]
pub struct LlmSettingsView {
    pub summarize_model: Option<String>,
    pub summarize_prompt: Option<String>,
    pub translate_model: Option<String>,
    pub translate_prompt: Option<String>,
    pub default_model: String,
    pub default_summarize_prompt: String,
    pub default_translate_prompt: String,
}

/// PUT の受信ボディ（未検証の生入力）。欠けたフィールドは None。
#[derive(Debug, Clone, Deserialize)]
pub struct LlmSettingsBody {
    #[serde(default)]
    pub summarize_model: Option<String>,
    #[serde(default)]
    pub summarize_prompt: Option<String>,
    #[serde(default)]
    pub translate_model: Option<String>,
    #[serde(default)]
    pub translate_prompt: Option<String>,
}

/// 検証済みパッチ。空文字は None に正規化（override 解除）。
#[derive(Debug, Clone)]
pub struct LlmSettingsPatch {
    pub summarize_model: Option<String>,
    pub summarize_prompt: Option<String>,
    pub translate_model: Option<String>,
    pub translate_prompt: Option<String>,
}

impl LlmSettingsPatch {
    pub fn parse(body: LlmSettingsBody) -> Result<Self, String> {
        Ok(Self {
            summarize_model: clean_model(body.summarize_model)?,
            summarize_prompt: clean_prompt(body.summarize_prompt)?,
            translate_model: clean_model(body.translate_model)?,
            translate_prompt: clean_prompt(body.translate_prompt)?,
        })
    }
}

/// モデル id: trim → 空なら None、そうでなければ文字種と長さを検証。
/// 将来のモデル id も通せるよう、許可リストではなく形式（英数と `. _ -`）で弾く。
fn clean_model(s: Option<String>) -> Result<Option<String>, String> {
    match s.map(|v| v.trim().to_string()) {
        None => Ok(None),
        Some(v) if v.is_empty() => Ok(None),
        Some(v) => {
            if v.chars().count() > MODEL_MAX {
                return Err(format!("model id too long (max {MODEL_MAX} chars)"));
            }
            if !v
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            {
                return Err("model id has invalid characters (allowed: A-Z a-z 0-9 . _ -)".into());
            }
            Ok(Some(v))
        }
    }
}

/// プロンプト: trim → 空なら None（既定使用）、長すぎは弾く。
fn clean_prompt(s: Option<String>) -> Result<Option<String>, String> {
    match s.map(|v| v.trim().to_string()) {
        None => Ok(None),
        Some(v) if v.is_empty() => Ok(None),
        Some(v) => {
            if v.chars().count() > PROMPT_MAX {
                return Err(format!("prompt too long (max {PROMPT_MAX} chars)"));
            }
            Ok(Some(v))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(
        sm: Option<&str>,
        sp: Option<&str>,
        tm: Option<&str>,
        tp: Option<&str>,
    ) -> LlmSettingsBody {
        LlmSettingsBody {
            summarize_model: sm.map(String::from),
            summarize_prompt: sp.map(String::from),
            translate_model: tm.map(String::from),
            translate_prompt: tp.map(String::from),
        }
    }

    #[test]
    fn parses_valid_overrides() {
        let p = LlmSettingsPatch::parse(body(
            Some("claude-opus-4-8"),
            Some("  summarize in {lang}  "),
            Some("claude-haiku-4-5-20251001"),
            None,
        ))
        .unwrap();
        assert_eq!(p.summarize_model.as_deref(), Some("claude-opus-4-8"));
        // trim される
        assert_eq!(p.summarize_prompt.as_deref(), Some("summarize in {lang}"));
        assert_eq!(
            p.translate_model.as_deref(),
            Some("claude-haiku-4-5-20251001")
        );
        assert_eq!(p.translate_prompt, None);
    }

    #[test]
    fn empty_or_blank_clears_override() {
        let p = LlmSettingsPatch::parse(body(Some(""), Some("   "), None, None)).unwrap();
        assert_eq!(p.summarize_model, None);
        assert_eq!(p.summarize_prompt, None);
    }

    #[test]
    fn rejects_invalid_model_chars() {
        let err =
            LlmSettingsPatch::parse(body(Some("claude opus/4.8"), None, None, None)).unwrap_err();
        assert!(err.contains("invalid characters"), "got: {err}");
    }

    #[test]
    fn rejects_too_long_prompt() {
        let long = "x".repeat(PROMPT_MAX + 1);
        let err = LlmSettingsPatch::parse(body(None, Some(&long), None, None)).unwrap_err();
        assert!(err.contains("too long"), "got: {err}");
    }
}
