//! Pure feed-autodiscovery logic: classify feed MIME types and extract
//! <link rel="alternate"> feed candidates from HTML. No network/DB → unit-tested.

use reqwest::Url; // reqwest re-exports the url crate (no extra dependency)
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveredFeed {
    pub url: String,
    pub title: Option<String>,
    pub kind: FeedKind,
    pub already_subscribed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedKind {
    Rss,
    Atom,
    Json,
    Unknown,
}

/// Input URL value object (http/https, trimmed); slice-local like SaveUrl.
#[derive(Debug, Clone)]
pub struct DiscoverUrl(String);

impl DiscoverUrl {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into();
        let t = s.trim();
        if !(t.starts_with("http://") || t.starts_with("https://")) {
            return Err("url must start with http:// or https://".into());
        }
        Ok(Self(t.to_string()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn mime_of(s: &str) -> String {
    s.split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// MIME (type attr / content-type) → FeedKind. Generic XML → Unknown.
pub fn feed_kind_from_type(type_attr: &str) -> FeedKind {
    match mime_of(type_attr).as_str() {
        "application/rss+xml" => FeedKind::Rss,
        "application/atom+xml" => FeedKind::Atom,
        "application/feed+json" | "application/json" => FeedKind::Json,
        _ => FeedKind::Unknown,
    }
}

/// Whether a content-type indicates a feed body (for self-detection). Bare
/// application/json excluded (could be an API response).
pub fn is_feed_content_type(content_type: &str) -> bool {
    matches!(
        mime_of(content_type).as_str(),
        "application/rss+xml"
            | "application/atom+xml"
            | "application/feed+json"
            | "application/xml"
            | "text/xml"
    )
}

/// Extract <link rel="alternate" type="<feed mime>" href> from HTML, resolving
/// href against `base` (the final fetched URL). Pure (no network).
pub fn extract_feed_links(html: &str, base: &Url) -> Vec<DiscoveredFeed> {
    use scraper::{Html, Selector};
    use std::collections::HashSet;

    let doc = Html::parse_document(html);
    let sel = Selector::parse("link").expect("static 'link' selector is valid");
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<DiscoveredFeed> = Vec::new();

    for el in doc.select(&sel) {
        let rel = el.value().attr("rel").unwrap_or_default();
        let is_alternate = rel
            .split_whitespace()
            .any(|t| t.eq_ignore_ascii_case("alternate"));
        if !is_alternate {
            continue;
        }
        let kind = feed_kind_from_type(el.value().attr("type").unwrap_or_default());
        if matches!(kind, FeedKind::Unknown) {
            continue;
        }
        let href = el.value().attr("href").unwrap_or_default().trim();
        if href.is_empty() {
            continue;
        }
        let Ok(abs) = base.join(href) else {
            continue;
        };
        let url = abs.to_string();
        if !seen.insert(url.clone()) {
            continue;
        }
        let title = el
            .value()
            .attr("title")
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        out.push(DiscoveredFeed {
            url,
            title,
            kind,
            already_subscribed: false,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/blog/").unwrap()
    }

    #[test]
    fn discover_url_accepts_http_and_https() {
        assert!(DiscoverUrl::parse("http://example.com").is_ok());
        assert!(DiscoverUrl::parse("https://example.com/blog").is_ok());
    }

    #[test]
    fn discover_url_trims_and_rejects_missing_scheme() {
        assert_eq!(
            DiscoverUrl::parse("  https://example.com  ")
                .unwrap()
                .as_str(),
            "https://example.com"
        );
        assert!(DiscoverUrl::parse("example.com").is_err());
        assert!(DiscoverUrl::parse("").is_err());
    }

    #[test]
    fn feed_kind_maps_known_mimes() {
        assert_eq!(feed_kind_from_type("application/rss+xml"), FeedKind::Rss);
        assert_eq!(feed_kind_from_type("application/atom+xml"), FeedKind::Atom);
        assert_eq!(feed_kind_from_type("application/feed+json"), FeedKind::Json);
        assert_eq!(feed_kind_from_type("application/json"), FeedKind::Json);
    }

    #[test]
    fn feed_kind_ignores_charset_param_and_case() {
        assert_eq!(
            feed_kind_from_type("Application/RSS+XML; charset=utf-8"),
            FeedKind::Rss
        );
    }

    #[test]
    fn feed_kind_unknown_for_non_feed() {
        assert_eq!(feed_kind_from_type("text/html"), FeedKind::Unknown);
        assert_eq!(feed_kind_from_type("application/xml"), FeedKind::Unknown);
        assert_eq!(feed_kind_from_type(""), FeedKind::Unknown);
    }

    #[test]
    fn is_feed_content_type_true_for_feed_mimes() {
        assert!(is_feed_content_type("application/rss+xml; charset=utf-8"));
        assert!(is_feed_content_type("application/atom+xml"));
        assert!(is_feed_content_type("text/xml"));
        assert!(is_feed_content_type("application/feed+json"));
    }

    #[test]
    fn is_feed_content_type_false_for_html_and_bare_json() {
        assert!(!is_feed_content_type("text/html"));
        assert!(!is_feed_content_type("application/json"));
        assert!(!is_feed_content_type(""));
    }

    #[test]
    fn extracts_rss_and_atom_links() {
        let html = r#"<html><head>
            <link rel="alternate" type="application/rss+xml" title="RSS" href="https://example.com/rss.xml">
            <link rel="alternate" type="application/atom+xml" href="https://example.com/atom.xml">
        </head></html>"#;
        let got = extract_feed_links(html, &base());
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].url, "https://example.com/rss.xml");
        assert_eq!(got[0].kind, FeedKind::Rss);
        assert_eq!(got[0].title.as_deref(), Some("RSS"));
        assert_eq!(got[1].kind, FeedKind::Atom);
        assert_eq!(got[1].title, None);
    }

    #[test]
    fn resolves_relative_href_against_base() {
        let html =
            r#"<head><link rel="alternate" type="application/rss+xml" href="../feed.xml"></head>"#;
        let got = extract_feed_links(html, &base());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].url, "https://example.com/feed.xml");
    }

    #[test]
    fn ignores_non_feed_alternate() {
        let html = r#"<head>
            <link rel="alternate" type="application/xhtml+xml" href="/m.html">
            <link rel="stylesheet" href="/style.css">
            <link rel="alternate" hreflang="en" href="/en/">
        </head>"#;
        assert!(extract_feed_links(html, &base()).is_empty());
    }

    #[test]
    fn dedups_same_resolved_url() {
        let html = r#"<head>
            <link rel="alternate" type="application/rss+xml" href="/rss.xml">
            <link rel="alternate" type="application/rss+xml" href="https://example.com/rss.xml">
        </head>"#;
        assert_eq!(extract_feed_links(html, &base()).len(), 1);
    }

    #[test]
    fn handles_multi_token_rel_and_skips_empty_href() {
        let html = r#"<head>
            <link rel="alternate home" type="application/rss+xml" href="/a.xml">
            <link rel="alternate" type="application/rss+xml" href="">
        </head>"#;
        let got = extract_feed_links(html, &base());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].url, "https://example.com/a.xml");
    }

    #[test]
    fn empty_when_no_link_tags() {
        assert!(extract_feed_links("<html><body>no head links</body></html>", &base()).is_empty());
    }
}
