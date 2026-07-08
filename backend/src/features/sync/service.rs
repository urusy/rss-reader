//! sync スライスのユースケース。ストリーム解決・edit-tag 意味論・cross-slice
//! 呼び出し（feeds/folders/annotations の所有関数）・ClientLogin。
//!
//! 書き込みの副作用が濃い操作（購読の追加/削除/改名/フォルダ移動）は必ず
//! 所有スライスの service を呼ぶ。既読だけは stale ID 耐性・一括更新のため
//! sync 所有の `set_read_by_short_ids` を使う（§5.3 の設計判断）。

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::domain::{
    self, EditOp, ItemId, StreamId, SyncToken, DEFAULT_PAGE_SIZE, MAX_ITEMS_PER_REQUEST,
    MAX_PAGE_SIZE,
};
use super::repository as repo;
use super::wire;
use crate::features::annotations::repository as annotations_repo;
use crate::features::auth::domain::Password;
use crate::features::auth::repository as auth_repo;
use crate::features::auth::service as auth_service;
use crate::features::feeds::domain::FeedId;
use crate::features::feeds::service as feeds_service;
use crate::features::folders::domain::{FolderId, FolderName};
use crate::features::folders::repository as folders_repo;
use crate::shared::auth::hash_token;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// 同一 label（= ClientLogin の Email）で保持するトークン数。再ログインを
/// ループするクライアントの無限増殖ガード。
const TOKEN_KEEP_PER_LABEL: i64 = 10;

// ---- 認証 -----------------------------------------------------------------------

#[derive(Debug)]
pub enum ClientLoginOutcome {
    Ok(SyncToken),
    BadCredentials,
    RateLimited(Duration),
}

/// ClientLogin。パスワード等価の攻撃面なので Web ログインと同一 limiter を共有
/// する（独立させると総当たりの迂回路になる）。Email は照合せず label に保存。
pub async fn client_login(
    state: &AppState,
    email: Option<&str>,
    passwd: &str,
) -> AppResult<ClientLoginOutcome> {
    if let Err(remaining) = check_limiter(state) {
        return Ok(ClientLoginOutcome::RateLimited(remaining));
    }
    // Password の構築時検証（長さ）を通らない入力は正しいパスワードでもあり得ない。
    // DB より先に弾く（ボディ欠落プローブで DB を叩かない）。
    let Ok(password) = Password::parse(passwd) else {
        record_failure(state);
        return Ok(ClientLoginOutcome::BadCredentials);
    };
    // 未セットアップは失敗として数えない（攻撃信号ではない）。
    let Some(phc) = auth_repo::get_credential(&state.db).await? else {
        return Ok(ClientLoginOutcome::BadCredentials);
    };
    if !auth_service::verify_password(password, phc).await? {
        record_failure(state);
        return Ok(ClientLoginOutcome::BadCredentials);
    }
    record_success(state);
    let token = SyncToken::generate();
    let label = email.map(str::trim).filter(|s| !s.is_empty());
    repo::insert_token(&state.db, &hash_token(token.as_str()), label).await?;
    repo::prune_tokens_for_label(&state.db, label, TOKEN_KEEP_PER_LABEL).await?;
    Ok(ClientLoginOutcome::Ok(token))
}

/// GoogleLogin トークンの検証。ハッシュ索引一致で引く（生値比較なし）。
/// ヒット時は last_used_at をタッチ（1時間スロットル）。
pub async fn verify_sync_token(state: &AppState, presented: &str) -> AppResult<Option<Uuid>> {
    let Some(id) = repo::find_token(&state.db, &hash_token(presented)).await? else {
        return Ok(None);
    };
    repo::touch_token(&state.db, id).await?;
    Ok(Some(id))
}

fn check_limiter(state: &AppState) -> Result<(), Duration> {
    state
        .login_limiter
        .lock()
        .expect("login limiter poisoned")
        .check(Instant::now())
}

fn record_failure(state: &AppState) {
    state
        .login_limiter
        .lock()
        .expect("login limiter poisoned")
        .record_failure(Instant::now());
}

