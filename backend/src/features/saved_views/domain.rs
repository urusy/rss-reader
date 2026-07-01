use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct SavedViewId(pub Uuid);

const MAX_TEXT_LEN: usize = 200;
const MAX_NAME_LEN: usize = 80;

/// Saved filter criteria; each field optional (None = no filter on that axis).
/// Resolved as an AND of all fields. Stored as JSONB.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuerySpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<Uuid>,
    #[serde(default)]
    pub unclassified: bool,
    #[serde(default)]
    pub unread_only: bool,
    /// Articles having ANY of these tags (#24). Empty = ignored.
    #[serde(default)]
    pub tag_ids: Vec<Uuid>,
}

impl QuerySpec {
    pub fn is_empty(&self) -> bool {
        self.text.as_deref().map(str::trim).unwrap_or("").is_empty()
            && self.feed_id.is_none()
            && self.folder_id.is_none()
            && !self.unclassified
            && !self.unread_only
            && self.tag_ids.is_empty()
    }

    /// Validate + normalize. Requires at least one axis; folds blank text to None.
    pub fn validate(mut self) -> AppResult<Self> {
        self.text = match self.text.take() {
            Some(t) => {
                let t = t.trim().to_string();
                if t.is_empty() {
                    None
                } else if t.chars().count() > MAX_TEXT_LEN {
                    return Err(AppError::Validation(format!(
                        "search text must be at most {MAX_TEXT_LEN} characters"
                    )));
                } else {
                    Some(t)
                }
            }
            None => None,
        };
        if self.is_empty() {
            return Err(AppError::Validation(
                "a smart view must have at least one filter".into(),
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedViewName(String);

impl SavedViewName {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let n = raw.into().split_whitespace().collect::<Vec<_>>().join(" ");
        if n.is_empty() {
            return Err("view name must not be empty".into());
        }
        if n.chars().count() > MAX_NAME_LEN {
            return Err(format!(
                "view name must be at most {MAX_NAME_LEN} characters"
            ));
        }
        Ok(Self(n))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedView {
    pub id: SavedViewId,
    pub name: String,
    pub query: QuerySpec,
    pub position: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SavedViewRow {
    pub id: SavedViewId,
    pub name: String,
    pub query: sqlx::types::Json<QuerySpec>,
    pub position: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<SavedViewRow> for SavedView {
    fn from(r: SavedViewRow) -> Self {
        SavedView {
            id: r.id,
            name: r.name,
            query: r.query.0,
            position: r.position,
            created_at: r.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_spec_is_empty() {
        assert!(QuerySpec::default().is_empty());
    }

    #[test]
    fn text_only_is_not_empty() {
        let s = QuerySpec {
            text: Some("rust".into()),
            ..Default::default()
        };
        assert!(!s.is_empty());
    }

    #[test]
    fn whitespace_text_counts_as_empty() {
        let s = QuerySpec {
            text: Some("   ".into()),
            ..Default::default()
        };
        assert!(s.is_empty());
    }

    #[test]
    fn validate_rejects_all_empty() {
        assert!(QuerySpec::default().validate().is_err());
    }

    #[test]
    fn validate_trims_and_folds_blank_text_to_none() {
        let s = QuerySpec {
            text: Some("  rust async ".into()),
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(s.text.as_deref(), Some("rust async"));
    }

    #[test]
    fn validate_keeps_other_axes_when_text_blank() {
        let s = QuerySpec {
            text: Some("  ".into()),
            unread_only: true,
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(s.text, None);
        assert!(s.unread_only);
    }

    #[test]
    fn validate_rejects_overlong_text() {
        let s = QuerySpec {
            text: Some("x".repeat(MAX_TEXT_LEN + 1)),
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn name_parse_trims_and_collapses_whitespace() {
        let n = SavedViewName::parse("  Rust   未読 ").unwrap();
        assert_eq!(n.as_str(), "Rust 未読");
    }

    #[test]
    fn name_parse_rejects_empty() {
        assert!(SavedViewName::parse("   ").is_err());
    }
}
