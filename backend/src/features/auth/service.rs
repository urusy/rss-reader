use super::domain::{AuthStatus, AuthToken};
use crate::shared::auth::constant_time_eq;
use crate::shared::state::AppState;

/// Whether auth is required (AUTH_TOKEN set). Never reads/returns the token value.
pub fn auth_status(state: &AppState) -> AuthStatus {
    AuthStatus {
        required: state.config.auth_token.is_some(),
    }
}

/// Result of verifying a submitted token; the handler maps this to HTTP status.
#[derive(Debug, PartialEq, Eq)]
pub enum LoginOutcome {
    /// Auth disabled (AUTH_TOKEN unset) → 200 so the frontend skips the gate.
    Disabled,
    /// Token matches → 200.
    Ok,
    /// Token mismatch → 401.
    Invalid,
}

/// Constant-time compare the submitted token against the configured AUTH_TOKEN.
pub fn verify_login(state: &AppState, token: &AuthToken) -> LoginOutcome {
    match state.config.auth_token.as_deref() {
        None => LoginOutcome::Disabled,
        Some(expected) if constant_time_eq(token.as_str().as_bytes(), expected.as_bytes()) => {
            LoginOutcome::Ok
        }
        Some(_) => LoginOutcome::Invalid,
    }
}
