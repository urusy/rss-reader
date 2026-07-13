//! Saved-pages slice (Pocket 風「後で読む」): 任意 URL を保存し、本文を抽出して
//! アプリ内で読む。保存ページは合成フィード（domain::SAVED_FEED_ID）配下の
//! 通常 articles 行なので、スター・タグ・ハイライト・AI・検索は無改修で効く。

pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::post;
use axum::Router;

use crate::shared::state::AppState;

/// Cookie セッション保護面（features/mod.rs の protected 側に merge）。
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/saved", post(handler::save).get(handler::list))
        .route(
            "/api/saved/{article_id}",
            axum::routing::patch(handler::set_archived).delete(handler::delete),
        )
}

/// トークン保存面（features/mod.rs の public 側に merge）。
/// SAVE_TOKEN 未設定なら None を返し、ルート自体が生えない（sync と同じ
/// secure-by-default。認証はルーター単位の layer — extractor の書き忘れで
/// 無認証公開になる事故を構造的に防ぐ）。
pub fn public_routes(state: &AppState) -> Option<Router<AppState>> {
    state.config.save_token.as_ref()?;
    Some(
        Router::new()
            .route("/api/save", post(handler::capture))
            // usage 計測（saved_capture）→ Bearer 認証の順（layer は逆順適用なので
            // 実行時は認証が先 = 401 は記録されない。sync/mod.rs と同じ構成）。
            .layer(axum::middleware::from_fn(
                crate::features::usage::track_usage,
            ))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                handler::require_save_token,
            )),
    )
}
