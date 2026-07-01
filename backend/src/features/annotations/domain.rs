//! Annotations domain: stars + highlights. Pure validation/normalization.

use serde::Serialize;
use uuid::Uuid;

/// A highlight as stored/returned. `quote` is the durable anchor; offsets are a
/// best-effort hint that may go stale when the article body changes.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Highlight {
    pub id: Uuid,
    pub article_id: Uuid,
    pub quote: String,
    pub note: Option<String>,
    pub start_offset: Option<i32>,
    pub end_offset: Option<i32>,
    pub color: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Validated input for creating a highlight.
pub struct NewHighlight {
    pub quote: String,
    pub note: Option<String>,
    pub start_offset: Option<i32>,
    pub end_offset: Option<i32>,
    pub color: Option<String>,
}

impl NewHighlight {
    /// Trim, drop empty optionals, and reject an empty quote.
    pub fn parse(
        quote: String,
        note: Option<String>,
        start_offset: Option<i32>,
        end_offset: Option<i32>,
        color: Option<String>,
    ) -> Result<Self, String> {
        let quote = quote.trim().to_string();
        if quote.is_empty() {
            return Err("quote must not be empty".into());
        }
        if quote.chars().count() > 10_000 {
            return Err("quote too long (max 10000 chars)".into());
        }
        Ok(Self {
            quote,
            note: clean_opt(note),
            start_offset,
            end_offset,
            color: clean_opt(color),
        })
    }
}

/// A validated patch for an existing highlight (note/color only).
pub struct HighlightPatch {
    pub note: Option<String>,
    pub color: Option<String>,
}

impl HighlightPatch {
    pub fn parse(note: Option<String>, color: Option<String>) -> Self {
        Self {
            note: clean_opt(note),
            color: clean_opt(color),
        }
    }
}

/// Treat a whitespace-only optional string as absent.
fn clean_opt(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trims_and_rejects_empty_quote() {
        assert!(NewHighlight::parse("   ".into(), None, None, None, None).is_err());
        let h = NewHighlight::parse("  hi  ".into(), Some("  ".into()), None, None, None).unwrap();
        assert_eq!(h.quote, "hi");
        assert_eq!(h.note, None); // whitespace-only note dropped
    }

    #[test]
    fn parse_keeps_real_note_and_color() {
        let h = NewHighlight::parse(
            "q".into(),
            Some(" memo ".into()),
            Some(3),
            Some(8),
            Some("yellow".into()),
        )
        .unwrap();
        assert_eq!(h.note.as_deref(), Some("memo"));
        assert_eq!(h.color.as_deref(), Some("yellow"));
        assert_eq!(h.start_offset, Some(3));
    }

    #[test]
    fn patch_normalizes_blanks_to_none() {
        let p = HighlightPatch::parse(Some("".into()), Some("  ".into()));
        assert_eq!(p.note, None);
        assert_eq!(p.color, None);
    }
}
