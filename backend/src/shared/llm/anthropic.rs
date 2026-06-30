//! Anthropic (Claude) adapter.
//!
//! There is no official Anthropic Rust SDK, so we call the Messages API directly
//! with reqwest. This keeps the dependency surface small and the behavior explicit.

use async_trait::async_trait;
use serde_json::json;

use super::{ChatMessage, ChatRequest, LlmClient, SummarizeRequest, TranslateRequest};
use crate::shared::error::{AppError, AppResult};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const CHAT_MAX_TOKENS: u32 = 2048;

#[derive(Clone)]
pub struct AnthropicClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

impl AnthropicClient {
    pub fn new(http: reqwest::Client, api_key: String, model: String) -> Self {
        Self {
            http,
            api_key,
            model,
        }
    }

    async fn complete(&self, system: &str, user: &str) -> AppResult<String> {
        let msgs = [ChatMessage {
            role: "user".into(),
            content: user.to_string(),
        }];
        self.complete_messages(system, &msgs, 1024).await
    }

    /// Multi-turn completion. `complete` delegates here with a single user turn.
    async fn complete_messages(
        &self,
        system: &str,
        messages: &[ChatMessage],
        max_tokens: u32,
    ) -> AppResult<String> {
        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect();
        let body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": msgs,
        });

        let resp = self
            .http
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Upstream(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Upstream(format!("anthropic {status}: {text}")));
        }

        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Upstream(e.to_string()))?;

        // Messages API returns content as an array of blocks; take the first text block.
        let text = value
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
            })
            .and_then(|b| b.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| AppError::Upstream("unexpected anthropic response shape".into()))?;

        Ok(text.to_string())
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn summarize(&self, req: SummarizeRequest) -> AppResult<String> {
        let system = format!(
            "You are a concise summarizer. Summarize the article in {} in 3-5 sentences. Output only the summary.",
            req.target_lang
        );
        let user = format!("Title: {}\n\n{}", req.title, req.content);
        self.complete(&system, &user).await
    }

    async fn translate(&self, req: TranslateRequest) -> AppResult<String> {
        let system = format!(
            "You are a translator. Translate the text into {}. Preserve meaning and tone. Output only the translation.",
            req.target_lang
        );
        self.complete(&system, &req.content).await
    }

    async fn chat(&self, req: ChatRequest) -> AppResult<String> {
        let max = req.max_tokens.unwrap_or(CHAT_MAX_TOKENS);
        self.complete_messages(&req.system, &req.messages, max)
            .await
    }
}
