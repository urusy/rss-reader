//! notifications スライスの use case。VAPID 署名・Web Push 送信・失効 GC・
//! 新着通知のウォーターマーク進行。抽象境界(trait)は増やさない（web-push を直接使う）。

use chrono::Utc;
use web_push::{
    ContentEncoding, HyperWebPushClient, SubscriptionInfo, VapidSignatureBuilder, WebPushClient,
    WebPushError, WebPushMessageBuilder,
};

use super::domain::{NotificationPayload, PushSubscriptionInput};
use super::repository::{self, StoredSubscription};
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// 1 取得サイクルで送る新着通知の上限。バーストで通知が溢れるのを防ぐ。
const MAX_PER_CYCLE: i64 = 20;

/// VAPID 鍵ペアを取り出す。未設定なら 503 NotEnabled（要約/翻訳と同型）。
fn vapid_keys(state: &AppState) -> AppResult<(&str, &str)> {
    match (
        state.config.vapid_public_key.as_deref(),
        state.config.vapid_private_key.as_deref(),
    ) {
        (Some(pubk), Some(privk)) => Ok((pubk, privk)),
        _ => Err(AppError::NotEnabled(
            "Web Push is not configured (VAPID keys unset)".into(),
        )),
    }
}

/// SW の applicationServerKey に渡す VAPID 公開鍵。
pub fn public_key(state: &AppState) -> AppResult<String> {
    let (pubk, _) = vapid_keys(state)?;
    Ok(pubk.to_string())
}

pub async fn subscribe(
    state: &AppState,
    input: PushSubscriptionInput,
    user_agent: Option<&str>,
) -> AppResult<()> {
    // 鍵未設定なら購読を受けても送れないので設定必須にする。
    vapid_keys(state)?;
    input.validate().map_err(AppError::Validation)?;
    repository::upsert_subscription(&state.db, &input, user_agent).await
}

pub async fn unsubscribe(state: &AppState, endpoint: &str) -> AppResult<()> {
    repository::delete_subscription(&state.db, endpoint).await?;
    Ok(())
}

/// テスト通知（疎通確認）。全購読へ 1 通送り、配送成功数を返す。
pub async fn test_notification(state: &AppState) -> AppResult<usize> {
    let (_, privk) = vapid_keys(state)?;
    let payload = NotificationPayload {
        title: "テスト通知".to_string(),
        body: "Web Push は正しく設定されています".to_string(),
        url: "/".to_string(),
    };
    broadcast(state, privk, &payload.to_json()).await
}

/// スケジューラのフック（#31）。高優先フィードの新着を全購読へ通知する。
/// VAPID 未設定なら no-op（クロールループを止めない）。
pub async fn notify_new_articles(state: &AppState) -> AppResult<()> {
    let privk = match vapid_keys(state) {
        Ok((_, privk)) => privk,
        Err(_) => return Ok(()), // 機能無効: 何もしない
    };

    let since = repository::get_watermark(&state.db).await?;
    let until = Utc::now();
    // 上限+1 件取り、超過を検知して silent cap を避ける。
    let notices =
        repository::new_priority_articles(&state.db, since, until, MAX_PER_CYCLE + 1).await?;
    let truncated = notices.len() as i64 > MAX_PER_CYCLE;

    for notice in notices.iter().take(MAX_PER_CYCLE as usize) {
        let payload =
            NotificationPayload::for_article(&notice.title, notice.feed_title.as_deref(), &notice.url);
        if let Err(e) = broadcast(state, privk, &payload.to_json()).await {
            tracing::warn!(error = %e, "push broadcast failed");
        }
    }

    if truncated {
        tracing::warn!(
            cap = MAX_PER_CYCLE,
            "push: capped new-article notifications this cycle; extra articles skipped"
        );
    }

    // 送れても送れなくてもウォーターマークは進める（同じ記事の再通知を防ぐ）。
    repository::set_watermark(&state.db, until).await?;
    Ok(())
}

