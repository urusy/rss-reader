//! Backup use cases: token gate, NDJSON export, idempotent import, and the
//! optional scheduled pg_dump. No LLM calls — the summary/translation cache is
//! merely data being preserved/restored (token defense).

use std::collections::HashMap;
use std::time::Duration;

use axum::body::Body;
use tokio::time::{interval, MissedTickBehavior};
use uuid::Uuid;

use super::domain::{
    check_version, parse_line, to_line, BackupRunRow, ImportSummary, Record, FORMAT_VERSION,
};
use super::repository;
use crate::shared::auth::constant_time_eq;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// BACKUP_TOKEN gate: unset → NotEnabled (503); set but mismatch/missing →
/// Validation (400). Checked before any work in every handler.
pub fn check_token(state: &AppState, presented: Option<&str>) -> AppResult<()> {
    let expected = state
        .config
        .backup_token
        .as_deref()
        .ok_or_else(|| AppError::NotEnabled("BACKUP_TOKEN is not set".into()))?;
    match presented {
        Some(t) if constant_time_eq(t.as_bytes(), expected.as_bytes()) => Ok(()),
        _ => Err(AppError::Validation(
            "invalid or missing backup token".into(),
        )),
    }
}

/// All data as an NDJSON Body, FK-dependency order (meta → folders → feeds →
/// articles → read_later). Memory-expanded (fine for a single-user home DB).
pub async fn export_ndjson(state: &AppState) -> AppResult<Body> {
    use serde_json::json;

    let folders = repository::all_folders(&state.db).await?;
    let feeds = repository::all_feeds(&state.db).await?;
    let articles = repository::all_articles(&state.db).await?;
    let read_later = repository::all_read_later(&state.db).await?;

    let mut buf = String::new();
    buf.push_str(&to_line(&json!({
        "v": FORMAT_VERSION,
        "kind": "meta",
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "app": "rss-reader",
    })));
    // serde_json::Error doesn't convert to AppError (error.rs stays unedited);
    // these DTOs never actually fail to serialize, so map to Other defensively.
    fn tagged<T: serde::Serialize>(row: &T, kind: &str) -> AppResult<serde_json::Value> {
        let mut v = serde_json::to_value(row)
            .map_err(|e| AppError::Other(anyhow::anyhow!("serialize {kind}: {e}")))?;
        v["kind"] = kind.into();
        Ok(v)
    }
    for f in &folders {
        buf.push_str(&to_line(&tagged(f, "folder")?));
    }
    for f in &feeds {
        buf.push_str(&to_line(&tagged(f, "feed")?));
    }
    for a in &articles {
        buf.push_str(&to_line(&tagged(a, "article")?));
    }
    for r in &read_later {
        buf.push_str(&to_line(&tagged(r, "read_later")?));
    }
    Ok(Body::from(buf))
}

/// Idempotently merge a full NDJSON body in one transaction. Remaps feed/article
/// ids (url-unique upserts may adopt an existing row's id).
pub async fn import_ndjson(state: &AppState, body: &str) -> AppResult<ImportSummary> {
    let mut summary = ImportSummary::default();
    let mut feed_map: HashMap<Uuid, Uuid> = HashMap::new();
    let mut article_map: HashMap<Uuid, Uuid> = HashMap::new();
    let mut version_checked = false;

    let mut tx = state.db.begin().await?;

    for (lineno, line) in body.lines().enumerate() {
        let rec = parse_line(line)
            .map_err(|e| AppError::Validation(format!("line {}: {e}", lineno + 1)))?;
        let Some(rec) = rec else { continue };
        match rec {
            Record::Meta(m) => {
                check_version(&m).map_err(AppError::Validation)?;
                version_checked = true;
            }
            Record::Folder(f) => {
                repository::upsert_folder(&mut tx, &f).await?;
                summary.folders += 1;
            }
            Record::Feed(f) => {
                let old = f.id;
                let actual = repository::upsert_feed(&mut tx, &f).await?;
                feed_map.insert(old, actual);
                summary.feeds += 1;
            }
            Record::Article(a) => {
                let mapped_feed = feed_map.get(&a.feed_id).copied().unwrap_or(a.feed_id);
                let old = a.id;
                let actual = repository::upsert_article(&mut tx, &a, mapped_feed).await?;
                article_map.insert(old, actual);
                summary.articles += 1;
            }
            Record::ReadLater(r) => {
                let mapped = article_map
                    .get(&r.article_id)
                    .copied()
                    .unwrap_or(r.article_id);
                repository::upsert_read_later(&mut tx, &r, mapped).await?;
                summary.read_later += 1;
            }
            Record::Unknown => summary.skipped += 1,
        }
    }

    if !version_checked {
        return Err(AppError::Validation(
            "missing meta header (first line must be kind=meta)".into(),
        ));
    }

    tx.commit().await?;
    Ok(summary)
}

pub async fn list_runs(state: &AppState) -> AppResult<Vec<BackupRunRow>> {
    repository::recent_runs(&state.db, 20).await
}

/// Spawn the optional pg_dump scheduler when BACKUP_DIR + interval are both set.
/// Mirrors shared/scheduler.rs (drop first tick / Skip / tracing).
pub fn spawn_pgdump_scheduler(state: AppState) {
    let (Some(dir), Some(secs)) = (
        state.config.backup_dir.clone(),
        state.config.backup_pgdump_interval_secs,
    ) else {
        tracing::info!("pg_dump scheduler disabled (BACKUP_DIR / interval not set)");
        return;
    };
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        ticker.tick().await; // drop the immediate first tick
        loop {
            ticker.tick().await;
            if let Err(e) = run_pgdump(&state, &dir).await {
                tracing::error!(error = %e, "scheduled pg_dump failed");
            }
        }
    });
}

async fn run_pgdump(state: &AppState, dir: &str) -> AppResult<()> {
    let id = Uuid::new_v4();
    let path = format!(
        "{dir}/rss-{}.sql",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    );
    repository::insert_run_started(&state.db, id).await?;

    let out = tokio::process::Command::new("pg_dump")
        .arg(&state.config.database_url)
        .arg("-f")
        .arg(&path)
        .output()
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!("spawn pg_dump: {e}")))?;

    if out.status.success() {
        let bytes = tokio::fs::metadata(&path)
            .await
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        repository::finish_run_ok(&state.db, id, &path, bytes).await?;
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        repository::finish_run_err(&state.db, id, &err).await?;
        Err(AppError::Other(anyhow::anyhow!("pg_dump failed: {err}")))
    }
}
