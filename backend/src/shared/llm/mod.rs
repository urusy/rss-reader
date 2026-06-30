//! LLM port + Anthropic adapter.
//!
//! Architectural note: this is the one external boundary we abstract behind a
//! trait from day one, because (a) we will mock it in tests and (b) swapping
//! Claude for another provider is a realistic future change. Other boundaries
//! (feed parser, http client) are NOT abstracted until a concrete reason appears.

pub mod anthropic;

use async_trait::async_trait;

use crate::shared::error::AppResult;

/// What we ask an LLM to do for a single article. Kept provider-agnostic.
#[derive(Debug, Clone)]
pub struct SummarizeRequest {
    pub title: String,
    pub content: String,
    /// Target language for the summary, e.g. "ja".
    pub target_lang: String,
}

#[derive(Debug, Clone)]
pub struct TranslateRequest {
    pub content: String,
    pub target_lang: String,
}

/// One conversation turn. role is "user" | "assistant" (system is separate).
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// A multi-turn chat request. system is passed out-of-band (Anthropic shape).
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub system: String,
    pub messages: Vec<ChatMessage>,
    /// Max response tokens; None → implementation default.
    pub max_tokens: Option<u32>,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn summarize(&self, req: SummarizeRequest) -> AppResult<String>;
    async fn translate(&self, req: TranslateRequest) -> AppResult<String>;
    /// Multi-turn chat. messages start with user and end with user (caller-validated).
    async fn chat(&self, req: ChatRequest) -> AppResult<String>;
}
