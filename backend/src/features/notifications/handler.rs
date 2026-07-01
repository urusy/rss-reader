//! notifications スライスの HTTP ハンドラ（#31）。全て機能14の認証ガード配下。

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};

use super::domain::PushSubscriptionInput;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Serialize)]
pub struct PublicKeyResponse {
    pub public_key: String,
}

pub async fn public_key(State(state): State<AppState>) -> AppResult<Json<PublicKeyResponse>> {
    Ok(Json(PublicKeyResponse {
        public_key: service::public_key(&state)?,
    }))
}

pub async fn subscribe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PushSubscriptionInput>,
) -> AppResult<StatusCode> {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    service::subscribe(&state, body, ua).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct UnsubscribeBody {
    pub endpoint: String,
}

pub async fn unsubscribe(
    State(state): State<AppState>,
    Json(body): Json<UnsubscribeBody>,
) -> AppResult<StatusCode> {
    service::unsubscribe(&state, &body.endpoint).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct TestResponse {
    pub delivered: usize,
}

pub async fn test(State(state): State<AppState>) -> AppResult<Json<TestResponse>> {
    Ok(Json(TestResponse {
        delivered: service::test_notification(&state).await?,
    }))
}
