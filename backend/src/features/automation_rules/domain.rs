//! Rules engine domain: typed conditions/actions + pure match/validate logic.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RuleName(String);

impl RuleName {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into().trim().to_string();
        if s.is_empty() {
            return Err("rule name must not be empty".into());
        }
        if s.chars().count() > 100 {
            return Err("rule name too long (max 100)".into());
        }
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Combinator {
    All,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeywordTarget {
    Title,
    Content,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOp {
    OlderThanDays,
    NewerThanDays,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum Condition {
    Keyword {
        target: KeywordTarget,
        value: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    Author {
        value: String,
    },
    Feed {
        feed_ids: Vec<Uuid>,
    },
    Tag {
        tag: String,
    },
    Date {
        op: DateOp,
        days: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conditions {
    pub combinator: Combinator,
    pub items: Vec<Condition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    MarkRead,
    Star,
    Tag { name: String },
    Save,
    Score { delta: i32 },
}

pub struct ArticleCtx<'a> {
    pub title: &'a str,
    pub content: &'a str,
    pub author: Option<&'a str>,
    pub feed_id: Uuid,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub tags: &'a [String],
    pub now: chrono::DateTime<chrono::Utc>,
}

fn contains(haystack: &str, needle: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        haystack.contains(needle)
    } else {
        haystack.to_lowercase().contains(&needle.to_lowercase())
    }
}

pub fn match_condition(c: &Condition, ctx: &ArticleCtx) -> bool {
    match c {
        Condition::Keyword {
            target,
            value,
            case_sensitive,
        } => match target {
            KeywordTarget::Title => contains(ctx.title, value, *case_sensitive),
            KeywordTarget::Content => contains(ctx.content, value, *case_sensitive),
            KeywordTarget::Any => {
                contains(ctx.title, value, *case_sensitive)
                    || contains(ctx.content, value, *case_sensitive)
            }
        },
        Condition::Author { value } => ctx
            .author
            .map(|a| contains(a, value, false))
            .unwrap_or(false),
        Condition::Feed { feed_ids } => feed_ids.contains(&ctx.feed_id),
        Condition::Tag { tag } => {
            let want = tag.to_lowercase();
            ctx.tags.contains(&want)
        }
        Condition::Date { op, days } => match ctx.published_at {
            None => false,
            Some(p) => {
                let age_days = (ctx.now - p).num_days();
                match op {
                    DateOp::OlderThanDays => age_days > *days,
                    DateOp::NewerThanDays => age_days <= *days,
                }
            }
        },
    }
}

pub fn rule_matches(conds: &Conditions, ctx: &ArticleCtx) -> bool {
    match conds.combinator {
        Combinator::All => conds.items.iter().all(|c| match_condition(c, ctx)),
        Combinator::Any => conds.items.iter().any(|c| match_condition(c, ctx)),
    }
}

pub fn validate_conditions(conds: &Conditions) -> Result<(), String> {
    if conds.items.is_empty() {
        return Err("at least one condition is required".into());
    }
    for c in &conds.items {
        match c {
            Condition::Keyword { value, .. } | Condition::Author { value } => {
                if value.trim().is_empty() {
                    return Err("condition value must not be empty".into());
                }
            }
            Condition::Tag { tag } => {
                if tag.trim().is_empty() {
                    return Err("tag must not be empty".into());
                }
            }
            Condition::Feed { feed_ids } => {
                if feed_ids.is_empty() {
                    return Err("feed condition needs at least one feed".into());
                }
            }
            Condition::Date { days, .. } => {
                if *days < 0 {
                    return Err("days must be non-negative".into());
                }
            }
        }
    }
    Ok(())
}

pub fn validate_actions(actions: &[Action]) -> Result<(), String> {
    if actions.is_empty() {
        return Err("at least one action is required".into());
    }
    for a in actions {
        match a {
            Action::Tag { name } => {
                if name.trim().is_empty() {
                    return Err("tag action needs a non-empty name".into());
                }
            }
            Action::Score { delta } => {
                if *delta == 0 {
                    return Err("score delta must not be zero".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(title: &'a str, content: &'a str, tags: &'a [String]) -> ArticleCtx<'a> {
        ArticleCtx {
            title,
            content,
            author: None,
            feed_id: Uuid::nil(),
            published_at: None,
            tags,
            now: chrono::Utc::now(),
        }
    }

    #[test]
    fn keyword_title_case_insensitive() {
        let c = Condition::Keyword {
            target: KeywordTarget::Title,
            value: "RUST".into(),
            case_sensitive: false,
        };
        assert!(match_condition(&c, &ctx("Learning rust", "", &[])));
        assert!(!match_condition(&c, &ctx("cooking", "rust", &[])));
    }

    #[test]
    fn keyword_any_checks_both() {
        let c = Condition::Keyword {
            target: KeywordTarget::Any,
            value: "db".into(),
            case_sensitive: false,
        };
        assert!(match_condition(&c, &ctx("x", "about DB stuff", &[])));
    }

    #[test]
    fn keyword_case_sensitive() {
        let c = Condition::Keyword {
            target: KeywordTarget::Title,
            value: "Rust".into(),
            case_sensitive: true,
        };
        assert!(match_condition(&c, &ctx("Rust", "", &[])));
        assert!(!match_condition(&c, &ctx("rust", "", &[])));
    }

    #[test]
    fn feed_condition() {
        let id = Uuid::from_bytes([7; 16]);
        let c = Condition::Feed { feed_ids: vec![id] };
        let mut cx = ctx("", "", &[]);
        cx.feed_id = id;
        assert!(match_condition(&c, &cx));
        cx.feed_id = Uuid::nil();
        assert!(!match_condition(&c, &cx));
    }

    #[test]
    fn tag_condition_case_insensitive() {
        let c = Condition::Tag { tag: "Rust".into() };
        let tags = vec!["rust".to_string()];
        assert!(match_condition(&c, &ctx("", "", &tags)));
        assert!(!match_condition(&c, &ctx("", "", &[])));
    }

    #[test]
    fn date_older_newer() {
        let now = chrono::Utc::now();
        let mut cx = ctx("", "", &[]);
        cx.now = now;
        cx.published_at = Some(now - chrono::Duration::days(40));
        assert!(match_condition(
            &Condition::Date {
                op: DateOp::OlderThanDays,
                days: 30
            },
            &cx
        ));
        assert!(!match_condition(
            &Condition::Date {
                op: DateOp::NewerThanDays,
                days: 30
            },
            &cx
        ));
    }

    #[test]
    fn date_none_never_matches() {
        let c = Condition::Date {
            op: DateOp::OlderThanDays,
            days: 1,
        };
        assert!(!match_condition(&c, &ctx("", "", &[])));
    }

    #[test]
    fn rule_all_vs_any() {
        let conds = |comb| Conditions {
            combinator: comb,
            items: vec![
                Condition::Keyword {
                    target: KeywordTarget::Title,
                    value: "a".into(),
                    case_sensitive: false,
                },
                Condition::Keyword {
                    target: KeywordTarget::Title,
                    value: "z".into(),
                    case_sensitive: false,
                },
            ],
        };
        let cx = ctx("has a only", "", &[]);
        assert!(!rule_matches(&conds(Combinator::All), &cx));
        assert!(rule_matches(&conds(Combinator::Any), &cx));
    }

    #[test]
    fn validate_rejects_empty_conditions() {
        assert!(validate_conditions(&Conditions {
            combinator: Combinator::All,
            items: vec![]
        })
        .is_err());
    }

    #[test]
    fn validate_rejects_empty_keyword_value() {
        assert!(validate_conditions(&Conditions {
            combinator: Combinator::All,
            items: vec![Condition::Keyword {
                target: KeywordTarget::Title,
                value: "  ".into(),
                case_sensitive: false
            }]
        })
        .is_err());
    }

    #[test]
    fn validate_rejects_negative_days() {
        assert!(validate_conditions(&Conditions {
            combinator: Combinator::All,
            items: vec![Condition::Date {
                op: DateOp::OlderThanDays,
                days: -1
            }]
        })
        .is_err());
    }

    #[test]
    fn validate_actions_rejects_empty_and_zero_score() {
        assert!(validate_actions(&[]).is_err());
        assert!(validate_actions(&[Action::Score { delta: 0 }]).is_err());
        assert!(validate_actions(&[Action::MarkRead]).is_ok());
    }

    #[test]
    fn name_parse() {
        assert!(RuleName::parse("  ").is_err());
        assert_eq!(RuleName::parse("  My Rule ").unwrap().as_str(), "My Rule");
    }

    #[test]
    fn condition_json_roundtrip() {
        let c = Condition::Keyword {
            target: KeywordTarget::Any,
            value: "x".into(),
            case_sensitive: false,
        };
        let j = serde_json::to_string(&c).unwrap();
        assert!(j.contains("\"field\":\"keyword\""));
        let back: Condition = serde_json::from_str(&j).unwrap();
        assert_eq!(back, c);
    }
}
