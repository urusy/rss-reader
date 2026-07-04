//! notifications スライスのドメイン。ワイヤに出す値の組み立て・検証を
//! 外部 I/O を持たない純粋関数に閉じ、TDD 対象にする（#31）。

use serde::{Deserialize, Serialize};

/// SW が `showNotification` に流す通知ペイロード。組み立ては純粋。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NotificationPayload {
    pub title: String,
    pub body: String,
    pub url: String,
}

impl NotificationPayload {
    /// 記事タイトル・フィード名・URL から通知を組み立てる。
    /// 空タイトルは `(untitled)`、フィード名欠落は「新着記事」にフォールバック。
    pub fn for_article(article_title: &str, feed_title: Option<&str>, url: &str) -> Self {
        let title = {
            let t = article_title.trim();
            if t.is_empty() {
                "(untitled)".to_string()
            } else {
                t.to_string()
            }
        };
        let body = feed_title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("新着記事")
            .to_string();
        Self {
            title,
            body,
            url: url.to_string(),
        }
    }

    /// JSON 文字列化（SW 側で JSON.parse する）。直列化不能でも空オブジェクトで握りつぶす。
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// ブラウザの `PushSubscription.toJSON()` に対応する入力。
#[derive(Debug, Clone, Deserialize)]
pub struct PushSubscriptionInput {
    pub endpoint: String,
    pub keys: PushKeys,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushKeys {
    pub p256dh: String,
    pub auth: String,
}

impl PushSubscriptionInput {
    /// 保存前検証。endpoint は https、鍵は非空であること。
    pub fn validate(&self) -> Result<(), String> {
        if !self.endpoint.starts_with("https://") {
            return Err("push endpoint must be an https URL".to_string());
        }
        if self.keys.p256dh.trim().is_empty() || self.keys.auth.trim().is_empty() {
            return Err("push subscription keys must not be empty".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_uses_article_title_and_feed_name() {
        let p =
            NotificationPayload::for_article("新機能リリース", Some("Rust Blog"), "https://x/a");
        assert_eq!(p.title, "新機能リリース");
        assert_eq!(p.body, "Rust Blog");
        assert_eq!(p.url, "https://x/a");
    }

    #[test]
    fn payload_falls_back_on_empty_title() {
        let p = NotificationPayload::for_article("   ", Some("Feed"), "https://x/a");
        assert_eq!(p.title, "(untitled)");
    }

    #[test]
    fn payload_falls_back_on_missing_feed_title() {
        let none = NotificationPayload::for_article("t", None, "https://x/a");
        assert_eq!(none.body, "新着記事");
        let blank = NotificationPayload::for_article("t", Some("  "), "https://x/a");
        assert_eq!(blank.body, "新着記事");
    }

    #[test]
    fn payload_to_json_roundtrips_fields() {
        let p = NotificationPayload::for_article("Hi", Some("F"), "https://x/a");
        let v: serde_json::Value = serde_json::from_str(&p.to_json()).unwrap();
        assert_eq!(v["title"], "Hi");
        assert_eq!(v["body"], "F");
        assert_eq!(v["url"], "https://x/a");
    }

    fn sub(endpoint: &str, p256dh: &str, auth: &str) -> PushSubscriptionInput {
        PushSubscriptionInput {
            endpoint: endpoint.to_string(),
            keys: PushKeys {
                p256dh: p256dh.to_string(),
                auth: auth.to_string(),
            },
        }
    }

    #[test]
    fn validate_accepts_https_with_keys() {
        assert!(sub("https://push.example/abc", "p", "a").validate().is_ok());
    }

    #[test]
    fn validate_rejects_non_https_endpoint() {
        assert!(sub("http://push.example/abc", "p", "a").validate().is_err());
    }

    #[test]
    fn validate_rejects_empty_keys() {
        assert!(sub("https://push.example/abc", "  ", "a")
            .validate()
            .is_err());
        assert!(sub("https://push.example/abc", "p", "").validate().is_err());
    }
}
