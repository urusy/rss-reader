//! #31 PWA Web Push 通知スライス。高優先フィードの新着をブラウザ/PWA へ配送する。
//! VAPID 鍵は env（未設定=機能無効）。スケジューラは `service::notify_new_articles`
//! を取得ループ末尾で 1 行呼ぶだけ（越境共通レイヤーは作らない）。

pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/push/public-key", get(handler::public_key))
        .route("/api/push/subscribe", post(handler::subscribe))
        .route("/api/push/unsubscribe", post(handler::unsubscribe))
        .route("/api/push/test", post(handler::test))
}
