//! Anthropic (Claude) adapter.
//!
//! There is no official Anthropic Rust SDK, so we call the Messages API directly
//! with reqwest. This keeps the dependency surface small and the behavior explicit.

use async_trait::async_trait;
use serde_json::json;

use super::{
    ChatMessage, ChatRequest, ClusterSummaryRequest, DigestRequest, LlmClient,
    ScoreRelevanceRequest, SuggestTagsRequest, SummarizeRequest, TranslateRequest,
};
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

    async fn complete(&self, purpose: &'static str, system: &str, user: &str) -> AppResult<String> {
        let msgs = [ChatMessage {
            role: "user".into(),
            content: user.to_string(),
        }];
        self.complete_messages(purpose, system, &msgs, 1024).await
    }

    /// Multi-turn completion. `complete` delegates here with a single user turn.
    ///
    /// `purpose` は利用状況記録（llm_usage_events）用のラベル。全 trait メソッドが
    /// ここに合流するため、この1箇所で実呼び出しの model + トークン数を捕捉できる
    /// （scheduler 起動の背景 digest/relevance/clustering も漏れない）。
    async fn complete_messages(
        &self,
        purpose: &'static str,
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

        // 成功した実呼び出しだけ記録（キャッシュヒットはここに来ない）。
        // record は非ブロッキング・失敗巻き込みなし（sink 未 install なら no-op）。
        let input_tokens = value
            .pointer("/usage/input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let output_tokens = value
            .pointer("/usage/output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        crate::shared::usage::record(crate::shared::usage::UsageEvent::Llm {
            purpose,
            model: self.model.clone(),
            input_tokens,
            output_tokens,
        });

        Ok(text.to_string())
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn summarize(&self, req: SummarizeRequest) -> AppResult<String> {
        let tmpl = req
            .system_prompt
            .as_deref()
            .unwrap_or(super::DEFAULT_SUMMARIZE_PROMPT);
        let system = tmpl.replace("{lang}", &req.target_lang);
        let user = format!("Title: {}\n\n{}", req.title, req.content);
        self.complete("summarize", &system, &user).await
    }

    async fn translate(&self, req: TranslateRequest) -> AppResult<String> {
        let tmpl = req
            .system_prompt
            .as_deref()
            .unwrap_or(super::DEFAULT_TRANSLATE_PROMPT);
        let system = tmpl.replace("{lang}", &req.target_lang);
        self.complete("translate", &system, &req.content).await
    }

    async fn chat(&self, req: ChatRequest) -> AppResult<String> {
        let max = req.max_tokens.unwrap_or(CHAT_MAX_TOKENS);
        self.complete_messages("chat", &req.system, &req.messages, max)
            .await
    }

    async fn suggest_tags(&self, req: SuggestTagsRequest) -> AppResult<String> {
        let vocab = if req.vocabulary.is_empty() {
            "(none yet)".to_string()
        } else {
            req.vocabulary.join(", ")
        };
        let system = format!(
            "You are a tagging assistant for a personal RSS reader. \
             Classify the article using a CONSISTENT personal vocabulary. \
             PREFER reusing tags from this existing vocabulary: [{vocab}]. \
             Only invent a new tag when none of the existing ones fit. \
             Return AT MOST {max} tags. \
             Respond with ONLY a JSON array, no prose, no code fences, like: \
             [{{\"name\":\"rust\",\"confidence\":0.9}}]. \
             Tag names should be short, lowercase nouns.",
            vocab = vocab,
            max = req.max_tags,
        );
        let user = format!("Title: {}\n\n{}", req.title, req.content);
        self.complete("suggest_tags", &system, &user).await
    }

    async fn digest(&self, req: DigestRequest) -> AppResult<String> {
        let system = format!(
            "You are an editor compiling a daily news digest in {}. Group the \
             following articles by topic. For each topic, write a short heading \
             (Markdown '## ') and 2-4 concise bullet points capturing the key \
             points. Keep each article's source link. Output Markdown only.",
            req.target_lang
        );
        self.complete("digest", &system, &req.items).await
    }

    async fn score_relevance(&self, req: ScoreRelevanceRequest) -> AppResult<String> {
        let system = format!(
            "You score how relevant unread articles are to a user's interest \
             profile, so the most worth-reading ones can be surfaced first. \
             The user's interest profile is:\n{}\n\n\
             For EACH article below, output an integer relevance score from 0 \
             (irrelevant) to 100 (highly relevant) and a very short reason. \
             Respond with ONLY a JSON array, no prose, no code fences, like: \
             [{{\"id\":\"<uuid>\",\"score\":80,\"reason\":\"matches rust interest\"}}]. \
             Use the exact id given for each article.",
            req.profile
        );
        let user = req
            .articles
            .iter()
            .map(|a| {
                let snippet = a.snippet.trim();
                let snippet = if snippet.chars().count() > 400 {
                    snippet.chars().take(400).collect::<String>()
                } else {
                    snippet.to_string()
                };
                format!(
                    "id: {}\ntitle: {}\nexcerpt: {}",
                    a.id,
                    a.title.trim(),
                    snippet
                )
            })
            .collect::<Vec<_>>()
            .join("\n---\n");
        self.complete("score_relevance", &system, &user).await
    }

    async fn cluster_summary(&self, req: ClusterSummaryRequest) -> AppResult<String> {
        let system = format!(
            "You are a news analyst. The following articles from different outlets \
             cover the SAME story. Write an integrated summary in {} that (1) states \
             the shared facts in 2-3 sentences, then (2) explicitly contrasts how the \
             outlets differ in framing, emphasis, or tone (use a short bulleted list, \
             naming each outlet). Be concise and neutral. Output only the summary.",
            req.target_lang
        );
        self.complete("cluster_summary", &system, &req.items).await
    }
}
