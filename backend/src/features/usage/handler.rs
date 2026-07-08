use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use super::domain::{self, UsageSummary};
use super::service;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SummaryQuery {
    pub days: Option<i32>,
    pub bucket: Option<String>,
}

/// GET /api/usage/summary?days=30&bucket=day
pub async fn summary(
    State(state): State<AppState>,
    Query(q): Query<SummaryQuery>,
) -> AppResult<Json<UsageSummary>> {
    let days = domain::clamp_days(q.days);
    let unit = domain::bucket_unit(q.bucket.as_deref().unwrap_or("day"))
        .ok_or_else(|| AppError::Validation("bucket must be one of day/week/month".into()))?;
    Ok(Json(service::summary(&state, days, unit).await?))
}

#[derive(Debug, Deserialize)]
pub struct ClientEventBody {
    pub feature: String,
    pub meta: Option<serde_json::Value>,
}

/// POST /api/usage/events — クライアント側で完結する機能（TTS 等）の利用申告。
/// 許可リスト外の feature／不正な meta は 400（サーバー側キーの詐称も遮断）。
pub async fn record_event(Json(body): Json<ClientEventBody>) -> AppResult<StatusCode> {
    if !domain::client_feature_allowed(&body.feature) {
        return Err(AppError::Validation(format!(
            "unknown client feature: {}",
            body.feature
        )));
    }
    if let Some(meta) = &body.meta {
        if !domain::validate_client_meta(&body.feature, meta) {
            return Err(AppError::Validation("invalid meta for feature".into()));
        }
    }
    service::record_client_event(body.feature, body.meta);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn record_event_accepts_allowed_feature_with_valid_meta() {
        let body = ClientEventBody {
            feature: "tts_play".into(),
            meta: Some(json!({ "source": "summary" })),
        };
        // sink 未 install なので record は no-op（DB 不要でハンドラ検証だけ通せる）。
        let status = record_event(Json(body)).await.unwrap();
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn record_event_accepts_meta_less_event() {
        let body = ClientEventBody {
            feature: "tts_play".into(),
            meta: None,
        };
        assert_eq!(
            record_event(Json(body)).await.unwrap(),
            StatusCode::NO_CONTENT
        );
    }

    #[tokio::test]
    async fn record_event_rejects_unknown_feature() {
        let body = ClientEventBody {
            feature: "summarize".into(), // サーバー側キーの詐称
            meta: None,
        };
        assert!(matches!(
            record_event(Json(body)).await,
            Err(AppError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn record_event_rejects_invalid_meta() {
        let body = ClientEventBody {
            feature: "tts_play".into(),
            meta: Some(json!({ "source": "unknown_source" })),
        };
        assert!(matches!(
            record_event(Json(body)).await,
            Err(AppError::Validation(_))
        ));
    }
}
