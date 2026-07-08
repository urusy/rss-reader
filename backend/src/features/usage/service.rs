//! writer タスクの起動（sink の受信側）・日次パージ・集計サービス。

use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};

use super::domain::UsageSummary;
use super::repository;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;
use crate::shared::usage::{self, UsageEvent};

/// チャネルを生成して writer タスクを起動し、送信端を shared::usage に渡す。
/// main.rs から起動時に1回だけ呼ぶ。
///
/// writer 1タスクが直列に INSERT するため、イベントがバーストしても
/// 要求パスと DB コネクションを奪い合わない。INSERT 失敗は warn のみ
/// （テレメトリは失ってよい。本体機能を巻き込まない）。
pub fn install(pool: PgPool) {
    let (tx, mut rx) = mpsc::unbounded_channel::<UsageEvent>();
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let result = match &ev {
                UsageEvent::Server { feature, status } => {
                    repository::insert_event(&pool, feature, "server", Some(*status as i16), None)
                        .await
                }
                UsageEvent::Client { feature, meta } => {
                    repository::insert_event(&pool, feature, "client", None, meta.as_ref()).await
                }
                UsageEvent::Llm {
                    purpose,
                    model,
                    input_tokens,
                    output_tokens,
                } => {
                    repository::insert_llm_event(
                        &pool,
                        purpose,
                        model,
                        *input_tokens,
                        *output_tokens,
                    )
                    .await
                }
            };
            if let Err(e) = result {
                tracing::warn!(error = %e, event = ?ev, "usage event write failed (dropped)");
            }
        }
    });
    usage::install(tx);
}

/// 保持期間を過ぎたイベントの日次パージ。retention_days <= 0 なら無効（無期限保持）。
pub fn spawn_purge(pool: PgPool, retention_days: i64) {
    if retention_days <= 0 {
        tracing::info!("usage purge disabled (USAGE_RETENTION_DAYS <= 0)");
        return;
    }
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(24 * 60 * 60));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // 起動直後の tick はスキップ（scheduler.rs と同じ型）。
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match repository::purge_older_than(&pool, retention_days).await {
                Ok((usage, llm)) => {
                    tracing::info!(usage, llm, retention_days, "usage events purged");
                }
                Err(e) => tracing::warn!(error = %e, "usage purge failed"),
            }
        }
    });
}

/// GET /api/usage/summary の本体。3つの集計を1レスポンスに束ねる。
pub async fn summary(state: &AppState, days: i32, unit: &'static str) -> AppResult<UsageSummary> {
    let buckets = repository::fetch_feature_buckets(&state.db, unit, days).await?;
    let llm = repository::fetch_llm_summary(&state.db, days).await?;
    let tts_sources = repository::fetch_tts_sources(&state.db, days).await?;
    Ok(UsageSummary {
        buckets,
        llm,
        tts_sources,
    })
}

/// POST /api/usage/events の本体。検証済みイベントを sink へ流すだけ。
pub fn record_client_event(feature: String, meta: Option<serde_json::Value>) {
    usage::record(UsageEvent::Client { feature, meta });
}
