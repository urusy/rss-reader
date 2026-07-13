//! Use cases for the saved-pages slice: 保存（201 即返し + 背景抽出）・一覧・
//! アーカイブ・削除。

use super::domain::SavedUrl;
use super::repository;
use crate::features::articles::domain::{Article, ArticleId};
use crate::features::articles::repository as articles_repo;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// 一覧の状態フィルタ。handler の Query から serde で入る。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SavedState {
    Inbox,
    Archived,
    All,
}

impl SavedState {
    pub fn as_sql(&self) -> &'static str {
        match self {
            SavedState::Inbox => "inbox",
            SavedState::Archived => "archived",
            SavedState::All => "all",
        }
    }
}

/// URL を保存する。既存 URL なら既存記事にマークを付けるだけ（ブックマーク化・
/// 再保存は inbox 復帰）。本文抽出は背景で行い、応答を待たせない
/// （feeds::create_feed の 201 即返しパターン）。
pub async fn save_url(state: &AppState, raw_url: &str) -> AppResult<Article> {
    let url = SavedUrl::parse(raw_url).map_err(AppError::Validation)?;
    let (id, needs_extract) = repository::ensure_article(&state.db, url.as_str()).await?;
    repository::mark_saved(&state.db, id).await?;

    // extracted_at が無い行だけ抽出を仕掛ける。過去に抽出失敗した行の再保存も
    // この一条件で自然に再試行になる。抽出自体は extraction スライスが
    // saved 対応（content+title も更新）を持つ。
    if needs_extract {
        let spawned = state.clone();
        tokio::spawn(async move {
            crate::features::extraction::service::extract_best_effort(&spawned, id).await;
        });
    }

    articles_repo::get(&state.db, id).await
}

pub async fn list_saved(
    state: &AppState,
    filter: SavedState,
    unread_only: bool,
) -> AppResult<Vec<Article>> {
    repository::list(&state.db, filter.as_sql(), unread_only).await
}

pub async fn set_archived(state: &AppState, id: ArticleId, archived: bool) -> AppResult<()> {
    repository::set_archived(&state.db, id, archived).await
}

pub async fn delete_saved(state: &AppState, id: ArticleId) -> AppResult<()> {
    repository::delete(&state.db, id).await
}
