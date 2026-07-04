//! notifications スライスの use case。VAPID 署名・Web Push 送信・失効 GC・
//! 新着通知のウォーターマーク進行。抽象境界(trait)は増やさない（web-push を直接使う）。

use axum::http::Uri;
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::Utc;
use web_push_native::{
    jwt_simple::algorithms::ES256KeyPair, p256::PublicKey, Auth, WebPushBuilder,
};

use super::domain::{NotificationPayload, PushSubscriptionInput};
use super::repository::{self, ArticleNotice, StoredSubscription};
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;
use chrono::DateTime;

/// 1 取得サイクルで送る新着通知の上限。バーストで通知が溢れるのを防ぐ。
const MAX_PER_CYCLE: i64 = 20;

/// 同時に飛ばす push 送信の並行数。直列だと死んだエンドポイントの分だけ
/// 送信が積み上がる（監査 #4）。
const SEND_CONCURRENCY: usize = 8;

/// VAPID の subject クレーム（RFC8292）。push サービスが要求する連絡先。mailto: か https:。
const VAPID_SUBJECT: &str = "mailto:rss-reader@example.com";

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

/// 通知ディスパッチタスクの多重起動防止フラグ。RAII で drop 時に必ず解放する
/// （spawn したタスクが panic しても Drop は走る）。
static DISPATCH_IN_FLIGHT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

struct DispatchGuard;

impl DispatchGuard {
    /// フラグが空いていれば獲得。既に走っていれば None。
    fn try_acquire() -> Option<Self> {
        use std::sync::atomic::Ordering;
        if DISPATCH_IN_FLIGHT.swap(true, Ordering::SeqCst) {
            None
        } else {
            Some(Self)
        }
    }
}

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        DISPATCH_IN_FLIGHT.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// スケジューラのフック（#31）。通知ディスパッチを独立タスクとして起動する。
/// クロールループ内で直列送信すると、死んだ push エンドポイントの分だけ次の
/// 取得サイクルが遅れる（監査 #4）ため、送信はループの外で行う。前回の
/// ディスパッチがまだ走っていればスキップ（ウォーターマークが拾い直すので
/// 取りこぼしはない）。
pub fn spawn_notify_new_articles(state: AppState) {
    let Some(guard) = DispatchGuard::try_acquire() else {
        tracing::warn!("push dispatch still running; skipping this cycle");
        return;
    };
    tokio::spawn(async move {
        let _guard = guard; // タスク終了（panic 含む）で解放
        if let Err(e) = notify_new_articles(&state).await {
            tracing::error!(error = %e, "push notification dispatch failed");
        }
    });
}

/// 高優先フィードの新着を全購読へ通知する。VAPID 未設定なら no-op。
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
    let (count, watermark) = plan_cycle(&notices, MAX_PER_CYCLE as usize, until);

    for notice in notices.iter().take(count) {
        let payload = NotificationPayload::for_article(
            &notice.title,
            notice.feed_title.as_deref(),
            &notice.url,
        );
        if let Err(e) = broadcast(state, privk, &payload.to_json()).await {
            tracing::warn!(error = %e, "push broadcast failed");
        }
    }

    if notices.len() > count {
        tracing::warn!(
            cap = MAX_PER_CYCLE,
            "push: capped new-article notifications this cycle; the rest will go out next cycle"
        );
    }

    // 送れても送れなくてもウォーターマークは進める（同じ記事の再通知を防ぐ）。
    repository::set_watermark(&state.db, watermark).await?;
    Ok(())
}

/// 1 サイクルで通知する件数と、進めてよいウォーターマークを決める（純粋）。
///
/// 上限超過時にウォーターマークを `until` まで進めると、上限から溢れた記事が
/// 次サイクルの `created_at > watermark` に引っかからず**永久にスキップ**される
/// （監査 #3）。実際に通知した最後の記事の `created_at` までに留め、残りは次
/// サイクルで拾う。`created_at` は行ごとの INSERT で刻まれるため同値衝突は稀
/// （同値だった場合は落とす方向に倒れ、重複通知はしない）。
fn plan_cycle(
    notices: &[ArticleNotice],
    cap: usize,
    until: DateTime<Utc>,
) -> (usize, DateTime<Utc>) {
    if notices.len() > cap {
        (cap, notices[cap - 1].created_at)
    } else {
        (notices.len(), until)
    }
}

/// 1 購読への送信結果。GC 対象(失効)と一時失敗を区別する。
enum SendOutcome {
    Delivered,
    Expired,
    Failed(String),
}

