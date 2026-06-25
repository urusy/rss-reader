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

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn summarize(&self, req: SummarizeRequest) -> AppResult<String>;
    async fn translate(&self, req: TranslateRequest) -> AppResult<String>;
}