fn record_success(state: &AppState) {
    state
        .login_limiter
        .lock()
        .expect("login limiter poisoned")
        .record_success();
}

// ---- ストリーム解決（純関数） ------------------------------------------------------

/// クエリパラメータの型付き表現（s= を除く共通部分）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamQuery {
    pub n: usize,
    /// xt=user/-/state/com.google/read
    pub unread_only: bool,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    /// r=o
    pub ascending: bool,
    pub cursor: Option<i64>,
}

pub fn parse_stream_query(params: &wire::Params) -> StreamQuery {
    let n = params
        .first("n")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE) as usize;
    // it= / ck= / client= は受理して無視（Miniflux 同等）。
    let unread_only = params
        .first("xt")
        .map(StreamId::parse)
        .is_some_and(|s| s == StreamId::Read);
    StreamQuery {
        n,
        unread_only,
        since: params.first("ot").and_then(domain::parse_epoch_secs),
        until: params.first("nt").and_then(domain::parse_epoch_secs),
        ascending: params.first("r") == Some("o"),
        cursor: params.first("c").and_then(|v| v.parse().ok()),
    }
}

/// StreamId → 抽出フィルタ。None = 中身が定義できないストリーム（FeedUrl /
/// broadcast 等）で、空ストリームとして応答する。
pub fn filter_for(stream: &StreamId, q: &StreamQuery) -> Option<repo::StreamFilter> {
    let mut f = repo::StreamFilter {
        unread_only: q.unread_only,
        since: q.since,
        until: q.until,
        ascending: q.ascending,
        cursor: q.cursor,
        limit: (q.n + 1) as i64, // n+1 フェッチ → domain::paginate
        ..repo::StreamFilter::default()
    };
    match stream {
        StreamId::ReadingList => {}
        StreamId::Read => f.read_only = true,
        StreamId::KeptUnread => f.unread_only = true,
        StreamId::Starred => f.starred_only = true,
        StreamId::Feed(id) => f.feed_id = Some(*id),
        StreamId::Label(name) => f.folder_name = Some(name.clone()),
        StreamId::FeedUrl(_) | StreamId::Ignored(_) => return None,
    }
    Some(f)
}

// ---- 読み取り系ユースケース ---------------------------------------------------------

pub async fn item_ids(
    state: &AppState,
    stream: &StreamId,
    params: &wire::Params,
) -> AppResult<wire::ItemRefs> {
    let q = parse_stream_query(params);
    let Some(f) = filter_for(stream, &q) else {
        return Ok(wire::ItemRefs {
            item_refs: Vec::new(),
            continuation: None,
        });
    };
    let rows = repo::list_item_ids(&state.db, &f).await?;
    let (page, continuation) = domain::paginate(rows, q.n);
    Ok(wire::ItemRefs {
        item_refs: page
            .into_iter()
            .map(|id| wire::ItemRef {
                id: ItemId(id).short_form(),
            })
            .collect(),
        continuation,
    })
}

pub async fn stream_contents(
    state: &AppState,
    stream: &StreamId,
    params: &wire::Params,
) -> AppResult<wire::StreamEnvelope> {
    let q = parse_stream_query(params);
    let Some(f) = filter_for(stream, &q) else {
        return Ok(envelope(Vec::new(), None));
    };
    let mut rows = repo::list_stream_items(&state.db, &f).await?;
    let continuation = if rows.len() > q.n {
        rows.truncate(q.n);
        rows.last().map(|r| r.short_id.to_string())
    } else {
        None
    };
    Ok(envelope(rows, continuation))
}

pub async fn items_contents(
    state: &AppState,
    params: &wire::Params,
) -> AppResult<wire::StreamEnvelope> {
    // i= は 1000 件で clamp — 超過分は先頭を処理し 400 にしない。
    // パース不能・負値・stale は黙って落ちる。
    let ids: Vec<i64> = params
        .all("i")
        .into_iter()
        .take(MAX_ITEMS_PER_REQUEST)
        .filter_map(ItemId::parse)
        .map(|i| i.0)
        .collect();
    let rows = repo::items_by_short_ids(&state.db, &ids).await?;
    Ok(envelope(rows, None))
}

