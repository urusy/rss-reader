//! Pure OPML parse/generate logic (no I/O → unit-tested offline).
//! Parser uses quick-xml's pull reader; generator is hand-written string output
//! with attribute escaping (deterministic, dependency-free, easy to test).

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use serde::Serialize;

// ---- import (parse) side ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFeed {
    pub title: Option<String>,
    pub xml_url: String,
}

/// folder=None means "unfiled" (body-level feeds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGroup {
    pub folder: Option<String>,
    pub feeds: Vec<ParsedFeed>,
}

fn attr_value(e: &BytesStart, key: &[u8]) -> Result<Option<String>, String> {
    match e.try_get_attribute(key).map_err(|e| e.to_string())? {
        Some(a) => {
            // value is Cow<[u8]>; decode (UTF-8) then unescape entities.
            // (unescape_value is deprecated in 0.41; normalized_value needs an
            // XmlVersion arg — this is the simple non-deprecated equivalent.)
            let raw = String::from_utf8_lossy(a.value.as_ref());
            let val = quick_xml::escape::unescape(&raw).map_err(|e| e.to_string())?;
            Ok(Some(val.into_owned()))
        }
        None => Ok(None),
    }
}

/// Feed projection of an <outline> if it carries xmlUrl; else None (= folder).
fn outline_feed(e: &BytesStart) -> Result<Option<ParsedFeed>, String> {
    match attr_value(e, b"xmlUrl")? {
        Some(u) if !u.trim().is_empty() => {
            let title = match attr_value(e, b"title")? {
                Some(t) => Some(t),
                None => attr_value(e, b"text")?,
            };
            Ok(Some(ParsedFeed { title, xml_url: u }))
        }
        _ => Ok(None),
    }
}

/// Folder label from title, else text.
fn outline_label(e: &BytesStart) -> Result<Option<String>, String> {
    match attr_value(e, b"title")? {
        Some(t) => Ok(Some(t)),
        None => attr_value(e, b"text"),
    }
}

fn push_feed(groups: &mut Vec<ParsedGroup>, folder: Option<String>, feed: ParsedFeed) {
    if let Some(g) = groups.iter_mut().find(|g| g.folder == folder) {
        g.feeds.push(feed);
    } else {
        groups.push(ParsedGroup {
            folder,
            feeds: vec![feed],
        });
    }
}

