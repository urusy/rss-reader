use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::service::{self, SavedState};
use crate::features::articles::domain::{Article, ArticleId};
use crate::shared::auth::constant_time_eq;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SaveBody {
    pub url: String,
}

/// POST /api/saved（Cookie 保護面）。201 即返し、抽出は背景。
pub async fn save(
    State(state): State<AppState>,
    Json(body): Json<SaveBody>,
) -> AppResult<(StatusCode, Json<Article>)> {
    let article = service::save_url(&state, &body.url).await?;
    Ok((StatusCode::CREATED, Json(article)))
}

/// POST /api/save（トークン保存面）。動作は save と同一で、認証だけが
/// require_save_token（Bearer）になる。iOS ショートカット / ブラウザ拡張用。
pub async fn capture(
    State(state): State<AppState>,
    Json(body): Json<SaveBody>,
) -> AppResult<(StatusCode, Json<Article>)> {
    let article = service::save_url(&state, &body.url).await?;
    Ok((StatusCode::CREATED, Json(article)))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub state: Option<SavedState>,
    #[serde(default)]
    pub unread: Option<bool>,
}

/// GET /api/saved?state=inbox|archived|all&unread=true
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Article>>> {
    let filter = q.state.unwrap_or(SavedState::Inbox);
    let items = service::list_saved(&state, filter, q.unread.unwrap_or(false)).await?;
    Ok(Json(items))
}

#[derive(Debug, Deserialize)]
pub struct ArchiveBody {
    pub archived: bool,
}

/// PATCH /api/saved/{article_id} {"archived": bool}
pub async fn set_archived(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ArchiveBody>,
) -> AppResult<StatusCode> {
    service::set_archived(&state, ArticleId(id), body.archived).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/saved/{article_id}
pub async fn delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    service::delete_saved(&state, ArticleId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 保存 API（public 面）の Bearer 認証。SAVE_TOKEN（.env 固定トークン）と
/// constant_time_eq で照合する（backup の X-Backup-Token と同方式）。
/// ルーター単位の layer として掛ける（extractor の書き忘れ = 無認証公開を
/// 構造的に防ぐ。sync/mod.rs の判断を踏襲）。
pub async fn require_save_token(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let ok = match (presented, state.config.save_token.as_deref()) {
        (Some(t), Some(expected)) => constant_time_eq(t.as_bytes(), expected.as_bytes()),
        _ => false,
    };
    if ok {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid save token" })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_state_parses_lowercase_and_rejects_unknown() {
        assert_eq!(
            serde_json::from_str::<SavedState>(r#""inbox""#).unwrap(),
            SavedState::Inbox
        );
        assert_eq!(
            serde_json::from_str::<SavedState>(r#""archived""#).unwrap(),
            SavedState::Archived
        );
        assert!(serde_json::from_str::<SavedState>(r#""bogus""#).is_err());
    }

    #[test]
    fn list_query_defaults_are_none() {
        let q: ListQuery = serde_json::from_str("{}").unwrap();
        assert!(q.state.is_none());
        assert!(q.unread.is_none());
    }
}
