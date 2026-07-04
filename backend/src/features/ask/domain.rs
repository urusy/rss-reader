//! Ask Claude: conversation validation, context truncation, system-prompt
//! building — all pure (no LLM/DB) → unit-tested.

use serde::{Deserialize, Serialize};

/// Max article-body chars packed into context (single article).
pub const MAX_CONTEXT_CHARS: usize = 12_000;
/// Total budget for the multi-article cross-Ask (divided across articles).
pub const MAX_CONTEXT_CHARS_MULTI: usize = 16_000;
/// 1 メッセージの最大文字数。無制限だと巨大 body がそのまま LLM 入力になる
/// （トークン浪費 + メモリ、監査 LOW）。
pub const MAX_MESSAGE_CHARS: usize = 8_000;
/// 会話の最大メッセージ数（往復の暴走防止）。
pub const MAX_MESSAGES: usize = 40;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AskMessage {
    pub role: String, // "user" | "assistant"
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ArticleContext {
    pub title: String,
    pub body: String,
}

/// Validate the conversation against Anthropic's constraints: non-empty, roles
/// user/assistant only, starts+ends with user, strictly alternating, non-empty
/// content.
pub fn validate_conversation(messages: &[AskMessage]) -> Result<(), String> {
    if messages.is_empty() {
        return Err("messages must not be empty".into());
    }
    if messages.len() > MAX_MESSAGES {
        return Err(format!("too many messages (max {MAX_MESSAGES})"));
    }
    for (i, m) in messages.iter().enumerate() {
        if m.role != "user" && m.role != "assistant" {
            return Err(format!("message[{i}].role must be 'user' or 'assistant'"));
        }
        if m.content.trim().is_empty() {
            return Err(format!("message[{i}].content must not be empty"));
        }
        if m.content.chars().count() > MAX_MESSAGE_CHARS {
            return Err(format!(
                "message[{i}].content exceeds {MAX_MESSAGE_CHARS} chars"
            ));
        }
        let expected = if i % 2 == 0 { "user" } else { "assistant" };
        if m.role != expected {
            return Err(format!(
                "messages must alternate starting with user (message[{i}] should be {expected})"
            ));
        }
    }
    if messages.last().map(|m| m.role.as_str()) != Some("user") {
        return Err("the last message must be from the user".into());
    }
    Ok(())
}

/// Truncate by chars on a safe boundary (no panic on multibyte).
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}\n\n[... truncated ...]")
}

pub fn build_system_single(ctx: &ArticleContext) -> String {
    let body = truncate_chars(&ctx.body, MAX_CONTEXT_CHARS);
    format!(
        "You are a helpful reading assistant. Answer the user's questions about \
the following article. Base your answers on the article content; if the article \
does not contain the answer, say so. Reply in the same language as the user's question.\n\n\
=== ARTICLE ===\nTitle: {}\n\n{}\n=== END ARTICLE ===",
        ctx.title, body
    )
}

pub fn build_system_multi(ctxs: &[ArticleContext]) -> String {
    let per = if ctxs.is_empty() {
        MAX_CONTEXT_CHARS_MULTI
    } else {
        MAX_CONTEXT_CHARS_MULTI / ctxs.len()
    };
    let mut buf = String::from(
        "You are a helpful reading assistant. Answer the user's questions about \
the following articles. You may compare and contrast them. Base your answers on \
the article contents. Reply in the same language as the user's question.\n",
    );
    for (i, c) in ctxs.iter().enumerate() {
        let body = truncate_chars(&c.body, per);
        buf.push_str(&format!(
            "\n=== ARTICLE {} ===\nTitle: {}\n\n{}\n=== END ARTICLE {} ===\n",
            i + 1,
            c.title,
            body,
            i + 1
        ));
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(role: &str, content: &str) -> AskMessage {
        AskMessage {
            role: role.into(),
            content: content.into(),
        }
    }

    #[test]
    fn validate_rejects_empty() {
        assert!(validate_conversation(&[]).is_err());
    }

    #[test]
    fn validate_rejects_unknown_role() {
        assert!(validate_conversation(&[m("system", "x")]).is_err());
    }

    #[test]
    fn validate_rejects_empty_content() {
        assert!(validate_conversation(&[m("user", "  ")]).is_err());
    }

    #[test]
    fn validate_rejects_not_starting_with_user() {
        assert!(validate_conversation(&[m("assistant", "x")]).is_err());
    }

    #[test]
    fn validate_rejects_non_alternating() {
        assert!(validate_conversation(&[m("user", "a"), m("user", "b")]).is_err());
    }

    #[test]
    fn validate_rejects_ending_with_assistant() {
        assert!(validate_conversation(&[m("user", "a"), m("assistant", "b")]).is_err());
    }

    #[test]
    fn validate_accepts_single_user() {
        assert!(validate_conversation(&[m("user", "hi")]).is_ok());
    }

    #[test]
    fn validate_accepts_multiturn() {
        assert!(
            validate_conversation(&[m("user", "a"), m("assistant", "b"), m("user", "c")]).is_ok()
        );
    }

    // 監査 LOW: 入力サイズ無制限の防御。
    #[test]
    fn validate_rejects_oversized_message() {
        let big = "x".repeat(MAX_MESSAGE_CHARS + 1);
        assert!(validate_conversation(&[m("user", &big)]).is_err());
        // 境界ちょうどは通る。
        let ok = "x".repeat(MAX_MESSAGE_CHARS);
        assert!(validate_conversation(&[m("user", &ok)]).is_ok());
    }

    #[test]
    fn validate_rejects_too_many_messages() {
        // user/assistant 交互で MAX_MESSAGES+1 通（user 終わり）を作る。
        let mut msgs: Vec<AskMessage> = (0..=MAX_MESSAGES)
            .map(|i| m(if i % 2 == 0 { "user" } else { "assistant" }, "x"))
            .collect();
        assert_eq!(msgs.len(), MAX_MESSAGES + 1);
        assert!(validate_conversation(&msgs).is_err());
        // 上限ちょうど（user 終わりに調整）は通る。
        msgs.truncate(MAX_MESSAGES - 1);
        assert!(validate_conversation(&msgs).is_ok());
    }

    #[test]
    fn truncate_keeps_short() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn truncate_cuts_long_on_char_boundary() {
        let s = "あ".repeat(100);
        let out = truncate_chars(&s, 10);
        assert!(out.contains("[... truncated ...]"));
        assert!(out.starts_with(&"あ".repeat(10)));
    }

    #[test]
    fn build_system_single_embeds_title_and_body() {
        let s = build_system_single(&ArticleContext {
            title: "T".into(),
            body: "BODY".into(),
        });
        assert!(s.contains("Title: T"));
        assert!(s.contains("BODY"));
    }

    #[test]
    fn build_system_multi_numbers_articles() {
        let s = build_system_multi(&[
            ArticleContext {
                title: "A".into(),
                body: "x".into(),
            },
            ArticleContext {
                title: "B".into(),
                body: "y".into(),
            },
        ]);
        assert!(s.contains("ARTICLE 1"));
        assert!(s.contains("ARTICLE 2"));
    }

    #[test]
    fn build_system_multi_handles_empty() {
        let _ = build_system_multi(&[]); // must not panic (no divide-by-zero)
    }
}
