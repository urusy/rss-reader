use chrono::NaiveDate;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Digest {
    pub date: NaiveDate,
    pub markdown: String,
    pub model: String,
    pub article_count: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// `?date=YYYY-MM-DD` value object; invalid dates rejected at construction.
#[derive(Debug, Clone, Copy)]
pub struct DigestDate(NaiveDate);

impl DigestDate {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, String> {
        let s = raw.as_ref().trim();
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(DigestDate)
            .map_err(|_| "date must be in YYYY-MM-DD format".to_string())
    }
    pub fn date(&self) -> NaiveDate {
        self.0
    }
}

/// One article's material for the digest (read projection).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DigestSource {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Body saved for a day with no new articles (no LLM call).
pub const EMPTY_DIGEST_MD: &str = "## 本日の新着記事はありませんでした\n";

/// Build the LLM input (Markdown bullets). Pure → unit-tested.
pub fn build_digest_input(items: &[DigestSource]) -> String {
    items
        .iter()
        .map(|it| {
            format!(
                "- [{}]({}): {}",
                it.title.trim(),
                it.url.trim(),
                it.snippet.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(title: &str, url: &str, snippet: &str) -> DigestSource {
        DigestSource {
            title: title.into(),
            url: url.into(),
            snippet: snippet.into(),
        }
    }

    #[test]
    fn digest_date_parses_valid() {
        let d = DigestDate::parse("2026-06-30").unwrap();
        assert_eq!(d.date(), NaiveDate::from_ymd_opt(2026, 6, 30).unwrap());
    }

    #[test]
    fn digest_date_rejects_bad_format() {
        assert!(DigestDate::parse("2026/06/30").is_err());
        assert!(DigestDate::parse("30-06-2026").is_err());
        assert!(DigestDate::parse("").is_err());
    }

    #[test]
    fn digest_date_rejects_impossible_date() {
        assert!(DigestDate::parse("2026-13-40").is_err());
    }

    #[test]
    fn build_digest_input_formats_bullets() {
        let out = build_digest_input(&[src("A", "http://a", "x"), src("B", "http://b", "y")]);
        assert_eq!(out, "- [A](http://a): x\n- [B](http://b): y");
    }

    #[test]
    fn build_digest_input_trims_fields() {
        let out = build_digest_input(&[src("  A ", " http://a ", "  x ")]);
        assert_eq!(out, "- [A](http://a): x");
    }

    #[test]
    fn build_digest_input_empty_is_empty_string() {
        assert_eq!(build_digest_input(&[]), "");
    }

    #[test]
    fn empty_digest_md_is_markdown_heading() {
        assert!(EMPTY_DIGEST_MD.starts_with("##"));
    }
}
