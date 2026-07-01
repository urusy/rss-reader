//! LLM port + Anthropic adapter.
//!
//! Architectural note: this is the one external boundary we abstract behind a
//! trait from day one, because (a) we will mock it in tests and (b) swapping
//! Claude for another provider is a realistic future change. Other boundaries
//! (feed parser, http client) are NOT abstracted until a concrete reason appears.

pub mod anthropic;

use async_trait::async_trait;

use crate::shared::error::AppResult;

/// Built-in system-prompt templates for summarize/translate. `{lang}` is replaced
/// with the target language at call time. Kept here (not in the adapter) so the
/// `llm_settings` slice can surface them as the "reset to default" text in the UI.
pub const DEFAULT_SUMMARIZE_PROMPT: &str =
    "You are a concise summarizer. Summarize the article in {lang} in 3-5 sentences. Output only the summary.";
pub const DEFAULT_TRANSLATE_PROMPT: &str =
    "You are a translator. Translate the text into {lang}. Preserve meaning and tone. Output only the translation.";

/// What we ask an LLM to do for a single article. Kept provider-agnostic.
#[derive(Debug, Clone)]
pub struct SummarizeRequest {
    pub title: String,
    pub content: String,
    /// Target language for the summary, e.g. "ja".
    pub target_lang: String,
    /// Optional system-prompt override (raw template, may contain `{lang}`).
    /// None → the adapter uses DEFAULT_SUMMARIZE_PROMPT.
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TranslateRequest {
    pub content: String,
    pub target_lang: String,
    /// Optional system-prompt override (raw template, may contain `{lang}`).
    /// None → the adapter uses DEFAULT_TRANSLATE_PROMPT.
    pub system_prompt: Option<String>,
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

/// Ask the LLM to classify an article into tags, reusing the existing vocabulary.
#[derive(Debug, Clone)]
pub struct SuggestTagsRequest {
    pub title: String,
    pub content: String,
    pub vocabulary: Vec<String>,
    pub max_tags: usize,
}

/// Ask the LLM to compile a topic-grouped daily digest (Markdown).
#[derive(Debug, Clone)]
pub struct DigestRequest {
    /// Article list as a Markdown bullet list (built by build_digest_input).
    pub items: String,
    pub target_lang: String,
}

/// One article to score (provider-agnostic). id is the article UUID string.
#[derive(Debug, Clone)]
pub struct ScorableArticle {
    pub id: String,
    pub title: String,
    pub snippet: String,
}

/// Ask the LLM to score unread articles against an interest profile.
#[derive(Debug, Clone)]
pub struct ScoreRelevanceRequest {
    pub profile: String,
    pub articles: Vec<ScorableArticle>,
}

/// Ask the LLM for an integrated cross-outlet summary of one cluster.
#[derive(Debug, Clone)]
pub struct ClusterSummaryRequest {
    /// Per-outlet article list (built by build_cluster_summary_input).
    pub items: String,
    pub target_lang: String,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn summarize(&self, req: SummarizeRequest) -> AppResult<String>;
    async fn translate(&self, req: TranslateRequest) -> AppResult<String>;
    /// Multi-turn chat. messages start with user and end with user (caller-validated).
    async fn chat(&self, req: ChatRequest) -> AppResult<String>;
    /// Tag suggestion. Returns a JSON array string (caller parses it).
    async fn suggest_tags(&self, req: SuggestTagsRequest) -> AppResult<String>;
    /// Daily digest: topic-grouped Markdown from a list of articles.
    async fn digest(&self, req: DigestRequest) -> AppResult<String>;
    /// Relevance scoring. Returns a JSON array string (caller parses it).
    async fn score_relevance(&self, req: ScoreRelevanceRequest) -> AppResult<String>;
    /// Integrated cross-outlet summary of a cluster (plain text).
    async fn cluster_summary(&self, req: ClusterSummaryRequest) -> AppResult<String>;
}
