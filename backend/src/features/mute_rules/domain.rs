use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct MuteRuleId(pub Uuid);

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MuteRule {
    pub id: MuteRuleId,
    pub field: String, // "title" | "content" | "url"
    pub pattern: String,
    pub match_type: String, // "contains" (v1)
    pub action: String,     // "hide" | "mark_read"
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// field string → articles column name (whitelist). Column names can't be
/// parameterized, so we only ever return fixed literals — no user string is
/// concatenated, so this is injection-safe. Unknown → Validation(400).
pub fn field_column(field: &str) -> AppResult<&'static str> {
    match field {
        "title" => Ok("title"),
        "content" => Ok("content"),
        "url" => Ok("url"),
        other => Err(AppError::Validation(format!("unknown mute field: {other}"))),
    }
}

/// Escape LIKE/ILIKE wildcards (% _ \) so the user pattern matches literally
/// (use with ESCAPE '\\' and wrap in %...% for contains).
pub fn escape_like(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    for ch in pattern.chars() {
        match ch {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Validate create/update input (400 before the DB CHECK).
pub fn validate(field: &str, pattern: &str, match_type: &str, action: &str) -> AppResult<()> {
    field_column(field)?;
    if pattern.trim().is_empty() {
        return Err(AppError::Validation("pattern must not be empty".into()));
    }
    if match_type != "contains" {
        return Err(AppError::Validation(format!(
            "unsupported match_type: {match_type} (only 'contains' in v1)"
        )));
    }
    if action != "hide" && action != "mark_read" {
        return Err(AppError::Validation(format!("unknown action: {action}")));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct NewMuteRule {
    pub field: String,
    pub pattern: String,
    #[serde(default = "default_match_type")]
    pub match_type: String,
    #[serde(default = "default_action")]
    pub action: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct PatchMuteRule {
    pub field: Option<String>,
    pub pattern: Option<String>,
    pub match_type: Option<String>,
    pub action: Option<String>,
    pub enabled: Option<bool>,
}

fn default_match_type() -> String {
    "contains".into()
}
fn default_action() -> String {
    "hide".into()
}
fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_like_passes_plain_text() {
        assert_eq!(escape_like("Sponsored"), "Sponsored");
    }

    #[test]
    fn escape_like_escapes_percent_and_underscore() {
        assert_eq!(escape_like("50%_off"), "50\\%\\_off");
    }

    #[test]
    fn escape_like_escapes_backslash() {
        assert_eq!(escape_like("a\\b"), "a\\\\b");
    }

    #[test]
    fn escape_like_handles_unicode() {
        assert_eq!(escape_like("広告"), "広告");
    }

    #[test]
    fn field_column_maps_known_fields() {
        assert_eq!(field_column("title").unwrap(), "title");
        assert_eq!(field_column("content").unwrap(), "content");
        assert_eq!(field_column("url").unwrap(), "url");
    }

    #[test]
    fn field_column_rejects_unknown_field() {
        assert!(field_column("title; DROP TABLE articles--").is_err());
    }

    #[test]
    fn validate_rejects_empty_pattern() {
        assert!(validate("title", "   ", "contains", "hide").is_err());
    }

    #[test]
    fn validate_rejects_regex_in_v1() {
        assert!(validate("title", "ad", "regex", "hide").is_err());
    }

    #[test]
    fn validate_rejects_unknown_action() {
        assert!(validate("title", "ad", "contains", "delete").is_err());
    }

    #[test]
    fn validate_accepts_well_formed_rule() {
        assert!(validate("url", "example.com", "contains", "mark_read").is_ok());
    }
}
