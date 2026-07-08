//! HTTP 境界。GReader ワイヤ規約（text/plain の "OK"・独自 401 ヘッダ・
//! 403 BadAuthentication）に合わせるため、ハンドラは `AppResult` を
//! `IntoResponse` に任せず `to_sync_response` で明示変換する
//! （`AppError::Database` の JSON 形をワイヤに漏らさない）。

use axum::body::{Body, Bytes};
use axum::extract::{Path, RawQuery, State};
use axum::http::{header, HeaderMap, Method, Request, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use uuid::Uuid;

use super::domain::{parse_google_login_header, StreamId};
use super::repository as repo;
use super::service;
use super::wire::{self, Params};
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

// ---- 認証ミドルウェア -----------------------------------------------------------

/// 認証ミドルウェア。per-handler extractor でなくルーター単位の layer なのは、
/// extractor は書き忘れ = 無認証公開になるのに対し layer は secure-by-default
/// のため（`require_auth` と同型）。GET / POST とも Authorization ヘッダで認証
/// （FreshRSS モデル。Miniflux 式「POST は T= で認証」は T を送らない Fluent を
/// 壊す）。`T=` は存在しても一切検証しない。
/// ★不変条件: sync ルートは Cookie を読まず、GoogleLogin トークンは /api/* で
/// 受理されない（§9.2 でテスト固定）。
pub async fn require_sync_auth(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_google_login_header);
    let Some(token) = token else {
        return wire::unauthorized_sync();
    };
    match service::verify_sync_token(&state, token).await {
        Ok(Some(_)) => next.run(req).await,
        Ok(None) => wire::unauthorized_sync(),
        Err(e) => {
            tracing::error!(error = %e, "greader: token verification failed");
            wire::internal_error()
        }
    }
}

/// AppResult → GReader ワイヤ形。エラー詳細は tracing のみ（内部情報非開示）。
fn to_sync_response<T>(r: AppResult<T>, ok: impl FnOnce(T) -> Response) -> Response {
    match r {
        Ok(v) => ok(v),
        Err(e) => {
            tracing::error!(error = %e, "greader: handler error");
            wire::internal_error()
        }
    }
}

fn json_ok<T: serde::Serialize>(v: T) -> Response {
    Json(v).into_response()
}

// ---- 認証系ハンドラ --------------------------------------------------------------

/// POST のみ（GET はルート未登録 → Passwd がクエリ文字列 = アクセスログに載る
/// 事故を排除）。query は意図的に見ない。
pub async fn client_login(State(state): State<AppState>, body: Bytes) -> Response {
    let p = Params::from(None, &body);
    let email = p.first("Email");
    let passwd = p.first("Passwd").unwrap_or("");
    match service::client_login(&state, email, passwd).await {
        Ok(service::ClientLoginOutcome::Ok(token)) => wire::client_login_ok(token.as_str()),
        Ok(service::ClientLoginOutcome::BadCredentials) => wire::bad_auth_clientlogin(),
        Ok(service::ClientLoginOutcome::RateLimited(d)) => wire::rate_limited_clientlogin(d),
        Err(e) => {
            tracing::error!(error = %e, "greader: ClientLogin failed");
            wire::internal_error()
        }
    }
}

/// 提示された auth トークンをそのまま返す（Miniflux 方式「edit token = auth
/// token」）。認証ミドルウェアの内側なのでヘッダは検証済み。
pub async fn token(headers: HeaderMap) -> Response {
    let t = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_google_login_header)
        .unwrap_or_default();
    wire::token_response(t)
}

pub async fn user_info() -> Response {
    json_ok(wire::UserInfo::single_user())
}

// ---- 読み取り系ハンドラ ------------------------------------------------------------

pub async fn tag_list(State(state): State<AppState>) -> Response {
    to_sync_response(service::tag_list(&state).await, json_ok)
}

pub async fn subscription_list(State(state): State<AppState>) -> Response {
    to_sync_response(service::subscription_list(&state).await, json_ok)
}

pub async fn unread_count(State(state): State<AppState>) -> Response {
    to_sync_response(service::unread_count_payload(&state).await, json_ok)
}

pub async fn stream_items_ids(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
) -> Response {
    let p = Params::from(query.as_deref(), b"");
    let stream = p
        .first("s")
        .map(StreamId::parse)
        .unwrap_or(StreamId::ReadingList);
    to_sync_response(service::item_ids(&state, &stream, &p).await, json_ok)
}

pub async fn stream_items_contents(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let p = Params::from(query.as_deref(), &body);
    to_sync_response(service::items_contents(&state, &p).await, json_ok)
}

/// bare 形（Fluent Reader はこれしか呼ばない）。?s= 変種も受理。
pub async fn stream_contents_bare(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
) -> Response {
    let p = Params::from(query.as_deref(), b"");
    let stream = p
        .first("s")
        .map(StreamId::parse)
        .unwrap_or(StreamId::ReadingList);
    to_sync_response(service::stream_contents(&state, &stream, &p).await, json_ok)
}

/// パス形 `/stream/contents/{*stream}`（axum の Path が percent-decode 済み）。
pub async fn stream_contents_path(
    State(state): State<AppState>,
    Path(stream): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let p = Params::from(query.as_deref(), b"");
    let stream = StreamId::parse(&stream);
    to_sync_response(service::stream_contents(&state, &stream, &p).await, json_ok)
}

// ---- 書き込み系ハンドラ（成功は literal "OK"） ---------------------------------------

pub async fn edit_tag(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let p = Params::from(query.as_deref(), &body);
    to_sync_response(service::edit_tag(&state, &p).await, |()| wire::ok_plain())
}

pub async fn mark_all_as_read(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let p = Params::from(query.as_deref(), &body);
    to_sync_response(service::mark_all_as_read(&state, &p).await, |()| {
        wire::ok_plain()
    })
}

pub async fn quickadd(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let p = Params::from(query.as_deref(), &body);
    let Some(url) = p.first("quickadd").map(str::to_string) else {
        return json_ok(wire::QuickAddResult {
            num_results: 0,
            query: String::new(),
            stream_id: None,
            stream_name: None,
        });
    };
    to_sync_response(service::quick_add(&state, &url).await, json_ok)
}

pub async fn subscription_edit(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let p = Params::from(query.as_deref(), &body);
    to_sync_response(service::subscription_edit(&state, &p).await, |()| {
        wire::ok_plain()
    })
}

pub async fn rename_tag(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let p = Params::from(query.as_deref(), &body);
    to_sync_response(service::rename_folder(&state, &p).await, |()| {
        wire::ok_plain()
    })
}

pub async fn disable_tag(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    let p = Params::from(query.as_deref(), &body);
    to_sync_response(service::delete_folders(&state, &p).await, |()| {
        wire::ok_plain()
    })
}

// ---- catch-all --------------------------------------------------------------------

/// 未実装パスは認証済みなら 200 `[]`（クライアントの未知プローブを飲み込む）。
/// ただし必ず warn ログを残す — 実装漏れとプローブを黙って隠さない。
pub async fn catch_all(method: Method, uri: Uri) -> Response {
    tracing::warn!(%method, path = %uri.path(), "greader: unimplemented endpoint");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        "[]",
    )
        .into_response()
}

// ---- トークン管理（protected 側 = Cookie セッション保護の通常 /api） ------------------

pub async fn list_sync_tokens(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<repo::TokenRow>>> {
    Ok(Json(repo::list_tokens(&state.db).await?))
}

pub async fn revoke_sync_token(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    if repo::delete_token(&state.db, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}
