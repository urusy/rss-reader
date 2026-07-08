//! sync スライスのデータ access。
//!
//! 読み取りは instapaper の `ArticleRef` 前例にならい素の `Uuid`/`i64` を bind する
//! read-only 射影（他スライスの domain 型に依存しない）。書き込みは 2 系統:
//! 既読 = short_id キーの一括 UPDATE（本ファイル所有）、スター = 所有スライス
//! `annotations::repository` の関数を service 層から呼ぶ（ここには置かない）。

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::shared::error::AppResult;

// ---- 購読・フォルダ（読み取り射影） -------------------------------------------

#[derive(Debug, sqlx::FromRow)]
pub struct SubscriptionRow {
    pub id: Uuid,
    pub url: String,
    pub title: Option<String>,
    pub folder_name: Option<String>,
}

pub async fn list_subscriptions(pool: &PgPool) -> AppResult<Vec<SubscriptionRow>> {
    let rows = sqlx::query_as::<_, SubscriptionRow>(
        r#"SELECT f.id, f.url, f.title, fo.name AS folder_name
           FROM feeds f LEFT JOIN folders fo ON fo.id = f.folder_id
           ORDER BY f.created_at"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_folder_names(pool: &PgPool) -> AppResult<Vec<String>> {
    let names = sqlx::query_scalar("SELECT name FROM folders ORDER BY position, created_at")
        .fetch_all(pool)
        .await?;
    Ok(names)
}

pub async fn folder_id_by_name(pool: &PgPool, name: &str) -> AppResult<Option<Uuid>> {
    let id = sqlx::query_scalar("SELECT id FROM folders WHERE name = $1 LIMIT 1")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    Ok(id)
}

// ---- ストリーム抽出 ------------------------------------------------------------

/// service が StreamId + params から組み立てる型付きフィルタ。
/// `limit` は clamp 済み n + 1（n+1 フェッチ → `domain::paginate`）。
#[derive(Debug, Clone, Default)]
pub struct StreamFilter {
    pub feed_id: Option<Uuid>,
    pub folder_name: Option<String>,
    pub starred_only: bool,
    /// xt=read
    pub unread_only: bool,
    /// s=.../read
    pub read_only: bool,
    /// ot → created_at >=（クロール時刻意味論 — published_at ではない）
    pub since: Option<DateTime<Utc>>,
    /// nt → created_at <=
    pub until: Option<DateTime<Utc>>,
    /// continuation（前ページ最後の short_id）
    pub cursor: Option<i64>,
    /// r=o
    pub ascending: bool,
    pub limit: i64,
}

/// WHERE 句は「($k IS NULL OR ...)」型で固定し動的連結しない。asc/desc と
/// cursor の比較方向だけが 2 変種（ユーザー入力は一切 SQL 文字列に入らない）。
const STREAM_WHERE: &str = r#"a.muted_at IS NULL
      AND ($1::uuid IS NULL OR a.feed_id = $1)
      AND ($2::text IS NULL OR a.feed_id IN (
            SELECT f.id FROM feeds f JOIN folders fo ON fo.id = f.folder_id
            WHERE fo.name = $2))
      AND (NOT $3 OR a.is_read = false)
      AND (NOT $4 OR a.is_read = true)
      AND (NOT $5 OR EXISTS (SELECT 1 FROM article_stars s WHERE s.article_id = a.id))
      AND ($6::timestamptz IS NULL OR a.created_at >= $6)
      AND ($7::timestamptz IS NULL OR a.created_at <= $7)"#;

fn cursor_and_order(ascending: bool) -> (&'static str, &'static str) {
    if ascending {
        ("($8::bigint IS NULL OR a.short_id > $8)", "ASC")
    } else {
        ("($8::bigint IS NULL OR a.short_id < $8)", "DESC")
    }
}

pub async fn list_item_ids(pool: &PgPool, f: &StreamFilter) -> AppResult<Vec<i64>> {
    let (cursor_cond, order) = cursor_and_order(f.ascending);
    let sql = format!(
        "SELECT a.short_id FROM articles a WHERE {STREAM_WHERE} AND {cursor_cond}
         ORDER BY a.short_id {order} LIMIT $9"
    );
    let ids = sqlx::query_scalar(&sql)
        .bind(f.feed_id)
        .bind(&f.folder_name)
        .bind(f.unread_only)
        .bind(f.read_only)
        .bind(f.starred_only)
        .bind(f.since)
        .bind(f.until)
        .bind(f.cursor)
        .bind(f.limit)
        .fetch_all(pool)
        .await?;
    Ok(ids)
}

/// items/contents・stream/contents 用の記事行（フィード・フォルダ・スター込み）。
#[derive(Debug, sqlx::FromRow)]
pub struct ItemRow {
    pub short_id: i64,
    pub url: String,
    pub title: String,
    pub content: String,
    pub author: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub is_read: bool,
    pub starred: bool,
    pub feed_id: Uuid,
    pub feed_title: Option<String>,
    pub feed_url: String,
    pub folder_name: Option<String>,
}

const ITEM_SELECT: &str = r#"SELECT a.short_id, a.url, a.title, a.content, a.author,
           a.published_at, a.created_at, a.is_read,
           EXISTS (SELECT 1 FROM article_stars s WHERE s.article_id = a.id) AS starred,
           a.feed_id, f.title AS feed_title, f.url AS feed_url, fo.name AS folder_name
      FROM articles a
      JOIN feeds f ON f.id = a.feed_id
      LEFT JOIN folders fo ON fo.id = f.folder_id"#;

/// stream/items/contents 用。存在しない ID は黙って落ちる（エラーにしない）。
pub async fn items_by_short_ids(pool: &PgPool, ids: &[i64]) -> AppResult<Vec<ItemRow>> {
    let sql = format!("{ITEM_SELECT} WHERE a.short_id = ANY($1) ORDER BY a.short_id DESC");
    let rows = sqlx::query_as::<_, ItemRow>(&sql)
        .bind(ids)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// stream/contents 用（フィルタ一発）。
pub async fn list_stream_items(pool: &PgPool, f: &StreamFilter) -> AppResult<Vec<ItemRow>> {
    let (cursor_cond, order) = cursor_and_order(f.ascending);
    let sql = format!(
        "{ITEM_SELECT} WHERE {STREAM_WHERE} AND {cursor_cond}
         ORDER BY a.short_id {order} LIMIT $9"
    );
    let rows = sqlx::query_as::<_, ItemRow>(&sql)
        .bind(f.feed_id)
        .bind(&f.folder_name)
        .bind(f.unread_only)
        .bind(f.read_only)
        .bind(f.starred_only)
        .bind(f.since)
        .bind(f.until)
        .bind(f.cursor)
        .bind(f.limit)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// スター操作用: short_id → articles.id の解決（存在するものだけ返す）。
pub async fn article_ids_by_short_ids(pool: &PgPool, ids: &[i64]) -> AppResult<Vec<(i64, Uuid)>> {
    let rows = sqlx::query_as::<_, (i64, Uuid)>(
        "SELECT short_id, id FROM articles WHERE short_id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ---- 書き込み系統 1: 既読（sync 所有の short_id キーで一括 UPDATE） ------------

/// ★このクエリは `articles::repository::set_read`（UPDATE articles SET is_read = $2）
///   と意味論的に等価であることを維持する義務がある。articles 側の既読化に
///   副作用が付いた場合は本関数も追随すること（§9.3 パリティテストで固定）。
///   存在しない short_id が混ざってもエラーにしない（stale ID 耐性）。
pub async fn set_read_by_short_ids(pool: &PgPool, ids: &[i64], read: bool) -> AppResult<u64> {
    let res = sqlx::query("UPDATE articles SET is_read = $2 WHERE short_id = ANY($1)")
        .bind(ids)
        .bind(read)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// mark-all-as-read。muted 記事は配信していないので既読化しない
/// （UI の `articles::repository::mark_all_read` は無条件 — 意図的差異）。
pub async fn mark_all_read(
    pool: &PgPool,
    feed_id: Option<Uuid>,
    folder_name: Option<&str>,
    older_than: DateTime<Utc>,
) -> AppResult<u64> {
    let res = sqlx::query(
        r#"UPDATE articles a SET is_read = true
           WHERE a.is_read = false
             AND a.muted_at IS NULL
             AND a.created_at <= $1
             AND ($2::uuid IS NULL OR a.feed_id = $2)
             AND ($3::text IS NULL OR a.feed_id IN (
                   SELECT f.id FROM feeds f JOIN folders fo ON fo.id = f.folder_id
                   WHERE fo.name = $3))"#,
    )
    .bind(older_than)
    .bind(feed_id)
    .bind(folder_name)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

// ---- sync_tokens ---------------------------------------------------------------

/// last_used_at の更新スロットル（毎リクエスト書込み回避 — auth の TOUCH_AFTER と
/// 同じ発想）。
const TOUCH_AFTER_SQL: &str = "interval '1 hour'";

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct TokenRow {
    pub id: Uuid,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

pub async fn insert_token(pool: &PgPool, token_hash: &str, label: Option<&str>) -> AppResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO sync_tokens (token_hash, label) VALUES ($1, $2) RETURNING id",
    )
    .bind(token_hash)
    .bind(label)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// 同一 label の古いトークンを keep 件だけ残して削除。再ログインをループする
/// 行儀の悪いクライアントでも行数が無限増殖しない（既定 keep=10）。
pub async fn prune_tokens_for_label(
    pool: &PgPool,
    label: Option<&str>,
    keep: i64,
) -> AppResult<u64> {
    let res = sqlx::query(
        r#"DELETE FROM sync_tokens
           WHERE label IS NOT DISTINCT FROM $1
             AND id NOT IN (SELECT id FROM sync_tokens
                            WHERE label IS NOT DISTINCT FROM $1
                            ORDER BY created_at DESC LIMIT $2)"#,
    )
    .bind(label)
    .bind(keep)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// ハッシュ索引一致で引く（平文比較のタイミングリークが無い — auth_sessions と
/// 同じ方式）。
pub async fn find_token(pool: &PgPool, token_hash: &str) -> AppResult<Option<Uuid>> {
    let id = sqlx::query_scalar("SELECT id FROM sync_tokens WHERE token_hash = $1")
        .bind(token_hash)
        .fetch_optional(pool)
        .await?;
    Ok(id)
}

pub async fn touch_token(pool: &PgPool, id: Uuid) -> AppResult<()> {
    sqlx::query(&format!(
        "UPDATE sync_tokens SET last_used_at = now()
         WHERE id = $1 AND (last_used_at IS NULL OR last_used_at < now() - {TOUCH_AFTER_SQL})"
    ))
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_tokens(pool: &PgPool) -> AppResult<Vec<TokenRow>> {
    let rows = sqlx::query_as::<_, TokenRow>(
        "SELECT id, label, created_at, last_used_at FROM sync_tokens
         ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn delete_token(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let res = sqlx::query("DELETE FROM sync_tokens WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

// ---- unread-count ---------------------------------------------------------------

/// 未読を持つフィードごとの集計行。フォルダ集計・合計行・max は service 層で合成。
#[derive(Debug, sqlx::FromRow)]
pub struct UnreadRow {
    pub feed_id: Uuid,
    pub folder_name: Option<String>,
    pub cnt: i64,
    pub newest: DateTime<Utc>,
}

pub async fn unread_counts(pool: &PgPool) -> AppResult<Vec<UnreadRow>> {
    let rows = sqlx::query_as::<_, UnreadRow>(
        r#"SELECT a.feed_id, fo.name AS folder_name,
                  count(*) AS cnt, max(a.created_at) AS newest
           FROM articles a
           JOIN feeds f ON f.id = a.feed_id
           LEFT JOIN folders fo ON fo.id = f.folder_id
           WHERE a.is_read = false AND a.muted_at IS NULL
           GROUP BY a.feed_id, fo.name"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    //! 実 DB 往復テスト。実行方法（dev DB をホストへ公開して）:
    //!   DB_PORT=15432 docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d db
    //!   DATABASE_URL=postgres://rss:<pw>@localhost:15432/rssreader cargo test -- --ignored
    use super::*;
    use crate::features::sync::domain::paginate;
    use crate::shared::auth::hash_token;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        PgPool::connect(&url).await.expect("connect")
    }

    /// 一意 URL のフィードを作る（後始末は cleanup_feed）。
    async fn mk_feed(pool: &PgPool) -> Uuid {
        let url = format!("https://example.com/sync-test/{}", Uuid::new_v4());
        let id: Uuid =
            sqlx::query_scalar("INSERT INTO feeds (id, url) VALUES ($1, $2) RETURNING id")
                .bind(Uuid::new_v4())
                .bind(&url)
                .fetch_one(pool)
                .await
                .expect("insert feed");
        id
    }

    /// 記事を1件作り (id, short_id) を返す。DEFAULT 採番（= migration の
    /// シーケンス）を経由することが移行検証を兼ねる。
    async fn mk_article(pool: &PgPool, feed_id: Uuid) -> (Uuid, i64) {
        let url = format!("https://example.com/sync-post/{}", Uuid::new_v4());
        let row: (Uuid, i64) = sqlx::query_as(
            r#"INSERT INTO articles (id, feed_id, url, title, content)
               VALUES (gen_random_uuid(), $1, $2, 't', 'c')
               RETURNING id, short_id"#,
        )
        .bind(feed_id)
        .bind(&url)
        .fetch_one(pool)
        .await
        .expect("insert article");
        row
    }

    async fn cleanup_feed(pool: &PgPool, feed_id: Uuid) {
        sqlx::query("DELETE FROM articles WHERE feed_id = $1")
            .bind(feed_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM feeds WHERE id = $1")
            .bind(feed_id)
            .execute(pool)
            .await
            .unwrap();
    }

    fn base_filter(limit: i64) -> StreamFilter {
        StreamFilter {
            limit,
            ..StreamFilter::default()
        }
    }

    // ---- migration 検証（§9.3 先頭・Task の Red だった項目） -----------------

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn short_id_default_assignment_is_monotonic_and_not_null() {
        let pool = pool().await;
        let feed = mk_feed(&pool).await;
        let (_, s1) = mk_article(&pool, feed).await;
        let (_, s2) = mk_article(&pool, feed).await;
        let (_, s3) = mk_article(&pool, feed).await;
        assert!(
            s1 > 0,
            "short_id must be positive (ItemId domain invariant)"
        );
        assert!(
            s2 > s1 && s3 > s2,
            "insertion order must be monotonic: {s1} {s2} {s3}"
        );
        cleanup_feed(&pool, feed).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn upsert_batch_from_articles_slice_still_works_with_short_id_default() {
        // articles スライス無変更の裏取り: 既存の upsert_batch（UNNEST・カラム列挙）
        // が short_id DEFAULT で通ること。
        use crate::features::articles::repository::{upsert_batch, NewArticle};
        use crate::features::feeds::domain::FeedId;
        let pool = pool().await;
        let feed = mk_feed(&pool).await;
        let url = format!("https://example.com/sync-upsert/{}", Uuid::new_v4());
        let stored = upsert_batch(
            &pool,
            FeedId(feed),
            &[NewArticle {
                url: url.clone(),
                title: "t".into(),
                content: "c".into(),
                published_at: None,
                author: None,
            }],
        )
        .await
        .expect("upsert_batch");
        assert_eq!(stored.len(), 1);
        let sid: i64 = sqlx::query_scalar("SELECT short_id FROM articles WHERE url = $1")
            .bind(&url)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(sid > 0);
        cleanup_feed(&pool, feed).await;
    }

    // ---- keyset ページング（§9.3） -------------------------------------------

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keyset_pages_without_dup_or_loss() {
        let pool = pool().await;
        let feed = mk_feed(&pool).await;
        let mut ids = Vec::new();
        for _ in 0..5 {
            ids.push(mk_article(&pool, feed).await.1);
        }

        // n=2 → 3 ページ（2+2+1）。n+1 フェッチ + paginate で回す。
        let n = 2usize;
        let mut seen = Vec::new();
        let mut cursor: Option<i64> = None;
        let mut pages = 0;
        loop {
            let f = StreamFilter {
                feed_id: Some(feed),
                cursor,
                limit: (n + 1) as i64,
                ..StreamFilter::default()
            };
            let rows = list_item_ids(&pool, &f).await.unwrap();
            let (page, cont) = paginate(rows, n);
            pages += 1;
            seen.extend_from_slice(&page);
            match cont {
                Some(c) => cursor = Some(c.parse().unwrap()),
                None => break,
            }
            assert!(pages < 10, "runaway pagination");
        }
        assert_eq!(pages, 3);
        // 重複・欠落なし（降順）。
        let mut expect = ids.clone();
        expect.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(seen, expect);
        cleanup_feed(&pool, feed).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keyset_ascending_with_r_o() {
        let pool = pool().await;
        let feed = mk_feed(&pool).await;
        let mut ids = Vec::new();
        for _ in 0..3 {
            ids.push(mk_article(&pool, feed).await.1);
        }
        let f = StreamFilter {
            feed_id: Some(feed),
            ascending: true,
            limit: 10,
            ..StreamFilter::default()
        };
        let rows = list_item_ids(&pool, &f).await.unwrap();
        assert_eq!(rows, ids); // 挿入順 = 昇順
                               // 昇順の cursor は「より大きいものへ進む」。
        let f = StreamFilter {
            feed_id: Some(feed),
            ascending: true,
            cursor: Some(ids[0]),
            limit: 10,
            ..StreamFilter::default()
        };
        assert_eq!(list_item_ids(&pool, &f).await.unwrap(), ids[1..]);
        cleanup_feed(&pool, feed).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stream_filters_unread_starred_folder_muted_ot() {
        use crate::features::annotations::repository::add_star;
        let pool = pool().await;
        let feed = mk_feed(&pool).await;
        let (id_a, s_a) = mk_article(&pool, feed).await; // 既読 + スターにする
        let (id_b, s_b) = mk_article(&pool, feed).await; // 未読のまま
        let (_id_c, s_c) = mk_article(&pool, feed).await; // muted にする

        sqlx::query("UPDATE articles SET is_read = true WHERE id = $1")
            .bind(id_a)
            .execute(&pool)
            .await
            .unwrap();
        add_star(&pool, id_a).await.unwrap();
        sqlx::query("UPDATE articles SET muted_at = now() WHERE short_id = $1")
            .bind(s_c)
            .execute(&pool)
            .await
            .unwrap();

        // muted は無条件で除外。
        let all = list_item_ids(
            &pool,
            &StreamFilter {
                feed_id: Some(feed),
                ..base_filter(10)
            },
        )
        .await
        .unwrap();
        assert!(all.contains(&s_a) && all.contains(&s_b) && !all.contains(&s_c));

        // unread_only（xt=read）。
        let unread = list_item_ids(
            &pool,
            &StreamFilter {
                feed_id: Some(feed),
                unread_only: true,
                ..base_filter(10)
            },
        )
        .await
        .unwrap();
        assert_eq!(unread, vec![s_b]);

        // read_only（s=.../read）。
        let read = list_item_ids(
            &pool,
            &StreamFilter {
                feed_id: Some(feed),
                read_only: true,
                ..base_filter(10)
            },
        )
        .await
        .unwrap();
        assert_eq!(read, vec![s_a]);

        // starred_only。
        let starred = list_item_ids(
            &pool,
            &StreamFilter {
                feed_id: Some(feed),
                starred_only: true,
                ..base_filter(10)
            },
        )
        .await
        .unwrap();
        assert_eq!(starred, vec![s_a]);

        // folder フィルタ。
        let folder_name = format!("sync-test-{}", Uuid::new_v4());
        let folder_id: Uuid = sqlx::query_scalar(
            "INSERT INTO folders (id, name, position) VALUES ($1, $2, 9999) RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(&folder_name)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE feeds SET folder_id = $2 WHERE id = $1")
            .bind(feed)
            .bind(folder_id)
            .execute(&pool)
            .await
            .unwrap();
        let in_folder = list_item_ids(
            &pool,
            &StreamFilter {
                folder_name: Some(folder_name.clone()),
                ..base_filter(10)
            },
        )
        .await
        .unwrap();
        assert!(in_folder.contains(&s_a) && in_folder.contains(&s_b));

        // ot（since）: b の created_at を過去へずらすと since で落ちる。
        sqlx::query("UPDATE articles SET created_at = now() - interval '2 days' WHERE id = $1")
            .bind(id_b)
            .execute(&pool)
            .await
            .unwrap();
        let since = list_item_ids(
            &pool,
            &StreamFilter {
                feed_id: Some(feed),
                since: Some(Utc::now() - chrono::Duration::days(1)),
                ..base_filter(10)
            },
        )
        .await
        .unwrap();
        assert!(since.contains(&s_a) && !since.contains(&s_b));
        // nt（until）は逆。
        let until = list_item_ids(
            &pool,
            &StreamFilter {
                feed_id: Some(feed),
                until: Some(Utc::now() - chrono::Duration::days(1)),
                ..base_filter(10)
            },
        )
        .await
        .unwrap();
        assert_eq!(until, vec![s_b]);

        cleanup_feed(&pool, feed).await;
        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(folder_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn items_by_short_ids_returns_rows_and_drops_stale_silently() {
        let pool = pool().await;
        let feed = mk_feed(&pool).await;
        let (id_a, s_a) = mk_article(&pool, feed).await;
        crate::features::annotations::repository::add_star(&pool, id_a)
            .await
            .unwrap();

        let rows = items_by_short_ids(&pool, &[s_a, 9_223_372_036_854_775_000])
            .await
            .unwrap();
        assert_eq!(rows.len(), 1); // stale ID は黙って落ちる
        let row = &rows[0];
        assert_eq!(row.short_id, s_a);
        assert!(row.starred);
        assert_eq!(row.feed_id, feed);
        assert!(row.feed_url.starts_with("https://example.com/sync-test/"));
        cleanup_feed(&pool, feed).await;
    }

    // ---- 既読パリティ（§9.3 の要） -------------------------------------------

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn set_read_by_short_ids_is_equivalent_to_articles_set_read() {
        use crate::features::articles::domain::ArticleId;
        use crate::features::articles::repository::set_read;
        let pool = pool().await;
        let feed = mk_feed(&pool).await;
        let (_id_a, s_a) = mk_article(&pool, feed).await;
        let (id_b, _s_b) = mk_article(&pool, feed).await;

        // 同じ操作を sync 経由（a）と articles 経由（b）で行い、最終状態が一致。
        let affected = set_read_by_short_ids(&pool, &[s_a], true).await.unwrap();
        assert_eq!(affected, 1);
        set_read(&pool, ArticleId(id_b), true).await.unwrap();
        let states: Vec<(Uuid, bool)> =
            sqlx::query_as("SELECT id, is_read FROM articles WHERE feed_id = $1")
                .bind(feed)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(states.iter().all(|(_, r)| *r));

        // 逆方向（未読化）も等価。
        set_read_by_short_ids(&pool, &[s_a], false).await.unwrap();
        set_read(&pool, ArticleId(id_b), false).await.unwrap();
        let states: Vec<(Uuid, bool)> =
            sqlx::query_as("SELECT id, is_read FROM articles WHERE feed_id = $1")
                .bind(feed)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(states.iter().all(|(_, r)| !*r));

        // 存在しない short_id 混在でもエラーにならない。
        let affected = set_read_by_short_ids(&pool, &[s_a, 9_223_372_036_854_775_000], true)
            .await
            .unwrap();
        assert_eq!(affected, 1);
        cleanup_feed(&pool, feed).await;
    }

    // ---- mark_all_read（ts 境界・muted 除外） ---------------------------------

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn mark_all_read_respects_ts_boundary_and_muted() {
        let pool = pool().await;
        let feed = mk_feed(&pool).await;
        let (id_old, s_old) = mk_article(&pool, feed).await;
        let (_id_new, s_new) = mk_article(&pool, feed).await;
        let (id_muted, s_muted) = mk_article(&pool, feed).await;

        sqlx::query("UPDATE articles SET created_at = now() - interval '2 days' WHERE id = $1")
            .bind(id_old)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE articles SET created_at = now() - interval '2 days', muted_at = now() WHERE id = $1",
        )
        .bind(id_muted)
        .execute(&pool)
        .await
        .unwrap();

        // ts = 1日前 → old だけ既読化（new は新しすぎ、muted は除外）。
        let affected = mark_all_read(
            &pool,
            Some(feed),
            None,
            Utc::now() - chrono::Duration::days(1),
        )
        .await
        .unwrap();
        assert_eq!(affected, 1);
        let read_ids: Vec<i64> =
            sqlx::query_scalar("SELECT short_id FROM articles WHERE feed_id = $1 AND is_read")
                .bind(feed)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(read_ids, vec![s_old]);
        let _ = (s_new, s_muted);
        cleanup_feed(&pool, feed).await;
    }

    // ---- sync_tokens ----------------------------------------------------------

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn token_roundtrip_prune_and_revoke() {
        let pool = pool().await;
        let label = format!("test-{}", Uuid::new_v4());

        // 発行 → ハッシュで引ける。平文では引けない。
        let raw = "some-raw-token-value";
        let id = insert_token(&pool, &hash_token(raw), Some(&label))
            .await
            .unwrap();
        assert_eq!(find_token(&pool, &hash_token(raw)).await.unwrap(), Some(id));
        assert_eq!(find_token(&pool, raw).await.unwrap(), None);

        // touch は last_used_at を埋める（初回は必ず更新される）。
        touch_token(&pool, id).await.unwrap();
        let row: (Option<DateTime<Utc>>,) =
            sqlx::query_as("SELECT last_used_at FROM sync_tokens WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(row.0.is_some());

        // 同一 label を計 3 本にして keep=2 で prune → 最古の1本が消える。
        // created_at の同時刻衝突を避けるため明示的にずらす。
        sqlx::query("UPDATE sync_tokens SET created_at = now() - interval '2 hours' WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let id2 = insert_token(&pool, &hash_token("t2"), Some(&label))
            .await
            .unwrap();
        sqlx::query("UPDATE sync_tokens SET created_at = now() - interval '1 hour' WHERE id = $1")
            .bind(id2)
            .execute(&pool)
            .await
            .unwrap();
        let id3 = insert_token(&pool, &hash_token("t3"), Some(&label))
            .await
            .unwrap();
        let pruned = prune_tokens_for_label(&pool, Some(&label), 2)
            .await
            .unwrap();
        assert_eq!(pruned, 1);
        assert_eq!(find_token(&pool, &hash_token(raw)).await.unwrap(), None); // 最古が消えた
        assert!(find_token(&pool, &hash_token("t2"))
            .await
            .unwrap()
            .is_some());
        assert!(find_token(&pool, &hash_token("t3"))
            .await
            .unwrap()
            .is_some());

        // 一覧に載る・失効できる・二重失効は false。
        let listed = list_tokens(&pool).await.unwrap();
        assert!(listed.iter().any(|t| t.id == id3));
        assert!(delete_token(&pool, id3).await.unwrap());
        assert!(!delete_token(&pool, id3).await.unwrap());

        sqlx::query("DELETE FROM sync_tokens WHERE label = $1")
            .bind(&label)
            .execute(&pool)
            .await
            .unwrap();
    }

    // ---- unread_counts ----------------------------------------------------------

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn unread_counts_groups_by_feed_with_folder_name() {
        let pool = pool().await;
        let feed = mk_feed(&pool).await;
        let (_a, _) = mk_article(&pool, feed).await;
        let (_b, _) = mk_article(&pool, feed).await;
        let (id_read, _) = mk_article(&pool, feed).await;
        sqlx::query("UPDATE articles SET is_read = true WHERE id = $1")
            .bind(id_read)
            .execute(&pool)
            .await
            .unwrap();

        let rows = unread_counts(&pool).await.unwrap();
        let row = rows.iter().find(|r| r.feed_id == feed).expect("feed row");
        assert_eq!(row.cnt, 2);
        assert_eq!(row.folder_name, None);
        cleanup_feed(&pool, feed).await;
    }
}
