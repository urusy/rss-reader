use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct TagId(pub Uuid);

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Tag {
    pub id: TagId,
    pub name: String,
    pub color: Option<String>,
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TagWithCount {
    pub id: TagId,
    pub name: String,
    pub color: Option<String>,
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub article_count: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ArticleTag {
    pub id: TagId,
    pub name: String,
    pub color: Option<String>,
    pub attached_source: String,
    pub confidence: Option<f32>,
}

const MAX_TAG_LEN: usize = 50;

/// Validated tag name. Empty / too-long rejected at construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagName(String);

/// Collapse internal whitespace runs to a single space and trim. Case is left as
/// typed (the DB's lower(name) unique index handles case-insensitivity).
pub fn normalize_name(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl TagName {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let n = normalize_name(&raw.into());
        if n.is_empty() {
            return Err("tag name must not be empty".into());
        }
        if n.chars().count() > MAX_TAG_LEN {
            return Err(format!("tag name must be at most {MAX_TAG_LEN} characters"));
        }
        Ok(Self(n))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RawSuggestion {
    pub name: String,
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// Parse Claude's raw output into normalized suggestions. Slices from the first
/// `[` to the last `]` (tolerating prose / code fences), dedups case-insensitively,
/// drops empties, clamps confidence, truncates to max_tags. Err = safe fallback.
pub fn parse_tag_suggestions(raw: &str, max_tags: usize) -> Result<Vec<RawSuggestion>, String> {
    let start = raw.find('[').ok_or("no JSON array found in LLM output")?;
    let end = raw.rfind(']').ok_or("no JSON array found in LLM output")?;
    if end < start {
        return Err("malformed JSON array in LLM output".into());
    }
    let slice = &raw[start..=end];
    let parsed: Vec<RawSuggestion> =
        serde_json::from_str(slice).map_err(|e| format!("invalid suggestion JSON: {e}"))?;

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for s in parsed {
        let name = normalize_name(&s.name);
        if name.is_empty() {
            continue;
        }
        let key = name.to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        let confidence = s.confidence.map(|c| c.clamp(0.0, 1.0));
        out.push(RawSuggestion { name, confidence });
        if out.len() >= max_tags {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_name_rejects_empty() {
        assert!(TagName::parse("").is_err());
        assert!(TagName::parse("   ").is_err());
    }

    #[test]
    fn tag_name_trims_and_collapses_whitespace() {
        assert_eq!(
            TagName::parse("  rust   lang ").unwrap().as_str(),
            "rust lang"
        );
    }

    #[test]
    fn tag_name_rejects_too_long() {
        assert!(TagName::parse("a".repeat(51)).is_err());
    }

    #[test]
    fn tag_name_accepts_valid() {
        assert_eq!(TagName::parse("Rust").unwrap().as_str(), "Rust");
    }

    #[test]
    fn normalize_name_collapses_internal_spaces() {
        assert_eq!(normalize_name("a   b\tc"), "a b c");
    }

    #[test]
    fn parse_suggestions_parses_plain_array() {
        let r = parse_tag_suggestions(r#"[{"name":"rust","confidence":0.9}]"#, 6).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "rust");
        assert_eq!(r[0].confidence, Some(0.9));
    }

    #[test]
    fn parse_suggestions_strips_prose_and_fences() {
        let raw = "Here are the tags:\n```json\n[{\"name\":\"async\"}]\n```\nDone.";
        let r = parse_tag_suggestions(raw, 6).unwrap();
        assert_eq!(r[0].name, "async");
    }

    #[test]
    fn parse_suggestions_dedupes_case_insensitive() {
        let r = parse_tag_suggestions(r#"[{"name":"Rust"},{"name":"rust"}]"#, 6).unwrap();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn parse_suggestions_drops_empty_names() {
        let r = parse_tag_suggestions(r#"[{"name":"  "},{"name":"ok"}]"#, 6).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "ok");
    }

    #[test]
    fn parse_suggestions_truncates_to_max() {
        let r = parse_tag_suggestions(r#"[{"name":"a"},{"name":"b"},{"name":"c"}]"#, 2).unwrap();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn parse_suggestions_clamps_confidence() {
        let r = parse_tag_suggestions(
            r#"[{"name":"a","confidence":1.5},{"name":"b","confidence":-0.2}]"#,
            6,
        )
        .unwrap();
        assert_eq!(r[0].confidence, Some(1.0));
        assert_eq!(r[1].confidence, Some(0.0));
    }

    #[test]
    fn parse_suggestions_errors_on_no_array() {
        assert!(parse_tag_suggestions("no array here", 6).is_err());
    }

    #[test]
    fn parse_suggestions_errors_on_malformed_json() {
        assert!(parse_tag_suggestions("[{name: rust}]", 6).is_err());
    }
}
