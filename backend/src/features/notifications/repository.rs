//! notifications スライスのデータ access。購読 CRUD・通知ウォーターマーク・
//! 高優先フィードの新着射影。plain sqlx（trait なし・runtime クエリ）。
//! 記事/フィードは他スライス所有だが、instapaper の read 射影前例に倣い
//! **読み取りのみ**ローカル SQL で引く（書き込みは所有スライスに委ねる）。

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::domain::PushSubscriptionInput;
use crate::shared::error::AppResult;

/// 送信に必要な最小の購読フィールド。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredSubscription {
    pub id: Uuid,
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

pub async fn upsert_subscription(
    pool: &PgPool,
    sub: &PushSubscriptionInput,
    user_agent: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO push_subscriptions (id, endpoint, p256dh, auth, user_agent)
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (endpoint) DO UPDATE
             SET p256dh = EXCLUDED.p256dh,
                 auth = EXCLUDED.auth,
                 user_agent = EXCLUDED.user_agent"#,
    )
    .bind(Uuid::new_v4())
    .bind(&sub.endpoint)
    .bind(&sub.keys.p256dh)
    .bind(&sub.keys.auth)
    .bind(user_agent)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_subscription(pool: &PgPool, endpoint: &str) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = $1")
        .bind(endpoint)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// 失効購読の GC（送信で 404/410 が返った行を id で削除）。
pub async fn delete_subscription_by_id(pool: &PgPool, id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM push_subscriptions WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_subscriptions(pool: &PgPool) -> AppResult<Vec<StoredSubscription>> {
    let rows = sqlx::query_as::<_, StoredSubscription>(
        "SELECT id, endpoint, p256dh, auth FROM push_subscriptions",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 高優先フィード（priority>=1）の、`since` より後・`until` 以下に作成された記事（新着）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArticleNotice {
    pub title: String,
    pub url: String,
    pub feed_title: Option<String>,
    /// ウォーターマーク進行の基準（上限超過時は「通知した最後の記事」まで進める）。
    pub created_at: DateTime<Utc>,
}

pub async fn new_priority_articles(
    pool: &PgPool,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    limit: i64,
) -> AppResult<Vec<ArticleNotice>> {
    let rows = sqlx::query_as::<_, ArticleNotice>(
        r#"SELECT a.title AS title, a.url AS url, f.title AS feed_title, a.created_at AS created_at
           FROM articles a
           JOIN feeds f ON f.id = a.feed_id
           WHERE f.priority >= 1
             AND a.muted_at IS NULL
             AND a.created_at > $1
             AND a.created_at <= $2
           ORDER BY a.created_at ASC
           LIMIT $3"#,
    )
    .bind(since)
    .bind(until)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 通知ウォーターマーク（シングルトン行）の取得。
pub async fn get_watermark(pool: &PgPool) -> AppResult<DateTime<Utc>> {
    let ts: DateTime<Utc> =
        sqlx::query_scalar("SELECT last_notified_at FROM push_notify_state WHERE id = true")
            .fetch_one(pool)
            .await?;
    Ok(ts)
}

pub async fn set_watermark(pool: &PgPool, ts: DateTime<Utc>) -> AppResult<()> {
    sqlx::query("UPDATE push_notify_state SET last_notified_at = $1 WHERE id = true")
        .bind(ts)
        .execute(pool)
        .await?;
    Ok(())
}
