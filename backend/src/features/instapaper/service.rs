use super::domain::{
    classify_add_status, classify_auth_status, read_later_status, AddOutcome, AuthOutcome,
    InstapaperCredentials, InstapaperStatus, ReadLaterItem, SaveUrl, StoredCredentials,
};
use super::repository;
use crate::features::articles::domain::ArticleId;
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

/// 記事を Instapaper に送り、保存状態を read_later_items に永続化して返す（機能06）。
/// 順序: (1) 既に added 済みなら即返す（冪等・資格情報も記事も見ない）、
///       (2) 記事あり? なければ NotFound、(3) 資格情報あり? なければ NotEnabled、
///       (4) pending 確保 → 転送 → 結果で added/failed 確定。
pub async fn save_for_later(state: &AppState, id: ArticleId) -> AppResult<ReadLaterItem> {
    // 1) 冪等: 既に added 済みなら、資格情報が後で消えても保存済みを読めるよう即返す。
    if let Some(item) = repository::get_item(&state.db, id).await? {
        if item.status.as_str() == read_later_status::ADDED {
            return Ok(item);
        }
    }

    // 2) 記事の URL/タイトル取得（無ければ NotFound）
    let article = repository::get_article_ref(&state.db, id.0)
        .await?
        .ok_or(AppError::NotFound)?;

    // 3) 資格情報確認（未設定なら NotEnabled）
    let creds = repository::get_credentials(&state.db)
        .await?
        .ok_or_else(|| AppError::NotEnabled("Instapaper credentials are not set".into()))?;

    // 4) pending 確保 → 転送 → 結果で状態確定。失敗も DB に残して可視化・再試行可能に。
    let url = SaveUrl::parse(article.url).map_err(AppError::Validation)?;
    repository::upsert_pending(&state.db, id).await?;
    match send_to_instapaper(state, &creds, &url, Some(article.title)).await {
        Ok(()) => repository::mark_added(&state.db, id).await,
        Err(e) => {
            let _ = repository::mark_failed(&state.db, id, &e.to_string()).await;
            Err(AppError::Upstream(format!("instapaper add failed: {e}")))
        }
    }
}

pub async fn get_read_later(state: &AppState, id: ArticleId) -> AppResult<Option<ReadLaterItem>> {
    repository::get_item(&state.db, id).await
}

pub async fn list_read_later(state: &AppState) -> AppResult<Vec<ReadLaterItem>> {
    repository::list_items(&state.db).await
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