/// OPML XML → groups. xmlUrl => feed (attached to nearest ancestor folder);
/// otherwise a folder (its label pushed on a stack; deep nesting flattens to the
/// innermost folder name). Malformed XML => Err.
pub fn parse_opml(xml: &str) -> Result<Vec<ParsedGroup>, String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    // Per-open-<outline> stack entry: Some(name)=folder, None=feed/leaf. Current
    // folder = innermost Some on the stack.
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut groups: Vec<ParsedGroup> = Vec::new();

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| e.to_string())?
        {
            Event::Start(e) if e.name().as_ref() == b"outline" => {
                if let Some(feed) = outline_feed(&e)? {
                    let folder = stack.iter().rev().find_map(|x| x.clone());
                    push_feed(&mut groups, folder, feed);
                    stack.push(None); // leaf; children (rare) keep ancestor folder
                } else {
                    stack.push(outline_label(&e)?); // folder (label may be None)
                }
            }
            Event::Empty(e) if e.name().as_ref() == b"outline" => {
                if let Some(feed) = outline_feed(&e)? {
                    let folder = stack.iter().rev().find_map(|x| x.clone());
                    push_feed(&mut groups, folder, feed);
                }
                // self-closing folder (no xmlUrl) carries no feeds → ignore
            }
            Event::End(e) if e.name().as_ref() == b"outline" => {
                stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(groups)
}

// ---- export (generate) side ----

#[derive(Debug, Clone)]
pub struct ExportFeed {
    pub title: Option<String>,
    pub xml_url: String,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExportGroup {
    pub folder: Option<String>,
    pub feeds: Vec<ExportFeed>,
}

pub fn build_opml(groups: &[ExportGroup]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<opml version=\"2.0\">\n");
    out.push_str("  <head>\n    <title>RSS Reader Subscriptions</title>\n  </head>\n");
    out.push_str("  <body>\n");
    for g in groups {
        match &g.folder {
            Some(name) => {
                let n = xml_escape(name);
                out.push_str(&format!("    <outline text=\"{n}\" title=\"{n}\">\n"));
                for f in &g.feeds {
                    out.push_str(&feed_outline(f, "      "));
                }
                out.push_str("    </outline>\n");
            }
            None => {
                for f in &g.feeds {
                    out.push_str(&feed_outline(f, "    "));
                }
            }
        }
    }
    out.push_str("  </body>\n</opml>\n");
    out
}

fn feed_outline(f: &ExportFeed, indent: &str) -> String {
    let title = xml_escape(f.title.as_deref().unwrap_or(""));
    let xml_url = xml_escape(&f.xml_url);
    let html = f
        .html_url
        .as_deref()
        .map(|h| format!(" htmlUrl=\"{}\"", xml_escape(h)))
        .unwrap_or_default();
    format!(
        "{indent}<outline type=\"rss\" text=\"{title}\" title=\"{title}\" xmlUrl=\"{xml_url}\"{html}/>\n"
    )
}

/// Escape XML attribute values (& first to avoid double-escaping).
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportSummary {
    pub imported_feeds: usize,
    pub imported_folders: usize,
    pub skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_feed_no_folder() {
        let g = parse_opml(
            r#"<opml><body><outline type="rss" text="HN" xmlUrl="https://hn/rss"/></body></opml>"#,
        )
        .unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].folder, None);
        assert_eq!(g[0].feeds[0].xml_url, "https://hn/rss");
        assert_eq!(g[0].feeds[0].title.as_deref(), Some("HN"));
    }

    #[test]
    fn parse_feed_inside_folder() {
        let g = parse_opml(
            r#"<opml><body><outline text="Tech"><outline xmlUrl="https://r/feed" title="Rust"/></outline></body></opml>"#,
        )
        .unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].folder.as_deref(), Some("Tech"));
        assert_eq!(g[0].feeds.len(), 1);
        assert_eq!(g[0].feeds[0].title.as_deref(), Some("Rust"));
    }

    #[test]
    fn parse_multiple_feeds_in_one_folder() {
        let g = parse_opml(
            r#"<opml><body><outline text="T"><outline xmlUrl="https://a"/><outline xmlUrl="https://b"/></outline></body></opml>"#,
        )
        .unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].feeds.len(), 2);
    }

    #[test]
    fn parse_title_falls_back_to_text_attr() {
        let g = parse_opml(
            r#"<opml><body><outline text="OnlyText" xmlUrl="https://a"/></body></opml>"#,
        )
        .unwrap();
        assert_eq!(g[0].feeds[0].title.as_deref(), Some("OnlyText"));
    }

    #[test]
    fn parse_title_none_when_both_absent() {
        let g = parse_opml(r#"<opml><body><outline xmlUrl="https://a"/></body></opml>"#).unwrap();
        assert_eq!(g[0].feeds[0].title, None);
    }

    #[test]
    fn parse_skips_outline_without_xmlurl_as_folder() {
        let g =
            parse_opml(r#"<opml><body><outline text="Empty"></outline></body></opml>"#).unwrap();
        // folder with no feeds yields no group
        assert!(g.is_empty());
    }

    #[test]
    fn parse_deep_nesting_flattens_to_innermost_folder() {
        let g = parse_opml(
            r#"<opml><body><outline text="Outer"><outline text="Inner"><outline xmlUrl="https://a"/></outline></outline></body></opml>"#,
        )
        .unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].folder.as_deref(), Some("Inner"));
    }

    #[test]
    fn parse_unescapes_entities() {
        let g = parse_opml(
            r#"<opml><body><outline text="A &amp; B" xmlUrl="https://a?x=1&amp;y=2"/></body></opml>"#,
        )
        .unwrap();
        assert_eq!(g[0].feeds[0].title.as_deref(), Some("A & B"));
        assert_eq!(g[0].feeds[0].xml_url, "https://a?x=1&y=2");
    }

    #[test]
    fn parse_rejects_malformed_xml() {
        assert!(parse_opml("<opml><body><outline").is_err());
    }

    #[test]
    fn build_opml_empty_groups_is_valid_skeleton() {
        let s = build_opml(&[]);
        assert!(s.contains("<opml version=\"2.0\">"));
        assert!(s.contains("<body>"));
        assert!(s.contains("</body>"));
    }

    #[test]
    fn build_opml_top_level_feed() {
        let s = build_opml(&[ExportGroup {
            folder: None,
            feeds: vec![ExportFeed {
                title: Some("HN".into()),
                xml_url: "https://hn/rss".into(),
                html_url: None,
            }],
        }]);
        assert!(s.contains(r#"<outline type="rss" text="HN" title="HN" xmlUrl="https://hn/rss"/>"#));
    }

    #[test]
    fn build_opml_folder_with_feeds_nested() {
        let s = build_opml(&[ExportGroup {
            folder: Some("Tech".into()),
            feeds: vec![ExportFeed {
                title: Some("Rust".into()),
                xml_url: "https://r/feed".into(),
                html_url: None,
            }],
        }]);
        assert!(s.contains(r#"<outline text="Tech" title="Tech">"#));
        assert!(s.contains("xmlUrl=\"https://r/feed\""));
    }

    #[test]
    fn build_opml_escapes_attributes() {
        let s = build_opml(&[ExportGroup {
            folder: None,
            feeds: vec![ExportFeed {
                title: Some(r#"A & <B>"#.into()),
                xml_url: "https://a?x=1&y=2".into(),
                html_url: None,
            }],
        }]);
        assert!(s.contains("A &amp; &lt;B&gt;"));
        assert!(s.contains("x=1&amp;y=2"));
    }

    #[test]
    fn xml_escape_handles_all_five_entities() {
        assert_eq!(xml_escape(r#"& < > " '"#), "&amp; &lt; &gt; &quot; &apos;");
    }

    #[test]
    fn round_trip_build_then_parse_preserves_structure() {
        let groups = vec![
            ExportGroup {
                folder: None,
                feeds: vec![ExportFeed {
                    title: Some("Top".into()),
                    xml_url: "https://top".into(),
                    html_url: None,
                }],
            },
            ExportGroup {
                folder: Some("Tech".into()),
                feeds: vec![ExportFeed {
                    title: Some("Rust".into()),
                    xml_url: "https://r/feed".into(),
                    html_url: None,
                }],
            },
        ];
        let xml = build_opml(&groups);
        let parsed = parse_opml(&xml).unwrap();
        // unfiled top feed
        let top = parsed.iter().find(|g| g.folder.is_none()).unwrap();
        assert_eq!(top.feeds[0].xml_url, "https://top");
        // folder feed
        let tech = parsed
            .iter()
            .find(|g| g.folder.as_deref() == Some("Tech"))
            .unwrap();
        assert_eq!(tech.feeds[0].xml_url, "https://r/feed");
    }
}
