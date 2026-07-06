//! auth スライスのデータ access。単一ユーザー資格情報（シングルトン行）と
//! サーバー側セッションの CRUD。plain sqlx（trait なし・runtime クエリ）。

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::shared::auth::SESSION_TTL_DAYS;
use crate::shared::error::AppResult;

/// 資格情報（パスワードハッシュ）を取得。行なし = 初回セットアップ未完了。
pub async fn get_credential(pool: &PgPool) -> AppResult<Option<String>> {
    let hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM auth_credential WHERE id = true")
            .fetch_optional(pool)
            .await?;
    Ok(hash)
}

/// 初回セットアップ: 行が無いときだけ INSERT（先勝ち）。true = このリクエストが
/// 設定に成功、false = 既に設定済み（競合した場合も含む）。
pub async fn insert_credential(pool: &PgPool, password_hash: &str) -> AppResult<bool> {
    let res = sqlx::query(
        "INSERT INTO auth_credential (id, password_hash) VALUES (true, $1)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(password_hash)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}

/// パスワード変更（行は必ず存在する前提。無ければ何もしない）。
pub async fn update_credential(pool: &PgPool, password_hash: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE auth_credential SET password_hash = $1, updated_at = now() WHERE id = true",
    )
    .bind(password_hash)
    .execute(pool)
    .await?;
    Ok(())
}

/// 一覧 API 用のセッション行。token_hash は取得しない（露出防止）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

pub async fn insert_session(
    pool: &PgPool,
    token_hash: &str,
    label: Option<&str>,
) -> AppResult<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO auth_sessions (id, token_hash, label, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(token_hash)
    .bind(label)
    .bind(Utc::now() + Duration::days(SESSION_TTL_DAYS))
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn delete_session_by_id(pool: &PgPool, id: Uuid) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM auth_sessions WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// パスワード変更時: 変更を行った現セッション以外を全失効。
pub async fn delete_sessions_except(pool: &PgPool, keep: Uuid) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM auth_sessions WHERE id <> $1")
        .bind(keep)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// 期限切れ行の opportunistic GC（ログイン成功時に呼ぶ）。
pub async fn delete_expired_sessions(pool: &PgPool) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM auth_sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn list_sessions(pool: &PgPool) -> AppResult<Vec<SessionRow>> {
    let rows = sqlx::query_as::<_, SessionRow>(
        "SELECT id, label, created_at, last_seen_at FROM auth_sessions
         WHERE expires_at > now()
         ORDER BY last_seen_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
