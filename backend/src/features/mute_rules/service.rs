use serde::Serialize;

use super::domain::{self, MuteRule, MuteRuleId, NewMuteRule, PatchMuteRule};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn list_rules(state: &AppState) -> AppResult<Vec<MuteRule>> {
    repository::list_all(&state.db).await
}

pub async fn create_rule(state: &AppState, input: NewMuteRule) -> AppResult<MuteRule> {
    domain::validate(
        &input.field,
        &input.pattern,
        &input.match_type,
        &input.action,
    )?;
    let rule = repository::insert(
        &state.db,
        &input.field,
        &input.pattern,
        &input.match_type,
        &input.action,
        input.enabled,
    )
    .await?;
    // Apply the new rule to existing articles immediately (additive; new hide
    // rule doesn't disturb existing stamps).
    if rule.enabled {
        repository::apply_rule(&state.db, &rule.field, &rule.pattern, &rule.action).await?;
    }
    Ok(rule)
}

pub async fn update_rule(
    state: &AppState,
    id: MuteRuleId,
    patch: PatchMuteRule,
) -> AppResult<MuteRule> {
    // Validate the merged final shape.
    let current = repository::get(&state.db, id).await?;
    let field = patch.field.as_deref().unwrap_or(&current.field);
    let pattern = patch.pattern.as_deref().unwrap_or(&current.pattern);
    let match_type = patch.match_type.as_deref().unwrap_or(&current.match_type);
    let action = patch.action.as_deref().unwrap_or(&current.action);
    domain::validate(field, pattern, match_type, action)?;

    let rule = repository::update(
        &state.db,
        id,
        patch.field.as_deref(),
        patch.pattern.as_deref(),
        patch.match_type.as_deref(),
        patch.action.as_deref(),
        patch.enabled,
    )
    .await?;
    apply_all(state).await?; // a change may re-show articles → full re-eval
    Ok(rule)
}

pub async fn delete_rule(state: &AppState, id: MuteRuleId) -> AppResult<()> {
    let affected = repository::delete(&state.db, id).await?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    apply_all(state).await?; // re-show what this hide rule was hiding
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ApplyReport {
    pub rules_evaluated: usize,
    pub hidden: u64,
    pub marked_read: u64,
}

/// Re-apply all enabled rules. hide = reset all then re-stamp (idempotent,
/// supports un-hide on delete/disable). mark_read = additive (never reverted).
pub async fn apply_all(state: &AppState) -> AppResult<ApplyReport> {
    let rules = repository::list_all(&state.db).await?;
    let enabled: Vec<&MuteRule> = rules.iter().filter(|r| r.enabled).collect();

    repository::clear_all_hidden(&state.db).await?;

    let mut hidden = 0u64;
    let mut marked_read = 0u64;
    for r in &enabled {
        let n = repository::apply_rule(&state.db, &r.field, &r.pattern, &r.action).await?;
        match r.action.as_str() {
            "hide" => hidden += n,
            "mark_read" => marked_read += n,
            _ => {}
        }
    }
    Ok(ApplyReport {
        rules_evaluated: enabled.len(),
        hidden,
        marked_read,
    })
}
