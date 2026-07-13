//! Data access for the saved-pages slice. Plain sqlx functions — no trait
//! abstraction (feeds/repository.rs と同じ流儀)。

use sqlx::PgPool;
use uuid::Uuid;

use super::domain::SAVED_FEED_ID;
use crate::features::articles::domain::{Article, ArticleId};
use crate::shared::error::{AppError, AppResult};

/// URL の記事行を確保して (id, 抽出要否) を返す。
/// - 新規 URL → 合成フィード配下に行を作る（content='' は抽出が埋める）
/// - 既存 URL（RSS 記事 or 再保存）→ 既存行をそのまま使う（ブックマーク化）
pub async fn ensure_article(pool: &PgPool, url: &str) -> AppResult<(ArticleId, bool)> {
    // INSERT と SELECT を 1 往復にする upsert-select（ON CONFLICT DO NOTHING は
    // 行を返さないため、既存行は本体テーブルから拾う）。
    let (id, extracted_at): (Uuid, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        r#"WITH ins AS (
               INSERT INTO articles (id, feed_id, url, title, content)
               VALUES (gen_random_uuid(), $1, $2, $2, '')
               ON CONFLICT (url) DO NOTHING
               RETURNING id, extracted_at
           )
           SELECT id, extracted_at FROM ins
           UNION ALL
           SELECT id, extracted_at FROM articles WHERE url = $2
           LIMIT 1"#,
    )
    .bind(SAVED_FEED_ID)
    .bind(url)
    .fetch_one(pool)
    .await?;
    Ok((ArticleId(id), extracted_at.is_none()))
}

/// 保存マークを付ける。再保存は inbox へ戻す（Pocket 意味論）。
pub async fn mark_saved(pool: &PgPool, id: ArticleId) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO saved_pages (article_id) VALUES ($1)
           ON CONFLICT (article_id) DO UPDATE
             SET archived_at = NULL, saved_at = now()"#,
    )
    .bind(id.0)
    .execute(pool)
    .await?;
    Ok(())
}

/// 一覧。state: inbox（未アーカイブ）/ archived / all。unread=true で未読のみ。
pub async fn list(pool: &PgPool, state: &str, unread_only: bool) -> AppResult<Vec<Article>> {
    let rows = sqlx::query_as::<_, Article>(
        r#"SELECT a.* FROM articles a
           JOIN saved_pages s ON s.article_id = a.id
           WHERE CASE $1
                   WHEN 'archived' THEN s.archived_at IS NOT NULL
                   WHEN 'all'      THEN TRUE
                   ELSE                 s.archived_at IS NULL
                 END
             AND (NOT $2 OR a.is_read = FALSE)
           ORDER BY s.saved_at DESC
           LIMIT 200"#,
    )
    .bind(state)
    .bind(unread_only)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// アーカイブ / 復元。対象が保存ページでなければ NotFound。
pub async fn set_archived(pool: &PgPool, id: ArticleId, archived: bool) -> AppResult<()> {
    let res = sqlx::query(
        r#"UPDATE saved_pages
           SET archived_at = CASE WHEN $2 THEN now() ELSE NULL END
           WHERE article_id = $1"#,
    )
    .bind(id.0)
    .bind(archived)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// 削除。合成フィード配下の保存ページは記事ごと DELETE（CASCADE でスター・
/// タグ・ハイライト等も消え、url UNIQUE が空くので削除→再保存が成立する）。
/// RSS 記事のブックマークなら saved_pages 行だけ外す（記事本体は残す）。
pub async fn delete(pool: &PgPool, id: ArticleId) -> AppResult<()> {
    let feed_id: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT a.feed_id FROM articles a
           JOIN saved_pages s ON s.article_id = a.id
           WHERE a.id = $1"#,
    )
    .bind(id.0)
    .fetch_optional(pool)
    .await?;
    match feed_id {
        None => Err(AppError::NotFound),
        Some(f) if f == SAVED_FEED_ID => {
            sqlx::query("DELETE FROM articles WHERE id = $1")
                .bind(id.0)
                .execute(pool)
                .await?;
            Ok(())
        }
        Some(_) => {
            sqlx::query("DELETE FROM saved_pages WHERE article_id = $1")
                .bind(id.0)
                .execute(pool)
                .await?;
            Ok(())
        }
    }
}

/// 抽出結果の保存（保存ページ専用）。RSS 記事と違い content が正典（検索
/// pg_trgm 索引・LLM 入力・digest snippet は content を読む）ため、full_content
/// と両方に書き、タイトルも確定させる。
pub async fn save_extracted(
    pool: &PgPool,
    id: ArticleId,
    title: Option<&str>,
    body: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE articles
           SET content = $2,
               full_content = $2,
               title = COALESCE($3, title),
               extracted_at = now()
           WHERE id = $1"#,
    )
    .bind(id.0)
    .bind(body)
    .bind(title)
    .execute(pool)
    .await?;
    Ok(())
}

/// 本文が薄すぎて抽出不成立（TooThin）でも、タイトルだけは反映する。
/// extracted_at は立てない（再保存・force 抽出での再試行を生かす）。
pub async fn save_title(pool: &PgPool, id: ArticleId, title: &str) -> AppResult<()> {
    sqlx::query("UPDATE articles SET title = $2 WHERE id = $1")
        .bind(id.0)
        .bind(title)
        .execute(pool)
        .await?;
    Ok(())
}
