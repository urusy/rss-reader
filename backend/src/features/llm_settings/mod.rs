//! 要約/翻訳のモデル・プロンプトを実行時に設定するスライス（設定画面）。
//! singleton override を DB に保持し、articles スライスが実効値を解決して使う。

pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::get;
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/settings/llm",
        get(handler::get_settings).put(handler::update_settings),
    )
}
