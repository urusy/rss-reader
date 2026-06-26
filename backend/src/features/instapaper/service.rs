use uuid::Uuid;

use super::domain::{
    classify_add_status, classify_auth_status, AddOutcome, AuthOutcome, InstapaperCredentials,
    InstapaperStatus, SaveUrl, StoredCredentials,
};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

// 実エンドポイントは instapaper.com/api で要確認。www. 有無に注意。
const ADD_URL: &str = "https://www.instapaper.com/api/add";
const AUTH_URL: &str = "https://www.instapaper.com/api/authenticate";

/// 保存前に Instapaper で検証してから永続化（誤入力を即座に弾く）。
pub async fn save_credentials(state: &AppState, creds: InstapaperCredentials) -> AppResult<()> {
    verify(state, creds.username(), creds.password()).await?;
    repository::upsert_credentials(&state.db, creds.username(), creds.password()).await
}

pub async fn get_status(state: &AppState) -> AppResult<InstapaperStatus> {
    let configured = repository::get_credentials(&state.db).await?.is_some();
    Ok(InstapaperStatus { configured })
}

pub async fn clear_credentials(state: &AppState) -> AppResult<()> {
    repository::delete_credentials(&state.db).await
}

/// 記事を Instapaper に送る（05 の所有エンドポイント `POST /api/read-later` の本体）。
/// 順序: (1) 資格情報あり? なければ NotEnabled、(2) 記事あり? なければ NotFound、(3) 転送。
/// 06（read-later）は本関数を同一スライス内で拡張し、(2)の後に read_later_items を
/// pending で upsert、(3)の結果で added/failed に更新する（HTTP 契約は不変）。
pub async fn add_to_read_later(state: &AppState, article_id: Uuid) -> AppResult<()> {
    let creds = repository::get_credentials(&state.db)
        .await?
        .ok_or_else(|| AppError::NotEnabled("Instapaper credentials are not set".into()))?;

    let article = repository::get_article_ref(&state.db, article_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // 保存済み記事の URL は本来 http(s) のはずだが、防御的に値オブジェクトへ通す。
    let url = SaveUrl::parse(article.url).map_err(AppError::Validation)?;
    send_to_instapaper(state, &creds, &url, Some(article.title)).await
}

/// Instapaper /api/add への低レベル転送プリミティブ。
async fn send_to_instapaper(
    state: &AppState,
    creds: &StoredCredentials,
    url: &SaveUrl,
    title: Option<String>,
) -> AppResult<()> {
    let mut form: Vec<(&str, String)> = vec![("url", url.as_str().to_string())];
    if let Some(t) = title {
        form.push(("title", t));
    }

    let resp = state
        .http
        .post(ADD_URL)
        .basic_auth(&creds.username, Some(&creds.password))
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?;

    let status = resp.status();
    match classify_add_status(status.as_u16()) {
        AddOutcome::Saved => Ok(()),
        AddOutcome::BadRequest => {
            let text = resp.text().await.unwrap_or_default();
            Err(AppError::Validation(format!(
                "instapaper rejected the request: {text}"
            )))
        }
        AddOutcome::Failed => {
            let text = resp.text().await.unwrap_or_default();
            Err(AppError::Upstream(format!("instapaper {status}: {text}")))
        }
    }
}

/// /api/authenticate で資格情報を検証。403 は誤資格情報 → Validation（保存フォームに表示）。
async fn verify(state: &AppState, username: &str, password: &str) -> AppResult<()> {
    let resp = state
        .http
        .post(AUTH_URL)
        .basic_auth(username, Some(password))
        .send()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?;

    let status = resp.status();
    match classify_auth_status(status.as_u16()) {
        AuthOutcome::Valid => Ok(()),
        AuthOutcome::Invalid => Err(AppError::Validation(
            "invalid Instapaper credentials".into(),
        )),
        AuthOutcome::Failed => {
            let text = resp.text().await.unwrap_or_default();
            Err(AppError::Upstream(format!("instapaper {status}: {text}")))
        }
    }
}
