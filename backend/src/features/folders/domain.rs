use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// フォルダ主キーの newtype（FeedId / ArticleId と取り違えない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct FolderId(pub Uuid);

/// 検証済みフォルダ名の値オブジェクト。空白のみ・長すぎる名前を構築時に弾く。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderName(String);

impl FolderName {
    pub const MAX_CHARS: usize = 100;

    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let trimmed = raw.into().trim().to_string();
        if trimmed.is_empty() {
            return Err("folder name must not be empty".to_string());
        }
        if trimmed.chars().count() > Self::MAX_CHARS {
            return Err(format!(
                "folder name must be at most {} chars",
                Self::MAX_CHARS
            ));
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 永続化されたフォルダ（folders テーブルをミラー）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    pub position: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_normal_name() {
        let n = FolderName::parse("Tech").unwrap();
        assert_eq!(n.as_str(), "Tech");
    }

    #[test]
    fn parse_trims_whitespace() {
        let n = FolderName::parse("  Tech  ").unwrap();
        assert_eq!(n.as_str(), "Tech");
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(FolderName::parse("").is_err());
    }

    #[test]
    fn parse_rejects_whitespace_only() {
        assert!(FolderName::parse("   ").is_err());
    }

    #[test]
    fn parse_accepts_boundary_100_and_rejects_101() {
        let ok = "a".repeat(FolderName::MAX_CHARS);
        assert!(FolderName::parse(ok).is_ok());
        let too_long = "a".repeat(FolderName::MAX_CHARS + 1);
        assert!(FolderName::parse(too_long).is_err());
    }
}
