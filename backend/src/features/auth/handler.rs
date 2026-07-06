//! auth スライスの HTTP 層。401/403/409/429 は AppError にバリアントが無いので
//! 生 `Response` で表現する（shared/auth.rs のヘルパを利用）。
//! パスワードはリクエスト body 以外のどこにも現れない（ログ・レスポンス禁止）。

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::domain::{AuthStatus, Password, SessionInfo};
use super::service::{self, ChangePasswordOutcome, LoginOutcome, SetupOutcome};
use crate::shared::auth::{
    session_from_headers, too_many_requests, unauthorized, CurrentSession, SESSION_COOKIE,
    SESSION_TTL_DAYS,
};
use crate::shared::error::AppError;
use crate::shared::state::AppState;

/// GET /api/auth/status — 公開。フロントがゲート表示を決めるための状態のみ返す。
pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthStatus>, Response> {
    let authenticated = session_from_headers(&state, &headers)
        .await
        .map_err(internal)?
        .is_some();
    let st = service::status(&state, authenticated)
        .await
        .map_err(IntoResponse::into_response)?;
    Ok(Json(st))
}

#[derive(Debug, Deserialize)]
pub struct PasswordBody {
    pub password: String,
}

/// POST /api/auth/setup — 公開（credential が無い間だけ成功）。成功 = ログイン扱い。
pub async fn setup(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(body): Json<PasswordBody>,
) -> Result<(CookieJar, Json<serde_json::Value>), Response> {
    let password = parse_password(body.password).map_err(IntoResponse::into_response)?;
    let label = device_label(&headers);
    match service::setup(&state, password, label.as_deref())
        .await
        .map_err(IntoResponse::into_response)?
    {
        SetupOutcome::Ok(token) => {
            tracing::info!("initial password configured");
            let jar = jar.add(session_cookie(&state, token.expose()));
            Ok((jar, Json(json!({ "ok": true }))))
        }
        SetupOutcome::AlreadyConfigured => Err(conflict("already configured")),
    }
}

/// POST /api/auth/login — 公開。成功時のみ Set-Cookie。失敗理由は出し分けない
/// （401 の本文は常に同一。バックオフ中だけ 429）。
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(body): Json<PasswordBody>,
) -> Result<(CookieJar, Json<serde_json::Value>), Response> {
    let password = parse_password(body.password).map_err(IntoResponse::into_response)?;
    let label = device_label(&headers);
    match service::login(&state, password, label.as_deref())
        .await
        .map_err(IntoResponse::into_response)?
    {
        LoginOutcome::Ok(token) => {
            let jar = jar.add(session_cookie(&state, token.expose()));
            Ok((jar, Json(json!({ "ok": true }))))
        }
        LoginOutcome::InvalidPassword => {
            tracing::warn!("login failed: invalid password");
            Err(unauthorized())
        }
        LoginOutcome::SetupRequired => Err(conflict("setup required")),
        LoginOutcome::RateLimited(remaining) => Err(too_many_requests(remaining)),
    }
}

/// POST /api/auth/logout — 保護。現セッションを失効し Cookie を破棄。
pub async fn logout(
    State(state): State<AppState>,
    Extension(current): Extension<CurrentSession>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), Response> {
    service::logout(&state, current)
        .await
        .map_err(IntoResponse::into_response)?;
    Ok((jar.remove(removal_cookie()), StatusCode::NO_CONTENT))
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordBody {
    pub current_password: String,
    pub new_password: String,
}

/// PUT /api/auth/password — 保護。成功時は他セッションを全失効。
pub async fn change_password(
    State(state): State<AppState>,
    Extension(current): Extension<CurrentSession>,
    Json(body): Json<ChangePasswordBody>,
) -> Result<StatusCode, Response> {
    let current_password =
        parse_password(body.current_password).map_err(IntoResponse::into_response)?;
    let new_password = parse_password(body.new_password).map_err(IntoResponse::into_response)?;
    match service::change_password(&state, current, current_password, new_password)
        .await
        .map_err(IntoResponse::into_response)?
    {
        ChangePasswordOutcome::Ok => Ok(StatusCode::NO_CONTENT),
        ChangePasswordOutcome::InvalidCurrent => {
            tracing::warn!("password change failed: invalid current password");
            Err(unauthorized())
        }
        ChangePasswordOutcome::RateLimited(remaining) => Err(too_many_requests(remaining)),
    }
}

/// GET /api/auth/sessions — 保護。有効セッション一覧（current フラグ付き）。
pub async fn list_sessions(
    State(state): State<AppState>,
    Extension(current): Extension<CurrentSession>,
) -> Result<Json<Vec<SessionInfo>>, Response> {
    let sessions = service::list_sessions(&state, current)
        .await
        .map_err(IntoResponse::into_response)?;
    Ok(Json(sessions))
}

/// DELETE /api/auth/sessions/{id} — 保護。個別失効（自分自身も可 = 実質 logout）。
pub async fn revoke_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, Response> {
    service::revoke_session(&state, id)
        .await
        .map_err(IntoResponse::into_response)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- 内部ヘルパ -----------------------------------------------------------

fn parse_password(raw: String) -> Result<Password, AppError> {
    Password::parse(raw).map_err(AppError::Validation)
}

/// セッション Cookie。SameSite=Strict + HttpOnly。Secure は COOKIE_SECURE で
/// opt-in（http 運用の LAN で強制すると Cookie が保存されない）。
fn session_cookie(state: &AppState, token: &str) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token.to_string()))
        .http_only(true)
        .same_site(SameSite::Strict)
        .path("/")
        .max_age(time::Duration::days(SESSION_TTL_DAYS))
        .secure(state.config.cookie_secure)
        .build()
}

/// 削除用 Cookie（発行時と同じ path でないとブラウザが消さない）。
fn removal_cookie() -> Cookie<'static> {
    Cookie::build(SESSION_COOKIE).path("/").build()
}

/// User-Agent 先頭を端末ラベルとして保存（セッション一覧の識別用）。
fn device_label(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| ua.chars().take(120).collect())
}

fn conflict(msg: &str) -> Response {
    (StatusCode::CONFLICT, Json(json!({ "error": msg }))).into_response()
}

fn internal(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "auth status lookup failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "internal error" })),
    )
        .into_response()
}
