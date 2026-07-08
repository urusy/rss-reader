//! usage_events / llm_usage_events への書き込み・集計・パージ。
//!
//! 書き込みは service.rs の writer タスク専用（応答パスからは呼ばない）。
//! 集計は生イベントの直接集計（単一ユーザー・低頻度なのでロールアップ不要。
//! `idx_usage_events_time_feature` が期間絞り込みを受ける）。

use sqlx::PgPool;

use super::domain::{LlmUsageRow, TtsSourceRow, UsageBucketRow};
use crate::shared::error::AppResult;

pub async fn insert_event(
    pool: &PgPool,
    feature: &str,
    source: &str,
    status: Option<i16>,
    meta: Option<&serde_json::Value>,
) -> AppResult<()> {
    sqlx::query("INSERT INTO usage_events (feature, source, status, meta) VALUES ($1, $2, $3, $4)")
        .bind(feature)
        .bind(source)
        .bind(status)
        .bind(meta)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_llm_event(
    pool: &PgPool,
    purpose: &str,
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO llm_usage_events (purpose, model, input_tokens, output_tokens)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(purpose)
    .bind(model)
    .bind(input_tokens)
    .bind(output_tokens)
    .execute(pool)
    .await?;
    Ok(())
}

/// 期間×機能の時系列。成功のみを「利用」と数える
/// （client イベントは status IS NULL、server は <400 が成功）。
/// unit は domain::bucket_unit で検証済みの3値のみ渡すこと。
pub async fn fetch_feature_buckets(
    pool: &PgPool,
    unit: &str,
    days: i32,
) -> AppResult<Vec<UsageBucketRow>> {
    let rows = sqlx::query_as::<_, UsageBucketRow>(
        r#"SELECT date_trunc($1, occurred_at) AS bucket, feature, COUNT(*)::bigint AS count
           FROM usage_events
           WHERE occurred_at >= now() - make_interval(days => $2)
             AND (status IS NULL OR status < 400)
           GROUP BY 1, 2
           ORDER BY 1, 2"#,
    )
    .bind(unit)
    .bind(days)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// LLM 実呼び出しの purpose×model 集計。
pub async fn fetch_llm_summary(pool: &PgPool, days: i32) -> AppResult<Vec<LlmUsageRow>> {
    let rows = sqlx::query_as::<_, LlmUsageRow>(
        r#"SELECT purpose, model, COUNT(*)::bigint AS calls,
                  COALESCE(SUM(input_tokens), 0)::bigint  AS input_tokens,
                  COALESCE(SUM(output_tokens), 0)::bigint AS output_tokens
           FROM llm_usage_events
           WHERE occurred_at >= now() - make_interval(days => $1)
           GROUP BY 1, 2
           ORDER BY calls DESC, purpose, model"#,
    )
    .bind(days)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// tts_play の読み上げ対象内訳（meta->>'source' 別）。
pub async fn fetch_tts_sources(pool: &PgPool, days: i32) -> AppResult<Vec<TtsSourceRow>> {
    let rows = sqlx::query_as::<_, TtsSourceRow>(
        r#"SELECT COALESCE(meta->>'source', 'unknown') AS source, COUNT(*)::bigint AS count
           FROM usage_events
           WHERE feature = 'tts_play'
             AND occurred_at >= now() - make_interval(days => $1)
           GROUP BY 1
           ORDER BY count DESC"#,
    )
    .bind(days)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 保持期間を過ぎた行を両テーブルから削除。戻り値は (usage, llm) の削除行数。
pub async fn purge_older_than(pool: &PgPool, days: i64) -> AppResult<(u64, u64)> {
    let usage = sqlx::query(
        "DELETE FROM usage_events WHERE occurred_at < now() - make_interval(days => $1::int)",
    )
    .bind(days)
    .execute(pool)
    .await?
    .rows_affected();
    let llm = sqlx::query(
        "DELETE FROM llm_usage_events WHERE occurred_at < now() - make_interval(days => $1::int)",
    )
    .bind(days)
    .execute(pool)
    .await?
    .rows_affected();
    Ok((usage, llm))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL for repo tests");
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap()
    }

    // テストは並行実行されるため、feature 名はテストごとに固有にし、
    // cleanup も自分の行だけを消す（他テストの行を巻き込まない）。
    async fn cleanup_feature(pool: &PgPool, feature: &str) {
        sqlx::query("DELETE FROM usage_events WHERE feature = $1")
            .bind(feature)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres (DATABASE_URL)"]
    async fn event_insert_and_bucket_aggregation_counts_success_only() {
        const F: &str = "test_usage_repo_bucket";
        let pool = pool().await;
        cleanup_feature(&pool, F).await;

        insert_event(&pool, F, "server", Some(200), None)
            .await
            .unwrap();
        insert_event(&pool, F, "server", Some(201), None)
            .await
            .unwrap();
        insert_event(&pool, F, "server", Some(500), None)
            .await
            .unwrap(); // 失敗: 数えない
        insert_event(&pool, F, "client", None, None).await.unwrap(); // client: 数える

        let rows = fetch_feature_buckets(&pool, "day", 7).await.unwrap();
        let count: i64 = rows
            .iter()
            .filter(|r| r.feature == F)
            .map(|r| r.count)
            .sum();
        assert_eq!(
            count, 3,
            "success (2) + client (1); the 500 must be excluded"
        );

        cleanup_feature(&pool, F).await;
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres (DATABASE_URL)"]
    async fn llm_summary_groups_by_purpose_and_model() {
        let pool = pool().await;
        sqlx::query("DELETE FROM llm_usage_events WHERE model = 'test-usage-repo-model'")
            .execute(&pool)
            .await
            .unwrap();

        insert_llm_event(&pool, "summarize", "test-usage-repo-model", 100, 10)
            .await
            .unwrap();
        insert_llm_event(&pool, "summarize", "test-usage-repo-model", 200, 20)
            .await
            .unwrap();
        insert_llm_event(&pool, "translate", "test-usage-repo-model", 50, 5)
            .await
            .unwrap();

        let rows = fetch_llm_summary(&pool, 7).await.unwrap();
        let sum = rows
            .iter()
            .find(|r| r.purpose == "summarize" && r.model == "test-usage-repo-model")
            .expect("summarize row");
        assert_eq!(sum.calls, 2);
        assert_eq!(sum.input_tokens, 300);
        assert_eq!(sum.output_tokens, 30);
        let tr = rows
            .iter()
            .find(|r| r.purpose == "translate" && r.model == "test-usage-repo-model")
            .expect("translate row");
        assert_eq!(tr.calls, 1);

        sqlx::query("DELETE FROM llm_usage_events WHERE model = 'test-usage-repo-model'")
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres (DATABASE_URL)"]
    async fn tts_sources_breakdown_reads_meta() {
        let pool = pool().await;
        let cleanup = || async {
            sqlx::query(
                "DELETE FROM usage_events WHERE feature = 'tts_play' AND meta ? 'test_usage_repo'",
            )
            .execute(&pool)
            .await
            .unwrap();
        };
        cleanup().await;

        // 実運用の tts_play 行と区別できるようテスト印を meta に残す。
        // 内訳集計は差分（before/after）で検証し、実データが混ざっていても壊れない。
        let before = fetch_tts_sources(&pool, 7).await.unwrap();
        let get = |rows: &[TtsSourceRow], s: &str| {
            rows.iter()
                .find(|r| r.source == s)
                .map(|r| r.count)
                .unwrap_or(0)
        };
        let m = |src: &str| serde_json::json!({ "source": src, "test_usage_repo": true });
        insert_event(&pool, "tts_play", "client", None, Some(&m("summary")))
            .await
            .unwrap();
        insert_event(&pool, "tts_play", "client", None, Some(&m("summary")))
            .await
            .unwrap();
        insert_event(&pool, "tts_play", "client", None, Some(&m("content")))
            .await
            .unwrap();

        let after = fetch_tts_sources(&pool, 7).await.unwrap();
        assert_eq!(get(&after, "summary") - get(&before, "summary"), 2);
        assert_eq!(get(&after, "content") - get(&before, "content"), 1);

        cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres (DATABASE_URL)"]
    async fn purge_removes_only_expired_rows() {
        const F: &str = "test_usage_repo_purge";
        let pool = pool().await;
        cleanup_feature(&pool, F).await;

        insert_event(&pool, F, "server", Some(200), None)
            .await
            .unwrap();
        insert_event(&pool, F, "server", Some(200), None)
            .await
            .unwrap();
        // 1行だけ400日前に偽装（テスト専用の backdate。本番 insert は now() 固定）。
        sqlx::query(
            "UPDATE usage_events SET occurred_at = now() - interval '400 days'
             WHERE id = (SELECT id FROM usage_events WHERE feature = $1 LIMIT 1)",
        )
        .bind(F)
        .execute(&pool)
        .await
        .unwrap();

        let (usage_deleted, _) = purge_older_than(&pool, 365).await.unwrap();
        assert!(usage_deleted >= 1, "the backdated row must be purged");

        let rows = fetch_feature_buckets(&pool, "day", 7).await.unwrap();
        let remaining: i64 = rows
            .iter()
            .filter(|r| r.feature == F)
            .map(|r| r.count)
            .sum();
        assert_eq!(remaining, 1, "the recent row must survive");

        cleanup_feature(&pool, F).await;
    }
}