pub async fn tag_list(state: &AppState) -> AppResult<wire::TagList> {
    let mut tags = vec![wire::TagEntry {
        // フォルダ 0 件でも starred 行は必ず返す（空配列で choke するクライアント対策）。
        id: "user/-/state/com.google/starred".to_string(),
        kind: None,
    }];
    for name in repo::list_folder_names(&state.db).await? {
        tags.push(wire::TagEntry {
            id: StreamId::label_output(&name),
            kind: Some("folder".to_string()),
        });
    }
    Ok(wire::TagList { tags })
}

pub async fn subscription_list(state: &AppState) -> AppResult<wire::SubscriptionList> {
    let rows = repo::list_subscriptions(&state.db).await?;
    Ok(wire::SubscriptionList {
        subscriptions: rows
            .into_iter()
            .map(|r| wire::Subscription {
                id: StreamId::feed_output(r.id),
                title: r.title.unwrap_or_else(|| r.url.clone()),
                categories: r
                    .folder_name
                    .iter()
                    .map(|name| wire::Category {
                        id: StreamId::label_output(name),
                        label: name.clone(),
                    })
                    .collect(),
                html_url: r.url.clone(),
                url: r.url,
            })
            .collect(),
    })
}

pub async fn unread_count_payload(state: &AppState) -> AppResult<wire::UnreadCounts> {
    let rows = repo::unread_counts(&state.db).await?;
    Ok(compose_unread(&rows))
}

/// UnreadRow（フィードごと）→ §7.15 の完全形（feed 行 + folder 行 + reading-list
/// 合計行 + max）。純関数。
pub fn compose_unread(rows: &[repo::UnreadRow]) -> wire::UnreadCounts {
    let mut entries: Vec<wire::UnreadCountEntry> = Vec::new();
    let mut folders: BTreeMap<&str, (i64, DateTime<Utc>)> = BTreeMap::new();
    let mut total = 0i64;
    let mut total_newest: Option<DateTime<Utc>> = None;

    for r in rows {
        total += r.cnt;
        total_newest = Some(total_newest.map_or(r.newest, |t| t.max(r.newest)));
        entries.push(wire::UnreadCountEntry {
            id: StreamId::feed_output(r.feed_id),
            count: r.cnt,
            newest_item_timestamp_usec: domain::epoch_usec_str(r.newest),
        });
        if let Some(name) = &r.folder_name {
            folders
                .entry(name.as_str())
                .and_modify(|(c, t)| {
                    *c += r.cnt;
                    *t = (*t).max(r.newest);
                })
                .or_insert((r.cnt, r.newest));
        }
    }
    for (name, (cnt, newest)) in folders {
        entries.push(wire::UnreadCountEntry {
            id: StreamId::label_output(name),
            count: cnt,
            newest_item_timestamp_usec: domain::epoch_usec_str(newest),
        });
    }
    entries.push(wire::UnreadCountEntry {
        id: "user/-/state/com.google/reading-list".to_string(),
        count: total,
        newest_item_timestamp_usec: total_newest
            .map(domain::epoch_usec_str)
            .unwrap_or_else(|| "0".to_string()),
    });
    wire::UnreadCounts {
        max: total,
        unreadcounts: entries,
    }
}

// ---- 書き込み系ユースケース ---------------------------------------------------------

