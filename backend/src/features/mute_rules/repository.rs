use sqlx::PgPool;

use super::domain::{self, MuteRule, MuteRuleId};
use crate::shared::error::{AppError, AppResult};

pub async fn list_all(pool: &PgPool) -> AppResult<Vec<MuteRule>> {
    let rows = sqlx::query_as::<_, MuteRule>("SELECT * FROM mute_rules ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn get(pool: &PgPool, id: MuteRuleId) -> AppResult<MuteRule> {
    sqlx::query_as::<_, MuteRule>("SELECT * FROM mute_rules WHERE id = $1")
        .bind(id.0)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn insert(
    pool: &PgPool,
    field: &str,
    pattern: &str,
    match_type: &str,
    action: &str,
    enabled: bool,
) -> AppResult<MuteRule> {
    let row = sqlx::query_as::<_, MuteRule>(
        r#"INSERT INTO mute_rules (field, pattern, match_type, action, enabled)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(field)
    .bind(pattern.trim())
    .bind(match_type)
    .bind(action)
    .bind(enabled)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn update(
    pool: &PgPool,
    id: MuteRuleId,
    field: Option<&str>,
    pattern: Option<&str>,
    match_type: Option<&str>,
    action: Option<&str>,
    enabled: Option<bool>,
) -> AppResult<MuteRule> {
    sqlx::query_as::<_, MuteRule>(
        r#"UPDATE mute_rules SET
             field      = COALESCE($2, field),
             pattern    = COALESCE($3, pattern),
             match_type = COALESCE($4, match_type),
             action     = COALESCE($5, action),
             enabled    = COALESCE($6, enabled),
             updated_at = now()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id.0)
    .bind(field)
    .bind(pattern.map(str::trim))
    .bind(match_type)
    .bind(action)
    .bind(enabled)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn delete(pool: &PgPool, id: MuteRuleId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM mute_rules WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// Clear all hide stamps (pre-step of re-evaluation). Idempotent.
pub async fn clear_all_hidden(pool: &PgPool) -> AppResult<u64> {
    let res = sqlx::query("UPDATE articles SET muted_at = NULL WHERE muted_at IS NOT NULL")
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// Apply one rule to existing articles. Column name comes from the
/// field_column whitelist (injection-safe); pattern is parameterized + escaped.
pub async fn apply_rule(pool: &PgPool, field: &str, pattern: &str, action: &str) -> AppResult<u64> {
    let col = domain::field_column(field)?;
    let needle = format!("%{}%", domain::escape_like(pattern.trim()));

    // 保存ページ（合成フィード）はミュートルールの対象外（勝手な既読化/非表示で
    // 「後で読む」から黙って消えるのを防ぐ）。
    const NOT_SAVED: &str = "feed_id NOT IN (SELECT id FROM feeds WHERE kind <> 'rss')";
    let sql = match action {
        "hide" => format!(
            "UPDATE articles SET muted_at = now() \
             WHERE muted_at IS NULL AND {NOT_SAVED} AND {col} ILIKE $1 ESCAPE '\\'"
        ),
        "mark_read" => format!(
            "UPDATE articles SET is_read = true \
             WHERE is_read = false AND {NOT_SAVED} AND {col} ILIKE $1 ESCAPE '\\'"
        ),
        other => return Err(AppError::Validation(format!("unknown action: {other}"))),
    };

    let res = sqlx::query(&sql).bind(&needle).execute(pool).await?;
    Ok(res.rows_affected())
}
