use crate::shared::error::{AppError, AppResult};

/// A validated, non-empty search query.
///
/// Constructing one guarantees the query is trimmed and non-empty, so we never
/// run a `'%%'`-matches-everything scan from a blank `?q=`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery(String);

impl SearchQuery {
    /// Trim and validate. Whitespace-only input is rejected.
    pub fn parse(raw: &str) -> AppResult<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(AppError::Validation(
                "search query must not be empty".into(),
            ));
        }
        Ok(Self(trimmed.to_string()))
    }

    /// The raw trimmed query, used as the argument to `similarity()` for ranking.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// An `ILIKE` pattern wrapped in `%…%` wildcards, with the query's own LIKE
    /// metacharacters (`\`, `%`, `_`) backslash-escaped so user input cannot act
    /// as a wildcard. Use with `ILIKE <pattern> ESCAPE '\'`.
    pub fn like_pattern(&self) -> String {
        let mut pattern = String::with_capacity(self.0.len() + 2);
        pattern.push('%');
        for ch in self.0.chars() {
            if matches!(ch, '\\' | '%' | '_') {
                pattern.push('\\');
            }
            pattern.push(ch);
        }
        pattern.push('%');
        pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trims_and_keeps_inner_text() {
        let q = SearchQuery::parse("  rust async  ").unwrap();
        assert_eq!(q.as_str(), "rust async");
    }

    #[test]
    fn parse_rejects_empty_and_whitespace() {
        assert!(SearchQuery::parse("").is_err());
        assert!(SearchQuery::parse("   ").is_err());
        assert!(SearchQuery::parse("\t\n").is_err());
    }

    #[test]
    fn parse_accepts_japanese() {
        let q = SearchQuery::parse("機械学習").unwrap();
        assert_eq!(q.as_str(), "機械学習");
    }

    #[test]
    fn like_pattern_wraps_in_wildcards() {
        let q = SearchQuery::parse("rust").unwrap();
        assert_eq!(q.like_pattern(), "%rust%");
    }

    #[test]
    fn like_pattern_escapes_like_metacharacters() {
        // The user's literal `%`, `_`, `\` must be escaped; the wrapping `%` stay.
        let q = SearchQuery::parse("50%_off\\sale").unwrap();
        assert_eq!(q.like_pattern(), "%50\\%\\_off\\\\sale%");
    }
}
