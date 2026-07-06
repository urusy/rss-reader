//! ログイン機能の値オブジェクト。パスワード・セッショントークンは
//! `Serialize` を実装しない（クライアントへ二度と返さない）。

use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::shared::auth::hash_token;

/// パスワードの最小/最大文字数。上限はハッシュ前の DoS 防止
/// （Argon2 は入力長に比例して遅くなるため無制限にしない）。
pub const PASSWORD_MIN_CHARS: usize = 8;
pub const PASSWORD_MAX_CHARS: usize = 128;

/// 検証済みパスワード入力。空白のみ・短すぎ・長すぎを構築時に弾く。
/// 前後の空白は落とさない（空白もパスワードの一部として尊重する）。
#[derive(Clone)]
pub struct Password(String);

impl Password {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into();
        if s.trim().is_empty() {
            return Err("password must not be empty".into());
        }
        let chars = s.chars().count();
        if chars < PASSWORD_MIN_CHARS {
            return Err(format!(
                "password must be at least {PASSWORD_MIN_CHARS} characters"
            ));
        }
        if chars > PASSWORD_MAX_CHARS {
            return Err(format!(
                "password must be at most {PASSWORD_MAX_CHARS} characters"
            ));
        }
        Ok(Self(s))
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

// Debug でも中身を出さない（ログ・panic メッセージへの漏洩防止）。
impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(***)")
    }
}

/// セッショントークン（平文）。ログイン時に生成して Set-Cookie で渡す一度きり。
/// DB にはハッシュ（`token_hash`）のみ保存する。
pub struct SessionToken(String);

impl SessionToken {
    /// OS 乱数 32 バイト → base64url(no pad, 43文字)。
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Self(Base64UrlUnpadded::encode_string(&bytes))
    }

    /// Cookie 値としてクライアントへ渡す平文。
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// DB 保存用ハッシュ。
    pub fn token_hash(&self) -> String {
        hash_token(&self.0)
    }
}

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionToken(***)")
    }
}

/// GET /api/auth/status の公開射影。値は状態のみで秘密を含まない。
#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    /// 初回セットアップ（パスワード設定）が未完了か。
    pub setup_required: bool,
    /// このリクエストが有効なセッション Cookie を伴っていたか。
    pub authenticated: bool,
}

/// GET /api/auth/sessions の一覧射影。トークンハッシュは公開しない。
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: Uuid,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    /// リクエスト元自身のセッションか（UI で「このデバイス」表示に使う）。
    pub current: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_rejects_empty_and_whitespace() {
        assert!(Password::parse("").is_err());
        assert!(Password::parse("        ").is_err());
    }

    #[test]
    fn password_rejects_too_short() {
        assert!(Password::parse("1234567").is_err());
    }

    #[test]
    fn password_accepts_min_and_max_boundary() {
        assert!(Password::parse("12345678").is_ok());
        assert!(Password::parse("a".repeat(128)).is_ok());
    }

    #[test]
    fn password_rejects_over_max() {
        assert!(Password::parse("a".repeat(129)).is_err());
    }

    #[test]
    fn password_counts_unicode_chars_not_bytes() {
        // 8 文字の日本語（バイト数では 24）を許可する。
        assert!(Password::parse("あいうえおかきく").is_ok());
    }

    #[test]
    fn password_debug_hides_value() {
        let p = Password::parse("super-secret-pw").unwrap();
        assert_eq!(format!("{p:?}"), "Password(***)");
    }

    #[test]
    fn token_is_43_chars_base64url() {
        let t = SessionToken::generate();
        assert_eq!(t.expose().len(), 43);
        assert!(t
            .expose()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn tokens_are_unique() {
        let a = SessionToken::generate();
        let b = SessionToken::generate();
        assert_ne!(a.expose(), b.expose());
    }

    #[test]
    fn token_hash_is_deterministic_and_hides_token() {
        let t = SessionToken::generate();
        assert_eq!(t.token_hash(), hash_token(t.expose()));
        assert_ne!(t.token_hash(), t.expose());
        // SHA-256 → base64url no pad = 43 文字。
        assert_eq!(t.token_hash().len(), 43);
    }

    #[test]
    fn token_debug_hides_value() {
        let t = SessionToken::generate();
        assert_eq!(format!("{t:?}"), "SessionToken(***)");
    }
}
