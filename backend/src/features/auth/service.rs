//! ログインのユースケース。Argon2 の hash/verify は CPU バウンド（~100ms）
//! なので必ず `spawn_blocking` で実行する。失敗バックオフ（LoginLimiter）は
//! ログインとパスワード変更の両方の検証に適用する（変更経路経由の総当たり防止）。

use std::time::{Duration, Instant};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use uuid::Uuid;

use super::domain::{AuthStatus, Password, SessionInfo, SessionToken};
use super::repository as repo;
use crate::shared::auth::CurrentSession;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

#[derive(Debug)]
pub enum LoginOutcome {
    /// 認証成功。トークンを Set-Cookie でクライアントへ渡す。
    Ok(SessionToken),
    InvalidPassword,
    /// 資格情報が未設定（初回セットアップへ誘導）。
    SetupRequired,
    RateLimited(Duration),
}

#[derive(Debug)]
pub enum SetupOutcome {
    /// セットアップ成功 = そのままログイン扱い（トークン発行）。
    Ok(SessionToken),
    AlreadyConfigured,
}

#[derive(Debug)]
pub enum ChangePasswordOutcome {
    Ok,
    InvalidCurrent,
    RateLimited(Duration),
}

pub async fn status(state: &AppState, authenticated: bool) -> AppResult<AuthStatus> {
    let setup_required = repo::get_credential(&state.db).await?.is_none();
    Ok(AuthStatus {
        setup_required,
        authenticated,
    })
}

/// 初回セットアップ。INSERT の先勝ちで競合を排除する（同時に来た2番目は
/// AlreadyConfigured になる）。
pub async fn setup(
    state: &AppState,
    password: Password,
    label: Option<&str>,
) -> AppResult<SetupOutcome> {
    if repo::get_credential(&state.db).await?.is_some() {
        return Ok(SetupOutcome::AlreadyConfigured);
    }
    let hash = hash_password(password).await?;
    if !repo::insert_credential(&state.db, &hash).await? {
        return Ok(SetupOutcome::AlreadyConfigured);
    }
    let token = create_session(state, label).await?;
    Ok(SetupOutcome::Ok(token))
}

pub async fn login(
    state: &AppState,
    password: Password,
    label: Option<&str>,
) -> AppResult<LoginOutcome> {
    if let Err(remaining) = check_limiter(state) {
        return Ok(LoginOutcome::RateLimited(remaining));
    }
    let Some(phc) = repo::get_credential(&state.db).await? else {
        return Ok(LoginOutcome::SetupRequired);
    };
    if !verify_password(password, phc).await? {
        record_failure(state);
        return Ok(LoginOutcome::InvalidPassword);
    }
    record_success(state);
    // 期限切れセッションの opportunistic GC（失敗しても本流は止めない）。
    if let Err(e) = repo::delete_expired_sessions(&state.db).await {
        tracing::warn!(error = %e, "expired session GC failed");
    }
    let token = create_session(state, label).await?;
    Ok(LoginOutcome::Ok(token))
}

pub async fn logout(state: &AppState, current: CurrentSession) -> AppResult<()> {
    repo::delete_session_by_id(&state.db, current.id).await?;
    Ok(())
}

/// パスワード変更。現パスワード検証にもバックオフを適用し、成功時は
/// 変更を行ったセッション以外を全失効させる（盗まれた端末の締め出し）。
pub async fn change_password(
    state: &AppState,
    current: CurrentSession,
    current_password: Password,
    new_password: Password,
) -> AppResult<ChangePasswordOutcome> {
    if let Err(remaining) = check_limiter(state) {
        return Ok(ChangePasswordOutcome::RateLimited(remaining));
    }
    // 認証済みでしか到達しないので credential は必ず在る（無ければ 404 に落とす）。
    let phc = repo::get_credential(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if !verify_password(current_password, phc).await? {
        record_failure(state);
        return Ok(ChangePasswordOutcome::InvalidCurrent);
    }
    record_success(state);
    let hash = hash_password(new_password).await?;
    repo::update_credential(&state.db, &hash).await?;
    let revoked = repo::delete_sessions_except(&state.db, current.id).await?;
    tracing::info!(revoked, "password changed; other sessions revoked");
    Ok(ChangePasswordOutcome::Ok)
}

pub async fn list_sessions(
    state: &AppState,
    current: CurrentSession,
) -> AppResult<Vec<SessionInfo>> {
    let rows = repo::list_sessions(&state.db).await?;
    Ok(rows
        .into_iter()
        .map(|r| SessionInfo {
            current: r.id == current.id,
            id: r.id,
            label: r.label,
            created_at: r.created_at,
            last_seen_at: r.last_seen_at,
        })
        .collect())
}

pub async fn revoke_session(state: &AppState, id: Uuid) -> AppResult<()> {
    if repo::delete_session_by_id(&state.db, id).await? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

// ---- 内部ヘルパ -----------------------------------------------------------

fn check_limiter(state: &AppState) -> Result<(), Duration> {
    state
        .login_limiter
        .lock()
        .expect("login limiter poisoned")
        .check(Instant::now())
}

fn record_failure(state: &AppState) {
    state
        .login_limiter
        .lock()
        .expect("login limiter poisoned")
        .record_failure(Instant::now());
}

fn record_success(state: &AppState) {
    state
        .login_limiter
        .lock()
        .expect("login limiter poisoned")
        .record_success();
}

async fn create_session(state: &AppState, label: Option<&str>) -> AppResult<SessionToken> {
    let token = SessionToken::generate();
    repo::insert_session(&state.db, &token.token_hash(), label).await?;
    Ok(token)
}

/// Argon2id（既定パラメータ = OWASP 推奨水準）で PHC 文字列へ。
async fn hash_password(password: Password) -> AppResult<String> {
    let hash = tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))
    })
    .await
    .map_err(|e| anyhow::anyhow!("hashing task join failed: {e}"))??;
    Ok(hash)
}

/// PHC 文字列に対する検証。パラメータはハッシュ側の記録値が使われる。
async fn verify_password(password: Password, phc: String) -> AppResult<bool> {
    let ok = tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&phc)
            .map_err(|e| anyhow::anyhow!("stored password hash is invalid: {e}"))?;
        Ok::<_, anyhow::Error>(
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok(),
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("verify task join failed: {e}"))??;
    Ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_hash_roundtrip_argon2id() {
        let pw = Password::parse("correct horse battery").unwrap();
        let phc = hash_password(pw.clone()).await.unwrap();
        assert!(phc.starts_with("$argon2id$v=19$"));
        assert!(verify_password(pw, phc.clone()).await.unwrap());
        let wrong = Password::parse("wrong password!").unwrap();
        assert!(!verify_password(wrong, phc).await.unwrap());
    }

    #[tokio::test]
    async fn same_password_hashes_differently_per_salt() {
        let pw = Password::parse("correct horse battery").unwrap();
        let a = hash_password(pw.clone()).await.unwrap();
        let b = hash_password(pw).await.unwrap();
        assert_ne!(a, b);
    }
}
