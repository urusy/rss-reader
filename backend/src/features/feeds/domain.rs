use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::features::folders::domain::FolderId;

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
    pub folder_id: Option<FolderId>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_https_url() {
        let url = FeedUrl::parse("https://example.com/feed.xml").unwrap();
        assert_eq!(url.as_str(), "https://example.com/feed.xml");
    }

    #[test]
    fn parse_accepts_http_url() {
        let url = FeedUrl::parse("http://example.com/rss").unwrap();
        assert_eq!(url.as_str(), "http://example.com/rss");
    }

    #[test]
    fn parse_trims_surrounding_whitespace() {
        let url = FeedUrl::parse("  https://example.com/feed.xml  ").unwrap();
        assert_eq!(url.as_str(), "https://example.com/feed.xml");
    }

    #[test]
    fn parse_rejects_url_without_scheme() {
        assert!(FeedUrl::parse("example.com/feed.xml").is_err());
    }

    #[test]
    fn parse_rejects_non_http_scheme() {
        assert!(FeedUrl::parse("ftp://example.com/feed.xml").is_err());
    }

    #[test]
    fn parse_rejects_empty_input() {
        assert!(FeedUrl::parse("").is_err());
    }
}
