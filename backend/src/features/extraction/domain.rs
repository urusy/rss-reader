//! Pure, network-free extraction logic: parse HTML, pick the main body
//! container, sanitize it, and judge whether it's a "real" body by plain-text
//! length. Everything here is deterministic so it can be unit-tested offline
//! (Red→Green) — see the tests at the bottom.

use scraper::{ElementRef, Html, Selector};

/// Tags removed *with their content* during sanitize. Several of these
/// (`nav`/`header`/`footer`/`aside`) are in ammonia's default tag whitelist, so
/// we must `rm_tags` them before `clean_content_tags` or ammonia panics.
const STRIP_TAGS: [&str; 9] = [
    "script", "style", "nav", "header", "footer", "aside", "form", "noscript", "iframe",
];

/// URL value object for the fetch target. `articles.url` should already be
/// http(s), but parse defensively so a bad row can't make us fetch garbage.
#[derive(Debug, Clone)]
pub struct FetchUrl(String);

impl FetchUrl {
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

/// Extraction outcome. We only persist `Ok` bodies; `TooThin` leaves
/// `full_content` NULL so display/AI fall back to the feed `content`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extracted {
    /// Sanitized body HTML.
    Ok(String),
    /// No body found, or it was too short to be meaningful.
    TooThin,
}

/// Entry point: raw fetched HTML → main body → sanitized → min-length check.
pub fn extract_main_content(raw_html: &str, min_chars: usize) -> Extracted {
    let doc = Html::parse_document(raw_html);
    let candidate = pick_main_html(&doc).unwrap_or_else(|| raw_html.to_string());
    let clean = sanitize_content(&candidate);
    if plain_text_len(&clean) >= min_chars {
        Extracted::Ok(clean)
    } else {
        Extracted::TooThin
    }
}

/// Choose the body container HTML. Priority: first `<article>`, then first
/// `<main>`, then the highest-scoring `<div>`/`<section>`. Score = body text
/// length − link text length (penalizes nav/link-dense blocks).
pub fn pick_main_html(doc: &Html) -> Option<String> {
    if let Some(html) = first_inner_html(doc, "article") {
        return Some(html);
    }
    if let Some(html) = first_inner_html(doc, "main") {
        return Some(html);
    }
    let block_sel = Selector::parse("div, section").ok()?;
    let mut best: Option<(i64, String)> = None;
    for el in doc.select(&block_sel) {
        let score = score_node_text(&el.text().collect::<String>(), link_text_len(&el));
        if score > best.as_ref().map(|(s, _)| *s).unwrap_or(0) {
            best = Some((score, el.inner_html()));
        }
    }
    best.map(|(_, html)| html)
}

fn first_inner_html(doc: &Html, sel: &str) -> Option<String> {
    let s = Selector::parse(sel).ok()?;
    doc.select(&s).next().map(|el| el.inner_html())
}

/// Total length of text inside `<a>` descendants (link-density signal).
fn link_text_len(el: &ElementRef) -> i64 {
    let a = Selector::parse("a").unwrap();
    el.select(&a)
        .map(|x| x.text().collect::<String>().chars().count() as i64)
        .sum()
}

/// Body score (pure → tested). Body length minus link chars; tiny fragments → 0.
pub fn score_node_text(text: &str, link_len: i64) -> i64 {
    let len = text.chars().filter(|c| !c.is_whitespace()).count() as i64;
    if len < 25 {
        return 0;
    }
    (len - link_len).max(0)
}

/// Sanitize HTML (Rust-side DOMPurify equivalent = ammonia). Noise tags are
/// removed with their content. ammonia's default `link_rel` already adds
/// `rel="noopener noreferrer"` to links, so we don't set it (setting it while
/// `rel` is otherwise managed would panic).
pub fn sanitize_content(raw_html: &str) -> String {
    ammonia::Builder::default()
        // Drop noise tags from the whitelist first; otherwise adding a
        // whitelisted tag (nav/header/footer/aside) to clean_content_tags panics.
        .rm_tags(STRIP_TAGS)
        .clean_content_tags(STRIP_TAGS.into_iter().collect())
        .clean(raw_html)
        .to_string()
}

