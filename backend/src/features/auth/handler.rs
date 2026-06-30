use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use super::domain::{AuthStatus, AuthToken, LoginResponse};
use super::service::{self, LoginOutcome};
use crate::shared::auth::unauthorized;
use crate::shared::error::AppError;
use crate::shared::state::AppState;

/// GET /api/auth/status — public; lets the frontend decide whether to gate.
pub async fn status(State(state): State<AppState>) -> Json<AuthStatus> {
    Json(service::auth_status(&state))
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub token: String,
}

/// POST /api/auth/login — verify a token (pre-save / startup check). 401 mismatch
/// is a raw Response (AppError has no 401); empty token is 400 via AppError.
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Result<Json<LoginResponse>, Response> {
    let token = AuthToken::parse(body.token)
        .map_err(AppError::Validation)
        .map_err(IntoResponse::into_response)?;

    match service::verify_login(&state, &token) {
        LoginOutcome::Ok | LoginOutcome::Disabled => Ok(Json(LoginResponse { ok: true })),
        LoginOutcome::Invalid => Err(unauthorized()),
    }
}
