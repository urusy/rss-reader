use serde::Serialize;

use crate::features::articles::domain::ArticleId;

/// 検証済みの資格情報入力。空文字は構築時に弾く（不正状態を表現不能にする）。
/// password を持つので Serialize は付けない（クライアントに漏らさない）。
#[derive(Debug, Clone)]
pub struct InstapaperCredentials {
    username: String,
    password: String,
}

impl InstapaperCredentials {
    pub fn parse(username: impl Into<String>, password: impl Into<String>) -> Result<Self, String> {
        let username = username.into().trim().to_string();
        let password = password.into(); // パスワードは前後空白も有意なので trim しない
        if username.is_empty() {
            return Err("username must not be empty".into());
        }
        if password.is_empty() {
            return Err("password must not be empty".into());
        }
        Ok(Self { username, password })
    }
    pub fn username(&self) -> &str {
        &self.username
    }
    pub fn password(&self) -> &str {
        &self.password
    }
}

/// DB から読んだ生の資格情報（add 時の Basic 認証に使う）。
/// Serialize は付けない（password 漏洩防止）。updated_at は読まないので持たない。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredCredentials {
    pub username: String,
    pub password: String,
}

/// GET /status が返す安全な射影。configured のみ公開。
#[derive(Debug, Clone, Serialize)]
pub struct InstapaperStatus {
    pub configured: bool,
}

/// Instapaper へ送る URL の値オブジェクト。FeedUrl と同じスキーム検査だが、
/// スライス越境結合を避けるため instapaper スライス内に閉じる。
#[derive(Debug, Clone)]
pub struct SaveUrl(String);

impl SaveUrl {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into();
        let t = s.trim();
        if !(t.starts_with("http://") || t.starts_with("https://")) {
            return Err("url must start with http:// or https://".into());
        }
        Ok(Self(t.to_string()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Instapaper /api/add のステータスコード分類（純粋関数 = 単体テスト対象）。
#[derive(Debug, PartialEq, Eq)]
pub enum AddOutcome {
    Saved,      // 200/201
    BadRequest, // 400（URL 不正など、クライアント修正可能）
    Failed,     // 403/5xx/その他（資格情報不正・障害）
}

pub fn classify_add_status(code: u16) -> AddOutcome {
    match code {
        200 | 201 => AddOutcome::Saved,
        400 => AddOutcome::BadRequest,
        _ => AddOutcome::Failed,
    }
}

/// Instapaper /api/authenticate のステータスコード分類（純粋関数 = 単体テスト対象）。
#[derive(Debug, PartialEq, Eq)]
pub enum AuthOutcome {
    Valid,   // 200
    Invalid, // 403（資格情報が誤り → フォームにエラー表示したい）
    Failed,  // その他（障害）
}

pub fn classify_auth_status(code: u16) -> AuthOutcome {
    match code {
        200 => AuthOutcome::Valid,
        403 => AuthOutcome::Invalid,
        _ => AuthOutcome::Failed,
    }
}

// ---- 機能06「後で読む」: read_later_items のドメイン ----

/// read_later_items 1 行をミラーする。status は DB の CHECK 制約で 3 値に限定される。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReadLaterItem {
    pub article_id: ArticleId,
    pub status: String, // "pending" | "added" | "failed"
    pub instapaper_added_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// 保存状態の文字列を 1 箇所に固定（マイグレーション 0004 の CHECK 制約と一致させる）。
/// PENDING/FAILED は SQL リテラルとして書き込み、Rust 側の分岐で参照するのは ADDED のみ。
/// 残り2つは CHECK 制約との一致を担保するドキュメント兼テスト基準なので allow(dead_code)。
#[allow(dead_code)]
pub mod read_later_status {
    pub const PENDING: &str = "pending";
    pub const ADDED: &str = "added";
    pub const FAILED: &str = "failed";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_empty_username() {
        assert!(InstapaperCredentials::parse("", "pw").is_err());
        assert!(InstapaperCredentials::parse("   ", "pw").is_err());
    }

    #[test]
    fn parse_rejects_empty_password() {
        assert!(InstapaperCredentials::parse("user", "").is_err());
    }

    #[test]
    fn parse_trims_username() {
        let c = InstapaperCredentials::parse("  user@example.com  ", "pw").unwrap();
        assert_eq!(c.username(), "user@example.com");
    }

    #[test]
    fn parse_keeps_password_verbatim() {
        let c = InstapaperCredentials::parse("user", "  pw ").unwrap();
        assert_eq!(c.password(), "  pw ");
    }

    #[test]
    fn parse_accepts_valid_credentials() {
        let c = InstapaperCredentials::parse("user", "pw").unwrap();
        assert_eq!(c.username(), "user");
        assert_eq!(c.password(), "pw");
    }

    #[test]
    fn save_url_accepts_http_and_https() {
        assert!(SaveUrl::parse("http://example.com/a").is_ok());
        assert!(SaveUrl::parse("https://example.com/a").is_ok());
    }

    #[test]
    fn save_url_rejects_missing_scheme() {
        assert!(SaveUrl::parse("example.com/a").is_err());
    }

    #[test]
    fn save_url_rejects_empty() {
        assert!(SaveUrl::parse("").is_err());
    }

    #[test]
    fn classify_add_status_maps_2xx_to_saved() {
        assert_eq!(classify_add_status(200), AddOutcome::Saved);
        assert_eq!(classify_add_status(201), AddOutcome::Saved);
    }

    #[test]
    fn classify_add_status_maps_400_to_bad_request() {
        assert_eq!(classify_add_status(400), AddOutcome::BadRequest);
    }

    #[test]
    fn classify_add_status_maps_403_and_5xx_to_failed() {
        assert_eq!(classify_add_status(403), AddOutcome::Failed);
        assert_eq!(classify_add_status(500), AddOutcome::Failed);
    }

    #[test]
    fn classify_auth_status_maps_200_valid_403_invalid_else_failed() {
        assert_eq!(classify_auth_status(200), AuthOutcome::Valid);
        assert_eq!(classify_auth_status(403), AuthOutcome::Invalid);
        assert_eq!(classify_auth_status(500), AuthOutcome::Failed);
    }

    #[test]
    fn status_constants_match_db_check() {
        // マイグレーション 0004 の CHECK (status IN ('pending','added','failed')) と一致。
        assert_eq!(read_later_status::PENDING, "pending");
        assert_eq!(read_later_status::ADDED, "added");
        assert_eq!(read_later_status::FAILED, "failed");
    }
}