/// Approximate plain-text length (whitespace excluded) of sanitized HTML.
pub fn plain_text_len(html: &str) -> usize {
    let doc = Html::parse_fragment(html);
    doc.root_element()
        .text()
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html><body>
        <nav><a href="/">home</a></nav>
        <article><p>これは十分に長い本文の段落です。意味のある文章が続きます。</p>
        <p>二つ目の段落も本文として抽出されるべきです。</p></article>
        <footer><a href="/about">about</a></footer>
        <script>alert(1)</script>
    </body></html>"#;

    #[test]
    fn fetch_url_accepts_http_and_https() {
        assert!(FetchUrl::parse("http://a.com").is_ok());
        assert!(FetchUrl::parse("https://a.com").is_ok());
    }

    #[test]
    fn fetch_url_rejects_missing_scheme() {
        assert!(FetchUrl::parse("ftp://a.com").is_err());
        assert!(FetchUrl::parse("a.com").is_err());
    }

    #[test]
    fn fetch_url_trims() {
        let u = FetchUrl::parse("  https://a.com/post  ").unwrap();
        assert_eq!(u.as_str(), "https://a.com/post");
    }

    #[test]
    fn pick_main_prefers_article_tag() {
        let doc = Html::parse_document(PAGE);
        let html = pick_main_html(&doc).unwrap();
        assert!(html.contains("本文の段落"));
        // The <article> content was chosen, not the page-level nav/footer.
        assert!(!html.to_lowercase().contains("<nav"));
    }

    #[test]
    fn pick_main_falls_back_to_main_tag() {
        let page = r#"<html><body><main><p>main body paragraph here</p></main></body></html>"#;
        let doc = Html::parse_document(page);
        let html = pick_main_html(&doc).unwrap();
        assert!(html.contains("main body paragraph"));
    }

    #[test]
    fn pick_main_scores_highest_p_density_block() {
        let page = r#"<html><body>
            <div id="side"><a href="/1">l1</a><a href="/2">l2</a></div>
            <div id="content"><p>本文がここに入ります。これは十分に長い段落で、抽出対象になります。</p></div>
        </body></html>"#;
        let doc = Html::parse_document(page);
        let html = pick_main_html(&doc).unwrap();
        assert!(html.contains("本文がここに入ります"));
    }

    #[test]
    fn score_node_text_penalizes_links() {
        let text = "a".repeat(100);
        let no_links = score_node_text(&text, 0);
        let many_links = score_node_text(&text, 60);
        assert!(many_links < no_links);
    }

    #[test]
    fn score_node_text_zero_for_tiny() {
        assert_eq!(score_node_text("short", 0), 0);
    }

    #[test]
    fn sanitize_strips_script_and_style() {
        let out = sanitize_content("<p>keep</p><script>alert(1)</script><style>a{}</style>");
        assert!(out.contains("keep"));
        assert!(!out.contains("alert(1)"));
        assert!(!out.contains("a{}"));
    }

    #[test]
    fn sanitize_strips_nav_footer_aside() {
        let out = sanitize_content(
            "<nav>NAVTEXT</nav><p>body</p><footer>FOOTTEXT</footer><aside>ASIDETEXT</aside>",
        );
        assert!(out.contains("body"));
        assert!(!out.contains("NAVTEXT"));
        assert!(!out.contains("FOOTTEXT"));
        assert!(!out.contains("ASIDETEXT"));
    }

    #[test]
    fn sanitize_adds_rel_noopener_to_links() {
        let out = sanitize_content(r#"<a href="https://x.com">x</a>"#);
        assert!(out.contains("noopener"));
    }

    #[test]
    fn extract_main_returns_too_thin_when_below_min() {
        assert_eq!(
            extract_main_content("<article><p>hi</p></article>", 200),
            Extracted::TooThin
        );
    }

    #[test]
    fn extract_main_returns_ok_for_real_body() {
        match extract_main_content(PAGE, 10) {
            Extracted::Ok(html) => {
                assert!(html.contains("本文の段落"));
                assert!(!html.contains("alert(1)"));
                assert!(!html.to_lowercase().contains("<nav"));
            }
            Extracted::TooThin => panic!("expected Ok"),
        }
    }

    #[test]
    fn plain_text_len_ignores_tags_and_whitespace() {
        assert_eq!(plain_text_len("<p>ab  cd</p>"), 4);
    }
}
