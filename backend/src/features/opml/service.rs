//! OPML import/export: parse to/from the pure domain functions, then reflect
//! into the existing feeds/folders repositories (no new SQL for writes, no new
//! abstraction boundary).

use std::collections::HashMap;

use super::domain::{build_opml, parse_opml, ExportFeed, ExportGroup, ImportSummary};
use super::repository;
use uuid::Uuid;

use crate::features::feeds;
use crate::features::feeds::domain::{FeedId, FeedUrl};
use crate::features::folders;
use crate::features::folders::domain::{FolderId, FolderName};
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// Import OPML; create folders/feeds idempotently (folder by name, feed by url).
pub async fn import_opml(state: &AppState, xml: &str) -> AppResult<ImportSummary> {
    let groups = parse_opml(xml).map_err(AppError::Validation)?;

    let mut folder_map: HashMap<String, FolderId> = repository::list_folder_names(&state.db)
        .await?
        .into_iter()
        .map(|r| (r.name, r.id))
        .collect();

    let mut imported_feeds = 0usize;
    let mut imported_folders = 0usize;
    let mut skipped = 0usize;

    for group in groups {
        let folder_id: Option<FolderId> = match group.folder {
            None => None,
            Some(raw_name) => match FolderName::parse(&raw_name) {
                Ok(valid) => {
                    let key = valid.as_str().to_string();
                    if let Some(id) = folder_map.get(&key) {
                        Some(*id)
                    } else {
                        let folder = folders::repository::insert(&state.db, &key).await?;
                        imported_folders += 1;
                        folder_map.insert(key, folder.id);
                        Some(folder.id)
                    }
                }
                Err(_) => None, // invalid folder name → demote to unfiled
            },
        };

        for pf in group.feeds {
            let url = match FeedUrl::parse(&pf.xml_url) {
                Ok(u) => u,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let feed = feeds::repository::insert(&state.db, url.as_str()).await?;
            imported_feeds += 1;

            if let Some(fid) = folder_id {
                feeds::repository::update(&state.db, FeedId(feed.id.0), None, Some(Some(fid)), None)
                    .await?;
            }
        }
    }

    Ok(ImportSummary {
        imported_feeds,
        imported_folders,
        skipped,
    })
}

/// Export current feeds + folders as OPML XML.
pub async fn export_opml(state: &AppState) -> AppResult<String> {
    let folder_list = folders::repository::list_all(&state.db).await?;
    let feed_list = feeds::repository::list_all(&state.db).await?;

    // Key by raw Uuid (FolderId doesn't derive Hash; avoid touching that slice).
    let mut by_folder: HashMap<Option<Uuid>, Vec<ExportFeed>> = HashMap::new();
    for f in feed_list {
        let ef = ExportFeed {
            title: f.title.clone(),
            xml_url: f.url.clone(),
            html_url: None,
        };
        by_folder
            .entry(f.folder_id.map(|x| x.0))
            .or_default()
            .push(ef);
    }

    let mut groups: Vec<ExportGroup> = Vec::new();
    if let Some(unfiled) = by_folder.remove(&None) {
        if !unfiled.is_empty() {
            groups.push(ExportGroup {
                folder: None,
                feeds: unfiled,
            });
        }
    }
    for folder in folder_list {
        let feeds = by_folder.remove(&Some(folder.id.0)).unwrap_or_default();
        groups.push(ExportGroup {
            folder: Some(folder.name),
            feeds,
        });
    }

    Ok(build_opml(&groups))
}
