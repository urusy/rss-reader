use chrono::NaiveDate;

use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// ダイジェストをメール送信する（Resend の HTTP API）。
/// RESEND_API_KEY / DIGEST_EMAIL_FROM / DIGEST_EMAIL_TO が揃っているときだけ送る。
/// SMTP は採用しない（自前 SMTP 到達性の問題と、serverless 移行検討でも
/// HTTP API 送信が結論。docs/research/2026-07-06-cloudflare-workers-study.md）。
/// 失敗はダイジェスト生成に対して致命ではない（呼び出し元がログして続行）。
pub async fn maybe_send(state: &AppState, date: NaiveDate, markdown: &str) -> AppResult<()> {
    let cfg = &state.config;
    let (Some(api_key), Some(from), Some(to)) = (
        cfg.resend_api_key.as_ref(),
        cfg.digest_email_from.as_ref(),
        cfg.digest_email_to.as_ref(),
    ) else {
        return Ok(()); // not configured → skip silently
    };

    let url = format!("{}/emails", cfg.resend_base_url.trim_end_matches('/'));
    let resp = state
        .http
        .post(&url)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "from": from,
            "to": [to],
            "subject": format!("RSS ダイジェスト {date}"),
            // v1 は Markdown をプレーンテキストのまま送る（可読で十分・依存を増やさない）。
            "text": markdown,
        }))
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("resend request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect();
        return Err(AppError::Upstream(format!(
            "resend returned {status}: {body}"
        )));
    }
    tracing::info!(%date, to = %to, "digest email sent via Resend");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::auth::LoginLimiter;
    use crate::shared::config::AppConfig;
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// 受信したリクエストの記録（Authorization ヘッダと JSON body）。
    #[derive(Default)]
    struct Captured {
        hits: AtomicUsize,
        auth: Mutex<Option<String>>,
        body: Mutex<Option<serde_json::Value>>,
    }

    /// ローカルのモック Resend サーバー。/emails を受けて status を返す。
    async fn mock_resend(status: u16, cap: Arc<Captured>) -> String {
        let app = Router::new()
            .route(
                "/emails",
                post(
                    move |State(cap): State<Arc<Captured>>,
                          headers: HeaderMap,
                          Json(body): Json<serde_json::Value>| async move {
                        cap.hits.fetch_add(1, Ordering::SeqCst);
                        *cap.auth.lock().unwrap() = headers
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            .map(String::from);
                        *cap.body.lock().unwrap() = Some(body);
                        (
                            axum::http::StatusCode::from_u16(status).unwrap(),
                            Json(serde_json::json!({ "id": "test" })),
                        )
                    },
                ),
            )
            .with_state(cap);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn state_with(base_url: &str, configured: bool) -> AppState {
        let mut cfg = AppConfig::for_test();
        cfg.resend_base_url = base_url.to_string();
        if configured {
            cfg.resend_api_key = Some("re_test_key".to_string());
            cfg.digest_email_from = Some("digest@reader.example".to_string());
            cfg.digest_email_to = Some("me@example.com".to_string());
        }
        AppState {
            db: sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://invalid/invalid")
                .unwrap(),
            config: Arc::new(cfg),
            http: reqwest::Client::new(),
            http_external: reqwest::Client::new(),
            login_limiter: Arc::new(Mutex::new(LoginLimiter::default())),
        }
    }

    fn a_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 7).unwrap()
    }

    #[tokio::test]
    async fn skips_silently_when_not_configured() {
        let cap = Arc::new(Captured::default());
        let base = mock_resend(200, cap.clone()).await;
        let state = state_with(&base, false);
        maybe_send(&state, a_date(), "# digest").await.unwrap();
        assert_eq!(cap.hits.load(Ordering::SeqCst), 0); // 一切送らない
    }

    #[tokio::test]
    async fn sends_bearer_auth_and_payload_when_configured() {
        let cap = Arc::new(Captured::default());
        let base = mock_resend(200, cap.clone()).await;
        let state = state_with(&base, true);
        maybe_send(&state, a_date(), "# 今日のダイジェスト")
            .await
            .unwrap();

        assert_eq!(cap.hits.load(Ordering::SeqCst), 1);
        assert_eq!(
            cap.auth.lock().unwrap().as_deref(),
            Some("Bearer re_test_key")
        );
        let body = cap.body.lock().unwrap().clone().unwrap();
        assert_eq!(body["from"], "digest@reader.example");
        assert_eq!(body["to"], serde_json::json!(["me@example.com"]));
        assert_eq!(body["subject"], "RSS ダイジェスト 2026-07-07");
        assert_eq!(body["text"], "# 今日のダイジェスト");
    }

    #[tokio::test]
    async fn non_2xx_is_an_upstream_error() {
        let cap = Arc::new(Captured::default());
        let base = mock_resend(422, cap.clone()).await;
        let state = state_with(&base, true);
        let err = maybe_send(&state, a_date(), "x").await.unwrap_err();
        assert!(matches!(err, AppError::Upstream(_)));
    }
}