/// 全購読へ payload を送る（並行数 SEND_CONCURRENCY、監査 #4）。
/// 失効購読(404/410)は DB から GC。配送成功数を返す。
async fn broadcast(
    state: &AppState,
    private_key_b64: &str,
    payload_json: &str,
) -> AppResult<usize> {
    let subs = repository::list_subscriptions(&state.db).await?;
    if subs.is_empty() {
        return Ok(0);
    }
    // VAPID 鍵ペアはサイクル内で 1 回だけ復号し使い回す。
    let key_pair = match decode_vapid_key(private_key_b64) {
        Ok(k) => std::sync::Arc::new(k),
        Err(e) => {
            tracing::error!(error = %e, "invalid VAPID private key; skipping push dispatch");
            return Ok(0);
        }
    };
    let payload: std::sync::Arc<[u8]> = payload_json.as_bytes().into();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(SEND_CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    for sub in subs {
        let http = state.http.clone();
        let db = state.db.clone();
        let key_pair = key_pair.clone();
        let payload = payload.clone();
        let semaphore = semaphore.clone();
        tasks.spawn(async move {
            // Semaphore は close しないので acquire は失敗しない。
            let _permit = semaphore.acquire().await;
            match send_one(&http, &key_pair, &sub, &payload).await {
                SendOutcome::Delivered => 1usize,
                SendOutcome::Expired => {
                    // 失効: 行を GC。失敗はログのみ。
                    if let Err(e) = repository::delete_subscription_by_id(&db, sub.id).await {
                        tracing::warn!(error = %e, "failed to GC expired push subscription");
                    }
                    0
                }
                SendOutcome::Failed(err) => {
                    tracing::warn!(error = %err, endpoint = %sub.endpoint, "web push send failed");
                    0
                }
            }
        });
    }
    let mut delivered = 0usize;
    while let Some(res) = tasks.join_next().await {
        delivered += res.unwrap_or(0); // JoinError(panic) は 0 扱いでログ済み想定
    }
    Ok(delivered)
}

/// base64url(no pad) の P-256 秘密鍵スカラーから ES256 鍵ペアを復元する。
fn decode_vapid_key(private_key_b64: &str) -> Result<ES256KeyPair, String> {
    let raw = Base64UrlUnpadded::decode_vec(private_key_b64)
        .map_err(|e| format!("VAPID private key is not valid base64url: {e}"))?;
    ES256KeyPair::from_bytes(&raw).map_err(|e| format!("VAPID private key is invalid: {e}"))
}

/// 1 購読へ 1 通。web-push-native で RFC8291 暗号化＋VAPID 署名した http::Request を組み、
/// 送信は既存 reqwest(rustls) で行う（openssl 非依存）。
async fn send_one(
    http: &reqwest::Client,
    key_pair: &ES256KeyPair,
    sub: &StoredSubscription,
    payload: &[u8],
) -> SendOutcome {
    let request = match build_request(key_pair, sub, payload) {
        Ok(r) => r,
        Err(e) => return SendOutcome::Failed(e),
    };
    let req = match reqwest::Request::try_from(request) {
        Ok(r) => r,
        Err(e) => return SendOutcome::Failed(e.to_string()),
    };
    let resp = match http.execute(req).await {
        Ok(r) => r,
        Err(e) => return SendOutcome::Failed(e.to_string()),
    };
    // 404 Not Found / 410 Gone = 失効購読 → GC。2xx = 配送。他は一時失敗。
    match resp.status().as_u16() {
        200..=299 => SendOutcome::Delivered,
        404 | 410 => SendOutcome::Expired,
        other => SendOutcome::Failed(format!("push service returned status {other}")),
    }
}

/// 購読情報から、暗号化＋VAPID 署名済みの HTTP リクエストを組み立てる（純粋・I/O なし）。
fn build_request(
    key_pair: &ES256KeyPair,
    sub: &StoredSubscription,
    payload: &[u8],
) -> Result<axum::http::Request<Vec<u8>>, String> {
    let endpoint: Uri = sub
        .endpoint
        .parse()
        .map_err(|e| format!("invalid push endpoint: {e}"))?;
    let p256dh =
        Base64UrlUnpadded::decode_vec(&sub.p256dh).map_err(|e| format!("invalid p256dh: {e}"))?;
    let auth_bytes =
        Base64UrlUnpadded::decode_vec(&sub.auth).map_err(|e| format!("invalid auth: {e}"))?;
    if auth_bytes.len() != 16 {
        return Err(format!(
            "auth secret must be 16 bytes, got {}",
            auth_bytes.len()
        ));
    }
    let ua_public =
        PublicKey::from_sec1_bytes(&p256dh).map_err(|e| format!("invalid p256dh point: {e}"))?;
    let ua_auth = Auth::clone_from_slice(&auth_bytes);

    WebPushBuilder::new(endpoint, ua_public, ua_auth)
        .with_vapid(key_pair, VAPID_SUBJECT)
        .build(payload.to_vec())
        .map_err(|e| format!("failed to build web push request: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // web-push-native の既知サンプル鍵/購読。scripts/gen-vapid.sh の秘密鍵形式
    // (base64url の 32B スカラー)と同一。I/O なし。
    const VAPID_PRIVATE: &str = "IQ9Ur0ykXoHS9gzfYX0aBjy9lvdrjx_PFUXmie9YRcY";
    const ENDPOINT: &str = "https://updates.push.services.mozilla.com/wpush/v1/gAAAAAB";
    const P256DH: &str =
        "BLMbF9ffKBiWQLCKvTHb6LO8Nb6dcUh6TItC455vu2kElga6PQvUmaFyCdykxY2nOSSL3yKgfbmFLRTUaGv4yV8";
    const AUTH: &str = "xS03Fi5ErfTNH_l9WHE9Ig";

    fn sample_sub() -> StoredSubscription {
        StoredSubscription {
            id: uuid::Uuid::nil(),
            endpoint: ENDPOINT.to_string(),
            p256dh: P256DH.to_string(),
            auth: AUTH.to_string(),
        }
    }

    // base64url の P-256 秘密鍵から ES256 鍵ペアを復元できる。
    #[test]
    fn decode_vapid_key_accepts_base64url_scalar() {
        assert!(decode_vapid_key(VAPID_PRIVATE).is_ok());
    }

    #[test]
    fn decode_vapid_key_rejects_garbage() {
        assert!(decode_vapid_key("!!!not-base64!!!").is_err());
    }

    // 暗号化＋VAPID 署名まで通り、http::Request が組み立つ（送信は行わない）。
    #[test]
    fn build_request_produces_signed_encrypted_request() {
        let key_pair = decode_vapid_key(VAPID_PRIVATE).unwrap();
        let sub = sample_sub();
        let req = build_request(&key_pair, &sub, br#"{"title":"t","body":"b","url":"/"}"#);
        assert!(
            req.is_ok(),
            "should build a valid web push request: {req:?}"
        );
        let req = req.unwrap();
        assert_eq!(req.method(), axum::http::Method::POST);
        // Content-Encoding: aes128gcm と Authorization(vapid) が付く。
        assert!(req
            .headers()
            .contains_key(axum::http::header::AUTHORIZATION));
    }

    // 壊れた購読鍵は panic せず Err を返す（auth 長不正 / p256dh 不正）。
    #[test]
    fn build_request_rejects_bad_subscription_keys() {
        let key_pair = decode_vapid_key(VAPID_PRIVATE).unwrap();
        let mut sub = sample_sub();
        sub.auth = "AAAA".to_string(); // 3 バイト → 16 でない
        assert!(build_request(&key_pair, &sub, b"x").is_err());
    }

    // ---- plan_cycle: ウォーターマーク進行の決定（監査 #3: 取りこぼし修正） ----

    use super::super::repository::ArticleNotice;
    use chrono::TimeZone;

    fn notice_at(secs: i64) -> ArticleNotice {
        ArticleNotice {
            title: "t".into(),
            url: "https://example.com/a".into(),
            feed_title: None,
            created_at: Utc.timestamp_opt(secs, 0).unwrap(),
        }
    }

    #[test]
    fn plan_cycle_advances_to_until_when_under_cap() {
        let until = Utc.timestamp_opt(1000, 0).unwrap();
        let notices: Vec<_> = (1..=3).map(notice_at).collect();
        let (count, watermark) = plan_cycle(&notices, 20, until);
        assert_eq!(count, 3);
        assert_eq!(watermark, until);
    }

    #[test]
    fn plan_cycle_advances_to_until_when_exactly_at_cap() {
        let until = Utc.timestamp_opt(1000, 0).unwrap();
        let notices: Vec<_> = (1..=20).map(notice_at).collect();
        let (count, watermark) = plan_cycle(&notices, 20, until);
        assert_eq!(count, 20);
        assert_eq!(watermark, until);
    }

    // 超過時: until まで進めると 21 件目以降が永久スキップされる。実際に通知した
    // 最後の記事の created_at までに留め、残りは次サイクルで拾う。
    #[test]
    fn plan_cycle_holds_watermark_at_last_notified_when_over_cap() {
        let until = Utc.timestamp_opt(1000, 0).unwrap();
        let notices: Vec<_> = (1..=21).map(notice_at).collect();
        let (count, watermark) = plan_cycle(&notices, 20, until);
        assert_eq!(count, 20);
        assert_eq!(watermark, Utc.timestamp_opt(20, 0).unwrap());
    }

    #[test]
    fn plan_cycle_handles_empty_cycle() {
        let until = Utc.timestamp_opt(1000, 0).unwrap();
        let (count, watermark) = plan_cycle(&[], 20, until);
        assert_eq!(count, 0);
        assert_eq!(watermark, until);
    }

    // ---- DispatchGuard: 通知タスクの多重起動防止（監査 #4） ----

    // 保持中は次の獲得が失敗し、drop（panic 時含む）で必ず解放される。
    #[test]
    fn dispatch_guard_excludes_second_acquire_until_dropped() {
        let first = DispatchGuard::try_acquire();
        assert!(first.is_some(), "free flag should be acquirable");
        assert!(
            DispatchGuard::try_acquire().is_none(),
            "second acquire while held must fail"
        );
        drop(first);
        assert!(
            DispatchGuard::try_acquire().is_some(),
            "flag must be released on drop"
        );
    }
}
