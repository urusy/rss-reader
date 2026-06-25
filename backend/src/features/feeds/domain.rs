use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Newtype for a feed's primary key. Prevents mixing it up with ArticleId etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct FeedId(pub Uuid);

/// Validated feed URL value object. Construction is the only way to get one,
/// so an invalid URL can never reach the rest of the domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeedUrl(String);

impl FeedUrl {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into();
        let trimmed = s.trim();
        if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
            return Err("feed url must start with http:// or https://".to_string());
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Persisted feed entity (mirrors the `feeds` table).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Feed {
    pub id: FeedId,
    pub url: String,
    pub title: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}
