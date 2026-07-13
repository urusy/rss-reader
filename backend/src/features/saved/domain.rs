//! Domain types for the saved-pages slice (Pocket 風「後で読む」).
//!
//! 保存ページは合成フィード（`SAVED_FEED_ID`）配下の通常 `articles` 行として
//! 表現する。ここには newtype と純関数だけを置く（ネットワーク・DB 非依存）。

use scraper::{Html, Selector};
use uuid::Uuid;

/// 保存ページ用合成フィードの固定 UUID。migration 0026 の INSERT と一致する
/// こと（`saved_feed_id_matches_migration` テストが守る）。
pub const SAVED_FEED_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0501);

/// 保存対象 URL の値オブジェクト。http(s) のみ・前後空白除去・fragment 除去。
/// 構築を通らない不正 URL はスライス内に入り込めない（FeedUrl と同じ流儀）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedUrl(String);

impl SavedUrl {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into();
        let trimmed = s.trim();
        if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
            return Err("url must start with http:// or https://".to_string());
        }
        // #fragment はページ内位置でしかなく、同一ページの二重保存の温床になる
        // ため落とす（articles.url は UNIQUE）。
        let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
        if without_fragment.len() <= "https://".len() {
            return Err("url has no host".to_string());
        }
        Ok(Self(without_fragment.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 保存ページのタイトルを HTML から取る。優先順: og:title → <title>。
/// 取れなければ None（呼び出し側が URL を暫定タイトルのまま残す）。
pub fn extract_page_title(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let og = Selector::parse(r#"meta[property="og:title"]"#).ok()?;
    if let Some(t) = doc
        .select(&og)
        .find_map(|el| el.value().attr("content"))
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        return Some(t.to_string());
    }
    let title = Selector::parse("title").ok()?;
    doc.select(&title)
        .next()
        .map(|el| el.text().collect::<String>())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_feed_id_matches_migration() {
        // migration 0026 の INSERT と一字一句一致すること
        assert_eq!(
            SAVED_FEED_ID.to_string(),
            "00000000-0000-0000-0000-000000000501"
        );
    }

    #[test]
    fn parse_accepts_https_and_trims() {
        let u = SavedUrl::parse("  https://example.com/post  ").unwrap();
        assert_eq!(u.as_str(), "https://example.com/post");
    }

    #[test]
    fn parse_strips_fragment() {
        let u = SavedUrl::parse("https://example.com/post#section-2").unwrap();
        assert_eq!(u.as_str(), "https://example.com/post");
    }

    #[test]
    fn parse_rejects_non_http() {
        assert!(SavedUrl::parse("ftp://example.com/x").is_err());
        assert!(SavedUrl::parse("javascript:alert(1)").is_err());
        assert!(SavedUrl::parse("example.com/x").is_err());
        assert!(SavedUrl::parse("").is_err());
    }

    #[test]
    fn parse_rejects_scheme_only() {
        assert!(SavedUrl::parse("https://").is_err());
        assert!(SavedUrl::parse("https://#frag").is_err());
    }

    #[test]
    fn title_prefers_og_title() {
        let html = r#"<html><head>
            <meta property="og:title" content="OG Title" />
            <title>Doc Title</title>
        </head><body></body></html>"#;
        assert_eq!(extract_page_title(html).as_deref(), Some("OG Title"));
    }

    #[test]
    fn title_falls_back_to_title_tag() {
        let html = "<html><head><title>  Doc Title  </title></head><body></body></html>";
        assert_eq!(extract_page_title(html).as_deref(), Some("Doc Title"));
    }

    #[test]
    fn title_none_when_absent_or_empty() {
        assert_eq!(
            extract_page_title("<html><body><p>x</p></body></html>"),
            None
        );
        assert_eq!(
            extract_page_title(
                r#"<head><meta property="og:title" content="  " /><title></title></head>"#
            ),
            None
        );
    }
}