/// edit-tag。未知 ID・Label・Ignored が混ざっても Err にしない（常に OK 相当）。
pub async fn edit_tag(state: &AppState, params: &wire::Params) -> AppResult<()> {
    let ids: Vec<i64> = params
        .all("i")
        .into_iter()
        .take(MAX_ITEMS_PER_REQUEST)
        .filter_map(ItemId::parse)
        .map(|i| i.0)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let add: Vec<StreamId> = params.all("a").into_iter().map(StreamId::parse).collect();
    let remove: Vec<StreamId> = params.all("r").into_iter().map(StreamId::parse).collect();
    for op in domain::plan_edits(&add, &remove) {
        match op {
            EditOp::MarkRead => {
                repo::set_read_by_short_ids(&state.db, &ids, true).await?;
            }
            EditOp::MarkUnread => {
                repo::set_read_by_short_ids(&state.db, &ids, false).await?;
            }
            EditOp::Star | EditOp::Unstar => {
                // スターは所有スライス（annotations）の関数を呼ぶ。バッチは
                // ユーザー操作起点で小さいため単件ループで十分。
                let resolved = repo::article_ids_by_short_ids(&state.db, &ids).await?;
                for (_, article_id) in resolved {
                    if op == EditOp::Star {
                        annotations_repo::add_star(&state.db, article_id).await?;
                    } else {
                        annotations_repo::remove_star(&state.db, article_id).await?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn quick_add(state: &AppState, raw: &str) -> AppResult<wire::QuickAddResult> {
    let url = raw.strip_prefix("feed/").unwrap_or(raw).trim();
    // 既購読なら既存 streamId を返す（クライアントのリトライで重複 500 にしない）。
    let existing: Option<(Uuid, Option<String>, String)> =
        sqlx::query_as("SELECT id, title, url FROM feeds WHERE url = $1")
            .bind(url)
            .fetch_optional(&state.db)
            .await?;
    if let Some((id, title, feed_url)) = existing {
        return Ok(wire::QuickAddResult {
            num_results: 1,
            query: raw.to_string(),
            stream_id: Some(StreamId::feed_output(id)),
            stream_name: Some(title.unwrap_or(feed_url)),
        });
    }
    match feeds_service::create_feed(state, url).await {
        Ok(feed) => Ok(wire::QuickAddResult {
            num_results: 1,
            query: raw.to_string(),
            // subscription/list と厳密一致（NNW が直後に照合する — 生命線）。
            stream_id: Some(StreamId::feed_output(feed.id.0)),
            // 初回フェッチは背景化されているためタイトル未確定時は URL
            // （次回同期で治る）。
            stream_name: Some(feed.title.unwrap_or(feed.url)),
        }),
        // URL 不能は 200 のまま numResults=0（クライアントは失敗 UI を出す）。
        Err(AppError::Validation(_)) => Ok(wire::QuickAddResult {
            num_results: 0,
            query: raw.to_string(),
            stream_id: None,
            stream_name: None,
        }),
        Err(e) => Err(e),
    }
}

/// subscription/edit（ac=edit / unsubscribe / subscribe）。
/// 対象が既に無い場合も 200 OK（クライアントのリトライを止める）。
pub async fn subscription_edit(state: &AppState, params: &wire::Params) -> AppResult<()> {
    let ac = params.first("ac").unwrap_or("");
    let streams: Vec<StreamId> = params.all("s").into_iter().map(StreamId::parse).collect();
    let title = params
        .first("t")
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);
    let add_label: Option<String> =
        params
            .all("a")
            .into_iter()
            .find_map(|s| match StreamId::parse(s) {
                StreamId::Label(name) => Some(name),
                _ => None,
            });
    let remove_label = params
        .all("r")
        .into_iter()
        .any(|s| matches!(StreamId::parse(s), StreamId::Label(_)));

    match ac {
        "unsubscribe" => {
            for s in &streams {
                if let StreamId::Feed(id) = s {
                    match feeds_service::delete_feed(state, FeedId(*id)).await {
                        Ok(()) | Err(AppError::NotFound) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            Ok(())
        }
        "edit" => {
            // NNW の「フォルダ移動」は r=旧 + a=新 の同時送信 → a= 優先。
            // r= のみ（a= なし）は未分類へ（NNW #3512: 未実装だと「外す」が壊れる）。
            let folder: Option<Option<FolderId>> = if let Some(name) = &add_label {
                Some(Some(FolderId(ensure_folder(state, name).await?)))
            } else if remove_label {
                Some(None)
            } else {
                None
            };
            for s in &streams {
                if let StreamId::Feed(id) = s {
                    if title.is_none() && folder.is_none() {
                        continue;
                    }
                    match feeds_service::update_feed(
                        state,
                        FeedId(*id),
                        title.clone(),
                        folder,
                        None,
                    )
                    .await
                    {
                        Ok(_) | Err(AppError::NotFound) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            Ok(())
        }
        "subscribe" => {
            for s in &streams {
                if let StreamId::FeedUrl(url) = s {
                    let result = quick_add(state, url).await?;
                    if let (Some(stream_id), Some(name)) = (&result.stream_id, &add_label) {
                        if let StreamId::Feed(id) = StreamId::parse(stream_id) {
                            let folder = FolderId(ensure_folder(state, name).await?);
                            match feeds_service::update_feed(
                                state,
                                FeedId(id),
                                None,
                                Some(Some(folder)),
                                None,
                            )
                            .await
                            {
                                Ok(_) | Err(AppError::NotFound) => {}
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }
            }
            Ok(())
        }
        // 未知 ac は受理して OK（クライアント互換のばらつき対策）。
        _ => Ok(()),
    }
}

/// GReader にフォルダ作成 API はなくラベル付与が作成を兼ねる（暗黙作成）。
async fn ensure_folder(state: &AppState, name: &str) -> AppResult<Uuid> {
    let name = FolderName::parse(name).map_err(AppError::Validation)?;
    if let Some(id) = repo::folder_id_by_name(&state.db, name.as_str()).await? {
        return Ok(id);
    }
    let folder = folders_repo::insert(&state.db, name.as_str()).await?;
    Ok(folder.id.0)
}

/// rename-tag。対象フォルダが無くても 200 OK（no-op）。
pub async fn rename_folder(state: &AppState, params: &wire::Params) -> AppResult<()> {
    let (Some(StreamId::Label(from)), Some(StreamId::Label(to))) = (
        params.first("s").map(StreamId::parse),
        params.first("dest").map(StreamId::parse),
    ) else {
        return Ok(());
    };
    let Ok(to) = FolderName::parse(to) else {
        return Ok(());
    };
    let Some(id) = repo::folder_id_by_name(&state.db, &from).await? else {
        return Ok(());
    };
    match folders_repo::update_name(&state.db, FolderId(id), to.as_str()).await {
        Ok(_) | Err(AppError::NotFound) => Ok(()),
        Err(e) => Err(e),
    }
}

/// disable-tag。folders::delete は FK SET NULL で所属フィードを未分類化する
/// （= GReader の期待動作）。
pub async fn delete_folders(state: &AppState, params: &wire::Params) -> AppResult<()> {
    for s in params.all("s") {
        if let StreamId::Label(name) = StreamId::parse(s) {
            if let Some(id) = repo::folder_id_by_name(&state.db, &name).await? {
                folders_repo::delete(&state.db, FolderId(id)).await?;
            }
        }
    }
    Ok(())
}

/// mark-all-as-read。スコープ外のストリーム（starred 等）は no-op で OK。
pub async fn mark_all_as_read(state: &AppState, params: &wire::Params) -> AppResult<()> {
    let stream = params
        .first("s")
        .map(StreamId::parse)
        .unwrap_or(StreamId::ReadingList);
    let ts = domain::parse_ts_param(params.first("ts"));
    let (feed_id, folder_name) = match &stream {
        StreamId::ReadingList => (None, None),
        StreamId::Feed(id) => (Some(*id), None),
        StreamId::Label(name) => (None, Some(name.as_str())),
        _ => return Ok(()),
    };
    repo::mark_all_read(&state.db, feed_id, folder_name, ts).await?;
    Ok(())
}

// ---- ワイヤ形式への変換（純関数） ---------------------------------------------------

fn envelope(rows: Vec<repo::ItemRow>, continuation: Option<String>) -> wire::StreamEnvelope {
    wire::StreamEnvelope {
        // NNW decoder が必須とする固定 id（§7.7）。
        id: "user/-/state/com.google/reading-list".to_string(),
        updated: domain::epoch_secs(Utc::now()),
        items: rows.into_iter().map(item_to_wire).collect(),
        continuation,
    }
}

fn item_to_wire(r: repo::ItemRow) -> wire::Item {
    let mut categories = vec!["user/-/state/com.google/reading-list".to_string()];
    if let Some(folder) = &r.folder_name {
        categories.push(StreamId::label_output(folder));
    }
    if r.is_read {
        categories.push("user/-/state/com.google/read".to_string());
    }
    if r.starred {
        categories.push("user/-/state/com.google/starred".to_string());
    }
    let content = domain::truncate_content(&r.content).to_string();
    wire::Item {
        id: ItemId(r.short_id).long_form(),
        crawl_time_msec: domain::epoch_msec_str(r.created_at),
        timestamp_usec: domain::epoch_usec_str(r.created_at),
        published: domain::epoch_secs(r.published_at.unwrap_or(r.created_at)),
        title: r.title,
        author: r.author,
        canonical: vec![wire::Href {
            href: r.url.clone(),
        }],
        alternate: vec![wire::Href { href: r.url }],
        categories,
        origin: wire::Origin {
            stream_id: StreamId::feed_output(r.feed_id),
            title: r.feed_title.clone().unwrap_or_else(|| r.feed_url.clone()),
            html_url: r.feed_url,
        },
        summary: wire::Content {
            content: content.clone(),
        },
        content: wire::Content { content },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(qs: &str) -> wire::Params {
        wire::Params::from(None, qs.as_bytes())
    }

    // ---- parse_stream_query -------------------------------------------------

    #[test]
    fn stream_query_defaults() {
        let q = parse_stream_query(&params(""));
        assert_eq!(q.n, DEFAULT_PAGE_SIZE as usize);
        assert!(!q.unread_only && !q.ascending);
        assert_eq!(q.since, None);
        assert_eq!(q.cursor, None);
    }

    #[test]
    fn stream_query_clamps_n() {
        assert_eq!(parse_stream_query(&params("n=0")).n, 1);
        assert_eq!(parse_stream_query(&params("n=99999")).n, 1000);
        assert_eq!(parse_stream_query(&params("n=garbage")).n, 20);
        assert_eq!(parse_stream_query(&params("n=50")).n, 50);
    }

    #[test]
    fn stream_query_parses_xt_ot_nt_r_c() {
        let q = parse_stream_query(&params(
            "xt=user%2F-%2Fstate%2Fcom.google%2Fread&ot=1751856000&nt=1751857000&r=o&c=42",
        ));
        assert!(q.unread_only);
        assert_eq!(q.since.unwrap().timestamp(), 1_751_856_000);
        assert_eq!(q.until.unwrap().timestamp(), 1_751_857_000);
        assert!(q.ascending);
        assert_eq!(q.cursor, Some(42));
        // xt が read 以外なら無視。
        assert!(!parse_stream_query(&params("xt=user/-/state/com.google/starred")).unread_only);
        // r=d / r=n は降順のまま。
        assert!(!parse_stream_query(&params("r=d")).ascending);
    }

    // ---- filter_for -----------------------------------------------------------

    fn q(n: usize) -> StreamQuery {
        StreamQuery {
            n,
            unread_only: false,
            since: None,
            until: None,
            ascending: false,
            cursor: None,
        }
    }

    #[test]
    fn filter_for_maps_streams_and_sets_n_plus_one() {
        let f = filter_for(&StreamId::ReadingList, &q(20)).unwrap();
        assert_eq!(f.limit, 21);
        assert!(!f.read_only && !f.starred_only && !f.unread_only);

        assert!(filter_for(&StreamId::Read, &q(20)).unwrap().read_only);
        assert!(
            filter_for(&StreamId::KeptUnread, &q(20))
                .unwrap()
                .unread_only
        );
        assert!(filter_for(&StreamId::Starred, &q(20)).unwrap().starred_only);

        let id = Uuid::new_v4();
        assert_eq!(
            filter_for(&StreamId::Feed(id), &q(20)).unwrap().feed_id,
            Some(id)
        );
        assert_eq!(
            filter_for(&StreamId::Label("Tech".into()), &q(20))
                .unwrap()
                .folder_name,
            Some("Tech".into())
        );
    }

    #[test]
    fn filter_for_undefined_streams_is_none() {
        assert!(filter_for(&StreamId::FeedUrl("https://x".into()), &q(20)).is_none());
        assert!(filter_for(&StreamId::Ignored("broadcast".into()), &q(20)).is_none());
    }

    // ---- item_to_wire -----------------------------------------------------------

    fn row(is_read: bool, starred: bool, folder: Option<&str>) -> repo::ItemRow {
        repo::ItemRow {
            short_id: 0x5f3,
            url: "https://example.org/post".into(),
            title: "Post".into(),
            content: "<p>body</p>".into(),
            author: Some("alice".into()),
            published_at: DateTime::from_timestamp(1_751_850_000, 0),
            created_at: DateTime::from_timestamp(1_751_856_000, 123_456_000).unwrap(),
            is_read,
            starred,
            feed_id: Uuid::nil(),
            feed_title: None,
            feed_url: "https://example.org/feed.xml".into(),
            folder_name: folder.map(String::from),
        }
    }

    #[test]
    fn item_to_wire_maps_ids_times_and_categories() {
        let item = item_to_wire(row(true, true, Some("Tech")));
        assert_eq!(item.id, "tag:google.com,2005:reader/item/00000000000005f3");
        assert_eq!(item.crawl_time_msec, "1751856000123");
        assert_eq!(item.timestamp_usec, "1751856000123456");
        assert_eq!(item.published, 1_751_850_000);
        assert_eq!(
            item.categories,
            vec![
                "user/-/state/com.google/reading-list",
                "user/-/label/Tech",
                "user/-/state/com.google/read",
                "user/-/state/com.google/starred",
            ]
        );
        assert_eq!(item.canonical[0].href, "https://example.org/post");
        assert_eq!(item.alternate[0].href, "https://example.org/post");
        // feed_title NULL は feed_url で代用。
        assert_eq!(item.origin.title, "https://example.org/feed.xml");
        assert_eq!(item.origin.stream_id, format!("feed/{}", Uuid::nil()));
        assert_eq!(item.summary.content, "<p>body</p>");
        assert_eq!(item.content.content, "<p>body</p>");
    }

    #[test]
    fn item_to_wire_unread_no_folder_has_minimal_categories() {
        let item = item_to_wire(row(false, false, None));
        assert_eq!(
            item.categories,
            vec!["user/-/state/com.google/reading-list"]
        );
        // published_at NULL は created_at で代用。
        let mut r = row(false, false, None);
        r.published_at = None;
        assert_eq!(item_to_wire(r).published, 1_751_856_000);
    }

    // ---- compose_unread -----------------------------------------------------------

    #[test]
    fn compose_unread_aggregates_folders_and_total() {
        let t1 = DateTime::from_timestamp(1_751_856_000, 0).unwrap();
        let t2 = DateTime::from_timestamp(1_751_857_000, 0).unwrap();
        let f1 = Uuid::new_v4();
        let f2 = Uuid::new_v4();
        let rows = vec![
            repo::UnreadRow {
                feed_id: f1,
                folder_name: Some("Tech".into()),
                cnt: 5,
                newest: t1,
            },
            repo::UnreadRow {
                feed_id: f2,
                folder_name: Some("Tech".into()),
                cnt: 12,
                newest: t2,
            },
        ];
        let out = compose_unread(&rows);
        assert_eq!(out.max, 17);
        // feed 2 行 + folder 1 行 + reading-list 1 行。
        assert_eq!(out.unreadcounts.len(), 4);
        let folder = out
            .unreadcounts
            .iter()
            .find(|e| e.id == "user/-/label/Tech")
            .unwrap();
        assert_eq!(folder.count, 17);
        assert_eq!(folder.newest_item_timestamp_usec, "1751857000000000");
        let total = out
            .unreadcounts
            .iter()
            .find(|e| e.id == "user/-/state/com.google/reading-list")
            .unwrap();
        assert_eq!(total.count, 17);
    }

    #[test]
    fn compose_unread_empty_still_has_reading_list_row() {
        let out = compose_unread(&[]);
        assert_eq!(out.max, 0);
        assert_eq!(out.unreadcounts.len(), 1);
        assert_eq!(
            out.unreadcounts[0].id,
            "user/-/state/com.google/reading-list"
        );
        assert_eq!(out.unreadcounts[0].count, 0);
    }

    // ---- edit-tag（実 DB 往復・§9.3） ------------------------------------------

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn edit_tag_roundtrip_read_star_with_stale_ids() {
        use crate::shared::auth::LoginLimiter;
        use crate::shared::config::AppConfig;
        use std::sync::{Arc, Mutex};

        let db = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
            .await
            .unwrap();
        let state = AppState {
            db: db.clone(),
            config: Arc::new(AppConfig::for_test()),
            http: reqwest::Client::new(),
            http_external: reqwest::Client::new(),
            login_limiter: Arc::new(Mutex::new(LoginLimiter::default())),
        };

        let feed: Uuid =
            sqlx::query_scalar("INSERT INTO feeds (id, url) VALUES ($1, $2) RETURNING id")
                .bind(Uuid::new_v4())
                .bind(format!("https://example.com/sync-svc/{}", Uuid::new_v4()))
                .fetch_one(&db)
                .await
                .unwrap();
        let (article_id, short_id): (Uuid, i64) = sqlx::query_as(
            r#"INSERT INTO articles (id, feed_id, url, title, content)
               VALUES (gen_random_uuid(), $1, $2, 't', 'c') RETURNING id, short_id"#,
        )
        .bind(feed)
        .bind(format!(
            "https://example.com/sync-svc-post/{}",
            Uuid::new_v4()
        ))
        .fetch_one(&db)
        .await
        .unwrap();

        let long = ItemId(short_id).long_form();
        let stale = "i=9223372036854775000";

        // 既読化（stale 混在・kept-unread 冗長ペア）→ OK・is_read=true。
        let p = wire::Params::from(
            None,
            format!(
                "i={long}&{stale}&a=user/-/state/com.google/read&r=user/-/state/com.google/kept-unread"
            )
            .as_bytes(),
        );
        edit_tag(&state, &p).await.unwrap();
        let is_read: bool = sqlx::query_scalar("SELECT is_read FROM articles WHERE id = $1")
            .bind(article_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(is_read);

        // 未読化（r=read）。
        let p = wire::Params::from(
            None,
            format!("i={long}&r=user/-/state/com.google/read").as_bytes(),
        );
        edit_tag(&state, &p).await.unwrap();
        let is_read: bool = sqlx::query_scalar("SELECT is_read FROM articles WHERE id = $1")
            .bind(article_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(!is_read);

        // スター → 解除（annotations 経由）。
        let p = wire::Params::from(
            None,
            format!("i={long}&a=user/-/state/com.google/starred").as_bytes(),
        );
        edit_tag(&state, &p).await.unwrap();
        let starred: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM article_stars WHERE article_id = $1)")
                .bind(article_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(starred);
        let p = wire::Params::from(
            None,
            format!("i={long}&r=user/-/state/com.google/starred").as_bytes(),
        );
        edit_tag(&state, &p).await.unwrap();
        let starred: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM article_stars WHERE article_id = $1)")
                .bind(article_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(!starred);

        // ラベル・broadcast だけなら何も起きず OK。
        let p = wire::Params::from(
            None,
            format!("i={long}&a=user/-/label/Tech&a=user/-/state/com.google/broadcast").as_bytes(),
        );
        edit_tag(&state, &p).await.unwrap();

        sqlx::query("DELETE FROM articles WHERE feed_id = $1")
            .bind(feed)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM feeds WHERE id = $1")
            .bind(feed)
            .execute(&db)
            .await
            .unwrap();
    }
}
