use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{Article, ArticleId};
use crate::features::feeds::domain::FeedId;
use crate::features::folders::domain::FolderId;
use crate::shared::error::{AppError, AppResult};

pub async fn list(
    pool: &PgPool,
    feed_id: Option<FeedId>,
    unread_only: bool,
    folder_id: Option<FolderId>,
    unclassified: bool,
    include_muted: bool,
) -> AppResult<Vec<Article>> {
    let rows = sqlx::query_as::<_, Article>(
        r#"SELECT * FROM articles
           WHERE ($1::uuid IS NULL OR feed_id = $1)
             AND ($2 = false OR is_read = false)
             AND ($3::uuid IS NULL
                  OR feed_id IN (SELECT id FROM feeds WHERE folder_id = $3))
             AND ($4 = false
                  OR feed_id IN (SELECT id FROM feeds WHERE folder_id IS NULL))
             AND ($5 = true OR muted_at IS NULL)
           ORDER BY published_at DESC NULLS LAST, created_at DESC
           LIMIT 200"#,
    )
    .bind(feed_id.map(|f| f.0))
    .bind(unread_only)
    .bind(folder_id.map(|f| f.0))
    .bind(unclassified)
    .bind(include_muted)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// is_read=false の記事を一括で既読にする。
/// feed_id=None なら全フィード、Some(id) ならそのフィードのみ。
/// 既に既読の行は対象外なので、戻り値（rows_affected）= 今回新たに既読化した件数。
pub async fn mark_all_read(pool: &PgPool, feed_id: Option<FeedId>) -> AppResult<u64> {
    let res = sqlx::query(
        r#"UPDATE articles
           SET is_read = true
           WHERE is_read = false
             AND ($1::uuid IS NULL OR feed_id = $1)"#,
    )
    .bind(feed_id.map(|f| f.0))
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn get(pool: &PgPool, id: ArticleId) -> AppResult<Article> {
    sqlx::query_as::<_, Article>("SELECT * FROM articles WHERE id = $1")
        .bind(id.0)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn set_read(pool: &PgPool, id: ArticleId, read: bool) -> AppResult<()> {
    let res = sqlx::query("UPDATE articles SET is_read = $2 WHERE id = $1")
        .bind(id.0)
        .bind(read)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn save_summary(
    pool: &PgPool,
    id: ArticleId,
    summary: &str,
    lang: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE articles
           SET summary = $2, summary_lang = $3, processed_at = now()
           WHERE id = $1"#,
    )
    .bind(id.0)
    .bind(summary)
    .bind(lang)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn save_translation(
    pool: &PgPool,
    id: ArticleId,
    translation: &str,
    lang: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE articles
           SET translation = $2, translation_lang = $3, processed_at = now()
           WHERE id = $1"#,
    )
    .bind(id.0)
    .bind(translation)
    .bind(lang)
    .execute(pool)
    .await?;
    Ok(())
}

/// Clear a cached summary (set it and its language back to NULL) so the user
/// can discard a stale/garbled result. NotFound if the article doesn't exist.
pub async fn clear_summary(pool: &PgPool, id: ArticleId) -> AppResult<()> {
    let res = sqlx::query("UPDATE articles SET summary = NULL, summary_lang = NULL WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Clear a cached translation (mirrors `clear_summary`).
pub async fn clear_translation(pool: &PgPool, id: ArticleId) -> AppResult<()> {
    let res = sqlx::query(
        "UPDATE articles SET translation = NULL, translation_lang = NULL WHERE id = $1",
    )
    .bind(id.0)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Cache the extracted full body. Only called on a successful extraction; on
/// failure the caller leaves full_content NULL so AI/display fall back to
/// `content`. Called from the `extraction` slice (same-aggregate write, mirrors
/// how `feeds` writes articles via `upsert_batch`).
pub async fn save_full_content(pool: &PgPool, id: ArticleId, full_content: &str) -> AppResult<()> {
    let res = sqlx::query(
        r#"UPDATE articles
           SET full_content = $2, extracted_at = now()
           WHERE id = $1"#,
    )
    .bind(id.0)
    .bind(full_content)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// クロール1回分のエントリ（`upsert_batch` の入力）。
pub struct NewArticle {
    pub url: String,
    pub title: String,
    pub content: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub author: Option<String>,
}

/// フィード1本分のエントリを1クエリで一括 upsert する。従来は記事ごとに
/// INSERT + author UPDATE + id SELECT の直列 3 クエリで、エントリの多い
/// フィードの取込みが遅かった（フィード追加の応答遅延調査の続き）。
/// - ON CONFLICT(url) は従来どおり title/content を更新
/// - author は既存値を尊重し NULL のときだけ埋める（旧 `set_author` と同じ意味論）
/// - バッチ内の同一 url は後勝ちで間引く（従来の逐次ループと同じ最終状態。
///   ON CONFLICT は同一文内で同じ行への二重更新を許さないため必須）
/// - 挿入/更新した行の (id, url) を返す（クロール時抽出が id を使う）
pub async fn upsert_batch(
    pool: &PgPool,
    feed_id: FeedId,
    items: &[NewArticle],
) -> AppResult<Vec<(ArticleId, String)>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    // 後勝ち dedup: url → 最後に現れた index を採用し、元の順序で並べ直す。
    let mut last = std::collections::HashMap::new();
    for (i, it) in items.iter().enumerate() {
        last.insert(it.url.as_str(), i);
    }
    let mut keep: Vec<usize> = last.into_values().collect();
    keep.sort_unstable();

    let mut urls = Vec::with_capacity(keep.len());
    let mut titles = Vec::with_capacity(keep.len());
    let mut contents = Vec::with_capacity(keep.len());
    let mut published = Vec::with_capacity(keep.len());
    let mut authors = Vec::with_capacity(keep.len());
    for i in keep {
        let it = &items[i];
        urls.push(it.url.clone());
        titles.push(it.title.clone());
        contents.push(it.content.clone());
        published.push(it.published_at);
        authors.push(it.author.clone());
    }

    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        r#"INSERT INTO articles (id, feed_id, url, title, content, published_at, author)
           SELECT gen_random_uuid(), $1, t.url, t.title, t.content, t.published_at, t.author
           FROM UNNEST($2::text[], $3::text[], $4::text[], $5::timestamptz[], $6::text[])
                AS t(url, title, content, published_at, author)
           ON CONFLICT (url) DO UPDATE
             SET title = EXCLUDED.title,
                 content = EXCLUDED.content,
                 author = COALESCE(articles.author, EXCLUDED.author)
           RETURNING id, url"#,
    )
    .bind(feed_id.0)
    .bind(&urls)
    .bind(&titles)
    .bind(&contents)
    .bind(&published)
    .bind(&authors)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, url)| (ArticleId(id), url))
        .collect())
}

#[cfg(test)]
mod tests {
    //! Round-trip tests against a real DB. Run with:
    //!   DATABASE_URL=... cargo test -- --ignored
    //! Requires migrations applied (`just dev-db` / `just migrate`).
    use super::*;
    use crate::features::feeds::domain::FeedUrl;
    use crate::features::feeds::repository as feeds_repo;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        PgPool::connect(&url).await.expect("connect")
    }

    async fn a_feed(pool: &PgPool) -> FeedId {
        // Unique url per run to avoid clashing with other tests / existing rows.
        let raw = format!("https://example.com/extraction-test/{}", Uuid::new_v4());
        let url = FeedUrl::parse(&raw).expect("feed url");
        let feed = feeds_repo::insert(pool, url.as_str())
            .await
            .expect("insert feed");
        FeedId(feed.id.0)
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn save_full_content_sets_full_content_and_extracted_at() {
        let pool = pool().await;
        let feed_id = a_feed(&pool).await;
        let url = format!("https://example.com/post/{}", Uuid::new_v4());
        let stored = upsert_batch(&pool, feed_id, &[item(&url, "t", None)])
            .await
            .unwrap();
        let id = stored[0].0;

        let before = get(&pool, id).await.unwrap();
        assert!(before.full_content.is_none());
        assert!(before.extracted_at.is_none());

        save_full_content(&pool, id, "<p>extracted body</p>")
            .await
            .unwrap();

        let after = get(&pool, id).await.unwrap();
        assert_eq!(after.full_content.as_deref(), Some("<p>extracted body</p>"));
        assert!(after.extracted_at.is_some());
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn save_full_content_missing_id_is_not_found() {
        let pool = pool().await;
        let res = save_full_content(&pool, ArticleId(Uuid::new_v4()), "x").await;
        assert!(matches!(res, Err(AppError::NotFound)));
    }

    async fn author_of(pool: &PgPool, url: &str) -> Option<String> {
        let (a,): (Option<String>,) = sqlx::query_as("SELECT author FROM articles WHERE url = $1")
            .bind(url)
            .fetch_one(pool)
            .await
            .expect("author query");
        a
    }

    fn item(url: &str, title: &str, author: Option<&str>) -> NewArticle {
        NewArticle {
            url: url.to_string(),
            title: title.to_string(),
            content: format!("content of {title}"),
            published_at: None,
            author: author.map(String::from),
        }
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn upsert_batch_inserts_updates_and_keeps_existing_author() {
        let pool = pool().await;
        let feed_id = a_feed(&pool).await;
        let u1 = format!("https://example.com/post/{}", Uuid::new_v4());
        let u2 = format!("https://example.com/post/{}", Uuid::new_v4());

        // 新規: u1 はバッチ内重複（後勝ち）・author 付き、u2 は author 無し。
        let stored = upsert_batch(
            &pool,
            feed_id,
            &[
                item(&u1, "old", Some("alice")),
                item(&u1, "t1", Some("alice")), // 同一 url 重複 → 後勝ち
                item(&u2, "t2", None),
            ],
        )
        .await
        .unwrap();
        assert_eq!(stored.len(), 2); // 重複は間引かれ、(id, url) が返る
        assert!(stored.iter().any(|(_, u)| u == &u1));

        let id1 = stored.iter().find(|(_, u)| u == &u1).unwrap().0;
        let a1 = get(&pool, id1).await.unwrap();
        assert_eq!(a1.title, "t1"); // 後勝ち
        assert_eq!(author_of(&pool, &u1).await.as_deref(), Some("alice"));

        // 再クロール相当: title/content は更新、author は既存優先（NULL のみ埋まる）。
        upsert_batch(
            &pool,
            feed_id,
            &[
                item(&u1, "t1v2", Some("bob")),
                item(&u2, "t2v2", Some("carol")),
            ],
        )
        .await
        .unwrap();
        let a1 = get(&pool, id1).await.unwrap();
        assert_eq!(a1.title, "t1v2");
        assert_eq!(author_of(&pool, &u1).await.as_deref(), Some("alice")); // 既存優先
        assert_eq!(author_of(&pool, &u2).await.as_deref(), Some("carol")); // NULL→埋まる
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn upsert_batch_empty_is_noop() {
        let pool = pool().await;
        let feed_id = a_feed(&pool).await;
        assert!(upsert_batch(&pool, feed_id, &[]).await.unwrap().is_empty());
    }
}
