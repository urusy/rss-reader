use serde::Serialize;

/// A validated login token input. Empty is rejected at construction. No Serialize
/// (we never echo a token back to the client).
#[derive(Debug, Clone)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into();
        let t = s.trim();
        if t.is_empty() {
            return Err("token must not be empty".into());
        }
        Ok(Self(t.to_string()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Safe projection for GET /api/auth/status — only whether auth is required.
#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    pub required: bool,
}

/// Success body for POST /api/auth/login.
#[derive(Debug, Clone, Serialize)]
pub struct LoginResponse {
    pub ok: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_empty() {
        assert!(AuthToken::parse("").is_err());
    }

    #[test]
    fn parse_rejects_whitespace_only() {
        assert!(AuthToken::parse("   ").is_err());
    }

    #[test]
    fn parse_trims_and_keeps_value() {
        let t = AuthToken::parse("  abc123  ").unwrap();
        assert_eq!(t.as_str(), "abc123");
    }
}
