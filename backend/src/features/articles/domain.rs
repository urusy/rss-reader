use serde::Serialize;
use uuid::Uuid;

use crate::features::feeds::domain::FeedId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct ArticleId(pub Uuid);

/// Persisted article. Summary/translation columns are the on-demand LLM cache:
/// null until the user requests processing, then filled and reused.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Article {
    pub id: ArticleId,
    pub feed_id: FeedId,
    pub url: String,
    pub title: String,
    pub content: String,
    /// Full body extracted on demand from the source URL (DOM heuristic +
    /// sanitize). NULL until extraction succeeds; AI features prefer this over
    /// `content` when present. See features/extraction.
    pub full_content: Option<String>,
    pub extracted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_read: bool,
    pub summary: Option<String>,
    pub summary_lang: Option<String>,
    pub translation: Option<String>,
    pub translation_lang: Option<String>,
    pub processed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// hide ミュート合致のスタンプ。NULL=表示。#19 mute_rules
    pub muted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