/// 全購読へ payload を 1 通ずつ送る。失効購読(404/410)は DB から GC。配送成功数を返す。
async fn broadcast(state: &AppState, private_key_b64: &str, payload_json: &str) -> AppResult<usize> {
    let subs = repository::list_subscriptions(&state.db).await?;
    if subs.is_empty() {
        return Ok(0);
    }
    let client = HyperWebPushClient::new();
    let bytes = payload_json.as_bytes();
    let mut delivered = 0usize;
    for sub in subs {
        match send_one(&client, private_key_b64, &sub, bytes).await {
            Ok(()) => delivered += 1,
            Err(WebPushError::EndpointNotValid(_)) | Err(WebPushError::EndpointNotFound(_)) => {
                // 失効: 行を GC。失敗はログのみ。
                if let Err(e) = repository::delete_subscription_by_id(&state.db, sub.id).await {
                    tracing::warn!(error = %e, "failed to GC expired push subscription");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, endpoint = %sub.endpoint, "web push send failed");
            }
        }
    }
    Ok(delivered)
}

/// 1 購読へ 1 通。VAPID 署名 → aes128gcm 暗号化 → 送信。
async fn send_one(
    client: &HyperWebPushClient,
    private_key_b64: &str,
    sub: &StoredSubscription,
    payload: &[u8],
) -> Result<(), WebPushError> {
    let info = SubscriptionInfo::new(
        sub.endpoint.clone(),
        sub.p256dh.clone(),
        sub.auth.clone(),
    );
    let signature = VapidSignatureBuilder::from_base64(private_key_b64, &info)?.build()?;
    let mut builder = WebPushMessageBuilder::new(&info);
    builder.set_payload(ContentEncoding::Aes128Gcm, payload);
    builder.set_vapid_signature(signature);
    client.send(builder.build()?).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // web-push crate が base64url(no pad) の P-256 秘密鍵を受理し、VAPID 署名まで
    // 組み立てられることを固定鍵で担保する（scripts/gen-vapid.sh が吐く形式と同一）。
    // 鍵/購読は web-push 公式 README の既知サンプル。I/O は発生しない。
    #[test]
    fn vapid_signature_builds_from_base64_key() {
        let info = SubscriptionInfo::new(
            "https://updates.push.services.mozilla.com/wpush/v1/gAAAAAB",
            "BLMbF9ffKBiWQLCKvTHb6LO8Nb6dcUh6TItC455vu2kElga6PQvUmaFyCdykxY2nOSSL3yKgfbmFLRTUaGv4yV8",
            "xS03Fi5ErfTNH_l9WHE9Ig",
        );
        let built = VapidSignatureBuilder::from_base64(
            "IQ9Ur0ykXoHS9gzfYX0aBjy9lvdrjx_PFUXmie9YRcY",
            &info,
        )
        .and_then(|b| b.build());
        assert!(built.is_ok(), "from_base64 should accept a base64url P-256 key");
    }

    // aes128gcm ペイロード付きメッセージが暗号化・ビルドまで通ること（送信は行わない）。
    #[test]
    fn message_with_payload_builds() {
        let info = SubscriptionInfo::new(
            "https://updates.push.services.mozilla.com/wpush/v1/gAAAAAB",
            "BLMbF9ffKBiWQLCKvTHb6LO8Nb6dcUh6TItC455vu2kElga6PQvUmaFyCdykxY2nOSSL3yKgfbmFLRTUaGv4yV8",
            "xS03Fi5ErfTNH_l9WHE9Ig",
        );
        let sig = VapidSignatureBuilder::from_base64(
            "IQ9Ur0ykXoHS9gzfYX0aBjy9lvdrjx_PFUXmie9YRcY",
            &info,
        )
        .unwrap()
        .build()
        .unwrap();
        let mut builder = WebPushMessageBuilder::new(&info);
        builder.set_payload(ContentEncoding::Aes128Gcm, br#"{"title":"t","body":"b","url":"/"}"#);
        builder.set_vapid_signature(sig);
        assert!(builder.build().is_ok());
    }
}
