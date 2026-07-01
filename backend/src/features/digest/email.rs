use chrono::NaiveDate;

use crate::shared::error::AppResult;
use crate::shared::state::AppState;

/// Send the digest by email when SMTP config (host/from/to) is all present.
/// Stub: SMTP is intentionally not wired yet (no `lettre` dependency). When all
/// config is set we log intent; a future task adds the actual send. Failures here
/// are never fatal to digest generation (the caller logs and continues).
pub async fn maybe_send(state: &AppState, date: NaiveDate, markdown: &str) -> AppResult<()> {
    let cfg = &state.config;
    let (Some(host), Some(_from), Some(to)) = (
        cfg.smtp_host.as_ref(),
        cfg.digest_email_from.as_ref(),
        cfg.digest_email_to.as_ref(),
    ) else {
        return Ok(()); // not configured → skip silently
    };
    // smtp_port/username/password are read here so they're wired for the future
    // real send; the actual lettre transport is a follow-up task.
    tracing::info!(
        %date,
        host = %host,
        port = cfg.smtp_port,
        auth = cfg.smtp_username.is_some() && cfg.smtp_password.is_some(),
        to = %to,
        bytes = markdown.len(),
        "digest email configured but SMTP sending is not yet implemented (stub)"
    );
    Ok(())
}
