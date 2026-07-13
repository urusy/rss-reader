//! Data access for the feeds slice. Plain sqlx functions — no trait abstraction,
//! because there is no realistic second implementation of "store a feed in our DB".

use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{Feed, FeedId};
use crate::features::folders::domain::FolderId;
use crate::shared::error::{AppError, AppResult};

pub async fn insert(pool: &PgPool, url: &str) -> AppResult<Feed> {
    let row = sqlx::query_as::<_, Feed>(
        r#"INSERT INTO feeds (id, url) VALUES ($1, $2)
           ON CONFLICT (url) DO UPDATE SET url = EXCLUDED.url
           RETURNING id, url, title, folder_id, created_at, last_fetched_at, priority, extract_full_content"#,
    )
    .bind(Uuid::new_v4())
    .bind(url)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get(pool: &PgPool, id: FeedId) -> AppResult<Feed> {
    sqlx::query_as::<_, Feed>(
        r#"SELECT id, url, title, folder_id, created_at, last_fetched_at, priority, extract_full_content
           FROM feeds WHERE id = $1"#,
    )
    .bind(id.0)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// RSS フィードのみ列挙する。保存ページの合成フィード（kind='saved'）は
/// サイドバー・manage・OPML export・スケジューラのクロール対象から除外
/// （この 1 箇所が 4 接点をまとめてカバーする要衝）。
pub async fn list_all(pool: &PgPool) -> AppResult<Vec<Feed>> {
    let rows = sqlx::query_as::<_, Feed>(
        r#"SELECT id, url, title, folder_id, created_at, last_fetched_at, priority, extract_full_content
           FROM feeds WHERE kind = 'rss' ORDER BY created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// kind='rss' ガード: 合成フィードを誤って DELETE すると CASCADE で保存ページが
/// 全滅するため、API 経由では構造的に消せないようにする。
pub async fn delete(pool: &PgPool, id: FeedId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM feeds WHERE id = $1 AND kind = 'rss'")
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn touch_fetched(pool: &PgPool, id: FeedId, title: Option<&str>) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE feeds
           SET last_fetched_at = now(),
               title = COALESCE($2, title)
           WHERE id = $1"#,
    )
    .bind(id.0)
    .bind(title)
    .execute(pool)
    .await?;
    Ok(())
}

/// PATCH: title / folder_id をそれぞれ「触る/触らない」で部分更新する。
/// folder_id の三値: 外側 None=未指定(据え置き) / Some(None)=未分類化(NULL) / Some(Some(x))=割当。
pub async fn update(
    pool: &PgPool,
    id: FeedId,
    title: Option<&str>,
    folder_id: Option<Option<FolderId>>,
    priority: Option<i16>,
    extract_full_content: Option<bool>,
) -> AppResult<Feed> {
    let touch_folder = folder_id.is_some();
    let folder_val: Option<Uuid> = folder_id.flatten().map(|f| f.0);
    sqlx::query_as::<_, Feed>(
        r#"UPDATE feeds
           SET title     = CASE WHEN $2 THEN $3 ELSE title     END,
               folder_id = CASE WHEN $4 THEN $5 ELSE folder_id END,
               priority  = CASE WHEN $6 THEN $7 ELSE priority  END,
               extract_full_content = CASE WHEN $8 THEN $9 ELSE extract_full_content END
           WHERE id = $1
           RETURNING id, url, title, folder_id, created_at, last_fetched_at, priority, extract_full_content"#,
    )
    .bind(id.0) // $1 :: uuid（WHERE id = $1）
    .bind(title.is_some()) // $2 :: bool
    .bind(title) // $3 :: text（CASE 結果型が title 列=TEXT から解決）
    .bind(touch_folder) // $4 :: bool
    .bind(folder_val) // $5 :: uuid（CASE 結果型が folder_id 列=UUID から解決）
    .bind(priority.is_some()) // $6 :: bool
    .bind(priority) // $7 :: smallint（CASE 結果型が priority 列=SMALLINT から解決）
    .bind(extract_full_content.is_some()) // $8 :: bool
    .bind(extract_full_content) // $9 :: bool（CASE 結果型が extract_full_content 列=BOOLEAN から解決）
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// 割当先フォルダの存在チェック（advisory）。FK が本命のガードで、
/// これは 23503(FK 違反=500) を Validation(400) に整形するためだけのもの。
pub async fn folder_exists(pool: &PgPool, id: FolderId) -> AppResult<bool> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM folders WHERE id = $1)")
        .bind(id.0)
        .fetch_one(pool)
        .await?;
    Ok(exists)
}
