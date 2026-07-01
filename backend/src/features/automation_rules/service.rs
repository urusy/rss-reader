use serde::Serialize;
use uuid::Uuid;

use super::domain::{self, Action, ArticleCtx, Conditions, RuleName};
use super::repository::{self, PendingArticle, RuleRow};
use crate::features::articles;
use crate::features::instapaper;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub position: i32,
    pub conditions: Conditions,
    pub actions: Vec<Action>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn parse_row(row: RuleRow) -> AppResult<Rule> {
    let conditions: Conditions = serde_json::from_str(&row.conditions)
        .map_err(|e| AppError::Other(anyhow::anyhow!("corrupt conditions json: {e}")))?;
    let actions: Vec<Action> = serde_json::from_str(&row.actions)
        .map_err(|e| AppError::Other(anyhow::anyhow!("corrupt actions json: {e}")))?;
    Ok(Rule {
        id: row.id,
        name: row.name,
        enabled: row.enabled,
        position: row.position,
        conditions,
        actions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub async fn list_rules(state: &AppState) -> AppResult<Vec<Rule>> {
    repository::list_all(&state.db)
        .await?
        .into_iter()
        .map(parse_row)
        .collect()
}

pub async fn get_rule(state: &AppState, id: Uuid) -> AppResult<Rule> {
    let row = repository::get(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    parse_row(row)
}

pub async fn create_rule(
    state: &AppState,
    name: String,
    enabled: bool,
    position: i32,
    conditions: Conditions,
    actions: Vec<Action>,
) -> AppResult<Rule> {
    let name = RuleName::parse(name).map_err(AppError::Validation)?;
    domain::validate_conditions(&conditions).map_err(AppError::Validation)?;
    domain::validate_actions(&actions).map_err(AppError::Validation)?;
    let cj = serde_json::to_string(&conditions).map_err(|e| AppError::Other(e.into()))?;
    let aj = serde_json::to_string(&actions).map_err(|e| AppError::Other(e.into()))?;
    let row = repository::insert(&state.db, name.as_str(), enabled, position, &cj, &aj).await?;
    parse_row(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_rule(
    state: &AppState,
    id: Uuid,
    name: String,
    enabled: bool,
    position: i32,
    conditions: Conditions,
    actions: Vec<Action>,
) -> AppResult<Rule> {
    let name = RuleName::parse(name).map_err(AppError::Validation)?;
    domain::validate_conditions(&conditions).map_err(AppError::Validation)?;
    domain::validate_actions(&actions).map_err(AppError::Validation)?;
    let cj = serde_json::to_string(&conditions).map_err(|e| AppError::Other(e.into()))?;
    let aj = serde_json::to_string(&actions).map_err(|e| AppError::Other(e.into()))?;
    let row = repository::update(&state.db, id, name.as_str(), enabled, position, &cj, &aj)
        .await?
        .ok_or(AppError::NotFound)?;
    parse_row(row)
}

pub async fn delete_rule(state: &AppState, id: Uuid) -> AppResult<()> {
    if repository::delete(&state.db, id).await? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Called from the crawl path: apply enabled rules to a feed's pending articles.
pub async fn apply_for_feed(state: &AppState, feed_id: Uuid) -> AppResult<()> {
    let rules = load_enabled(state).await?;
    let pending = repository::fetch_pending(&state.db, feed_id, 500).await?;
    apply_rules_to(state, &rules, &pending).await?;
    let ids: Vec<Uuid> = pending.iter().map(|a| a.id).collect();
    repository::mark_applied(&state.db, &ids).await?;
    Ok(())
}

/// Re-apply enabled rules to all articles (manual backfill, ignores stamp).
pub async fn apply_all(state: &AppState) -> AppResult<usize> {
    let rules = load_enabled(state).await?;
    let articles = repository::fetch_all_articles(&state.db, 5000).await?;
    apply_rules_to(state, &rules, &articles).await?;
    let ids: Vec<Uuid> = articles.iter().map(|a| a.id).collect();
    repository::mark_applied(&state.db, &ids).await?;
    Ok(articles.len())
}

/// Dry run: which recent articles a single rule matches (no DB writes).
pub async fn test_rule(state: &AppState, id: Uuid, sample: i64) -> AppResult<Vec<Uuid>> {
    let rule = get_rule(state, id).await?;
    let articles = repository::fetch_all_articles(&state.db, sample).await?;
    let now = chrono::Utc::now();
    let mut matched = Vec::new();
    for a in &articles {
        let tags = repository::tags_for(&state.db, a.id)
            .await
            .unwrap_or_default();
        let ctx = build_ctx(a, &tags, now);
        if domain::rule_matches(&rule.conditions, &ctx) {
            matched.push(a.id);
        }
    }
    Ok(matched)
}

async fn load_enabled(state: &AppState) -> AppResult<Vec<Rule>> {
    let v: Vec<Rule> = repository::list_enabled(&state.db)
        .await?
        .into_iter()
        .filter_map(|r| parse_row(r).ok()) // skip corrupt rows; don't stop the crawl
        .collect();
    Ok(v)
}

fn build_ctx<'a>(
    a: &'a PendingArticle,
    tags: &'a [String],
    now: chrono::DateTime<chrono::Utc>,
) -> ArticleCtx<'a> {
    ArticleCtx {
        title: &a.title,
        content: &a.content,
        author: a.author.as_deref(),
        feed_id: a.feed_id,
        published_at: a.published_at,
        tags,
        now,
    }
}

async fn apply_rules_to(
    state: &AppState,
    rules: &[Rule],
    articles: &[PendingArticle],
) -> AppResult<()> {
    if rules.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now();
    for a in articles {
        let tags = repository::tags_for(&state.db, a.id)
            .await
            .unwrap_or_default();
        let ctx = build_ctx(a, &tags, now);
        for rule in rules {
            if domain::rule_matches(&rule.conditions, &ctx) {
                for action in &rule.actions {
                    if let Err(e) = apply_action(state, a.id, action).await {
                        tracing::warn!(
                            error = %e, rule = %rule.name, article = %a.id,
                            "rule action failed; continuing"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

async fn apply_action(state: &AppState, article_id: Uuid, action: &Action) -> AppResult<()> {
    match action {
        Action::MarkRead => {
            articles::repository::set_read(&state.db, articles::domain::ArticleId(article_id), true)
                .await
        }
        Action::Score { delta } => repository::bump_score(&state.db, article_id, *delta).await,
        Action::Save => {
            instapaper::service::save_for_later(state, articles::domain::ArticleId(article_id))
                .await
                .map(|_| ())
        }
        Action::Tag { name } => tag_article(state, article_id, name).await,
        Action::Star => star_article(state, article_id).await,
    }
}

/// Attach a tag (writes the #24 tables; errors if they don't exist).
async fn tag_article(state: &AppState, article_id: Uuid, name: &str) -> AppResult<()> {
    let tag_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO tags (id, name, source) VALUES (gen_random_uuid(), $1, 'user')
           ON CONFLICT (lower(name)) DO UPDATE SET name = tags.name
           RETURNING id"#,
    )
    .bind(name.trim())
    .fetch_one(&state.db)
    .await?;
    sqlx::query(
        r#"INSERT INTO article_tags (article_id, tag_id, source)
           VALUES ($1, $2, 'user') ON CONFLICT (article_id, tag_id) DO NOTHING"#,
    )
    .bind(article_id)
    .bind(tag_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Star (writes the #32 table; errors → warned by caller, since #32 is a stub).
async fn star_article(state: &AppState, article_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO article_stars (article_id) VALUES ($1)
           ON CONFLICT (article_id) DO NOTHING"#,
    )
    .bind(article_id)
    .execute(&state.db)
    .await?;
    Ok(())
}
