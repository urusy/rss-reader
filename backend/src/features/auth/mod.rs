pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::shared::state::AppState;

/// 公開ルート（認証ミドルウェアの外）。ログイン導線だけを晒す。
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/status", get(handler::status))
        .route("/api/auth/setup", post(handler::setup))
        .route("/api/auth/login", post(handler::login))
}

/// 保護ルート（有効セッション必須）。features/mod.rs の protected 側へ merge する。
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/logout", post(handler::logout))
        .route("/api/auth/password", put(handler::change_password))
        .route("/api/auth/sessions", get(handler::list_sessions))
        .route("/api/auth/sessions/{id}", delete(handler::revoke_session))
}
