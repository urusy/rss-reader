//! Backup DTOs + pure NDJSON line logic (no I/O → unit-tested offline).
//! Backup-specific row types keep the wire format decoupled from domain types
//! (whose ids are newtypes); these mirror the DB columns as raw Uuid/values.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// First NDJSON line. Used for import compatibility checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupMeta {
    pub v: u32,
    #[serde(default)]
    pub exported_at: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
}

/// Current format version. Import rejects anything newer.
pub const FORMAT_VERSION: u32 = 1;

/// A parsed NDJSON line. Unknown kinds fall to `Unknown` (forward-compatible).
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Meta(BackupMeta),
    Folder(FolderRow),
    Feed(FeedRow),
    Article(ArticleRow),
    ReadLater(ReadLaterRow),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct FolderRow {
    pub id: Uuid,
    pub name: String,
    pub position: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeedRow {
    pub id: Uuid,
    pub url: String,
    pub title: Option<String>,
    pub folder_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ArticleRow {
    pub id: Uuid,
    pub feed_id: Uuid,
    pub url: String,
    pub title: String,
    pub content: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_read: bool,
    pub summary: Option<String>,
    pub summary_lang: Option<String>,
    pub translation: Option<String>,
    pub translation_lang: Option<String>,
    pub processed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReadLaterRow {
    pub article_id: Uuid,
    pub status: String,
    pub instapaper_added_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Row for the optional pg_dump audit log.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BackupRunRow {
    pub id: Uuid,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
    pub file_path: Option<String>,
    pub byte_size: Option<i64>,
    pub error: Option<String>,
}

/// Import counts (response body).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportSummary {
    pub folders: u64,
    pub feeds: u64,
    pub articles: u64,
    pub read_later: u64,
    pub skipped: u64,
}

/// One NDJSON line → Record. Empty line → Ok(None). Bad JSON / missing kind →
/// per the rules below. Unknown kind → Record::Unknown (forward-compatible).
pub fn parse_line(line: &str) -> Result<Option<Record>, String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let v: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("invalid json line: {e}"))?;
    let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
    let rec = match kind {
        "meta" => Record::Meta(serde_json::from_value(v).map_err(|e| format!("bad meta: {e}"))?),
        "folder" => {
            Record::Folder(serde_json::from_value(v).map_err(|e| format!("bad folder: {e}"))?)
        }
        "feed" => Record::Feed(serde_json::from_value(v).map_err(|e| format!("bad feed: {e}"))?),
        "article" => {
            Record::Article(serde_json::from_value(v).map_err(|e| format!("bad article: {e}"))?)
        }
        "read_later" => Record::ReadLater(
            serde_json::from_value(v).map_err(|e| format!("bad read_later: {e}"))?,
        ),
        _ => Record::Unknown,
    };
    Ok(Some(rec))
}

/// Import compatibility gate: reject files newer than FORMAT_VERSION.
pub fn check_version(meta: &BackupMeta) -> Result<(), String> {
    if meta.v > FORMAT_VERSION {
        return Err(format!(
            "backup format v{} is newer than supported v{}",
            meta.v, FORMAT_VERSION
        ));
    }
    Ok(())
}

/// Serialize one record value to an NDJSON line (newline-terminated).
pub fn to_line(value: &serde_json::Value) -> String {
    let mut s = value.to_string();
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_line_empty_returns_none() {
        assert_eq!(parse_line("").unwrap(), None);
        assert_eq!(parse_line("   ").unwrap(), None);
    }

    #[test]
    fn parse_line_meta() {
        let r = parse_line(r#"{"kind":"meta","v":1,"app":"rss-reader"}"#)
            .unwrap()
            .unwrap();
        match r {
            Record::Meta(m) => assert_eq!(m.v, 1),
            _ => panic!("expected meta"),
        }
    }

    #[test]
    fn parse_line_feed() {
        let line = r#"{"kind":"feed","id":"11111111-1111-1111-1111-111111111111","url":"https://a/f","title":"A","folder_id":null,"created_at":"2026-06-30T00:00:00Z","last_fetched_at":null}"#;
        match parse_line(line).unwrap().unwrap() {
            Record::Feed(f) => {
                assert_eq!(f.url, "https://a/f");
                assert_eq!(f.title.as_deref(), Some("A"));
                assert!(f.folder_id.is_none());
            }
            _ => panic!("expected feed"),
        }
    }

    #[test]
    fn parse_line_article_with_nulls() {
        let line = r#"{"kind":"article","id":"22222222-2222-2222-2222-222222222222","feed_id":"11111111-1111-1111-1111-111111111111","url":"https://a/p","title":"t","content":"c","published_at":null,"is_read":false,"summary":null,"summary_lang":null,"translation":null,"translation_lang":null,"processed_at":null,"created_at":"2026-06-30T00:00:00Z"}"#;
        match parse_line(line).unwrap().unwrap() {
            Record::Article(a) => {
                assert!(a.summary.is_none());
                assert!(a.translation.is_none());
                assert!(!a.is_read);
            }
            _ => panic!("expected article"),
        }
    }

    #[test]
    fn parse_line_read_later() {
        let line = r#"{"kind":"read_later","article_id":"22222222-2222-2222-2222-222222222222","status":"added","instapaper_added_at":null,"last_error":null,"created_at":"2026-06-30T00:00:00Z","updated_at":"2026-06-30T00:00:00Z"}"#;
        assert!(matches!(
            parse_line(line).unwrap().unwrap(),
            Record::ReadLater(_)
        ));
    }

    #[test]
    fn parse_line_unknown_kind_is_unknown() {
        assert_eq!(
            parse_line(r#"{"kind":"tag","name":"x"}"#).unwrap().unwrap(),
            Record::Unknown
        );
    }

    #[test]
    fn parse_line_invalid_json_errs() {
        assert!(parse_line("{not json").is_err());
    }

    #[test]
    fn parse_line_missing_kind_is_unknown() {
        assert_eq!(
            parse_line(r#"{"foo":"bar"}"#).unwrap().unwrap(),
            Record::Unknown
        );
    }

    #[test]
    fn check_version_accepts_current_and_older() {
        assert!(check_version(&BackupMeta {
            v: FORMAT_VERSION,
            exported_at: None,
            app: None
        })
        .is_ok());
        assert!(check_version(&BackupMeta {
            v: 0,
            exported_at: None,
            app: None
        })
        .is_ok());
    }

    #[test]
    fn check_version_rejects_newer() {
        assert!(check_version(&BackupMeta {
            v: FORMAT_VERSION + 1,
            exported_at: None,
            app: None
        })
        .is_err());
    }

    #[test]
    fn to_line_appends_newline() {
        assert!(to_line(&json!({"a":1})).ends_with('\n'));
    }

    #[test]
    fn roundtrip_feed_row_serde() {
        let f = FeedRow {
            id: Uuid::nil(),
            url: "https://a/f".into(),
            title: Some("A".into()),
            folder_id: None,
            created_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            last_fetched_at: None,
        };
        let mut v = serde_json::to_value(&f).unwrap();
        v["kind"] = "feed".into();
        let line = to_line(&v);
        match parse_line(&line).unwrap().unwrap() {
            Record::Feed(g) => assert_eq!(f, g),
            _ => panic!("expected feed"),
        }
    }
}
