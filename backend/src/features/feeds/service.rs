//! Use cases for the feeds slice: create/list/delete and the crawl loop.

use feed_rs::parser;

use super::domain::{Feed, FeedId, FeedUrl};
use super::repository;
use crate::features::articles;
use crate::features::folders::domain::FolderId;
use crate::shared::error::{AppError, AppResult};
use crate::shared::fetch::{read_body_limited, safe_get, UrlGuard};
use crate::shared::state::AppState;

/// Hard cap on a fetched feed document. Full-content feeds run a few MB;
/// anything past this is hostile or broken (guards the crawl task from OOM).
const FEED_MAX_BYTES: usize = 10 * 1024 * 1024;

pub async fn create_feed(state: &AppState, raw_url: &str) -> AppResult<Feed> {
    let url = FeedUrl::parse(raw_url).map_err(AppError::Validation)?;
    let feed = repository::insert(&state.db, url.as_str()).await?;
    // Best-effort initial fetch in the background: a slow upstream must not
    // hold up the POST response. Failures are recorded to feed_health by
    // fetch_and_store, so they surface in the manage screen.
    let state = state.clone();
    let spawned = feed.clone();
    tokio::spawn(async move {
        if let Err(e) = fetch_and_store(&state, &spawned).await {
            tracing::warn!(error = %e, feed = %spawned.url, "initial fetch failed");
        }
    });
    Ok(feed)
}

pub async fn list_feeds(state: &AppState) -> AppResult<Vec<Feed>> {
    repository::list_all(&state.db).await
}

pub async fn delete_feed(state: &AppState, id: FeedId) -> AppResult<()> {
    let affected = repository::delete(&state.db, id).await?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn update_feed(
    state: &AppState,
    id: FeedId,
    title: Option<String>,
    folder_id: Option<Option<FolderId>>,
    priority: Option<i16>,
) -> AppResult<Feed> {
    if let Some(t) = &title {
        if t.trim().is_empty() {
            return Err(AppError::Validation("title must not be empty".into()));
        }
    }
    // 優先度は 0=通常 / 1=高 の2値のみ受け付ける（#31）。
    if let Some(p) = priority {
        if !(0..=1).contains(&p) {
            return Err(AppError::Validation("priority must be 0 or 1".into()));
        }
    }
    // 実在しないフォルダへの割当は 400 に整形（FK 違反の 500 を避ける advisory）。
    if let Some(Some(fid)) = folder_id {
        if !repository::folder_exists(&state.db, fid).await? {
            return Err(AppError::Validation("folder not found".into()));
        }
    }
    repository::update(&state.db, id, title.as_deref(), folder_id, priority).await
}

/// 単一フィードのみ再取得（従来の全件 refresh ではなく当該フィードだけ）。
pub async fn refresh_one(state: &AppState, id: FeedId) -> AppResult<Feed> {
    let feed = repository::get(&state.db, id).await?; // 無ければ NotFound
    fetch_and_store(state, &feed).await?; // 既存ロジックを再利用
    repository::get(&state.db, id).await // 更新後（last_fetched_at 反映）を返す
}

pub async fn refresh_all_feeds(state: &AppState) -> AppResult<()> {
    let feeds = repository::list_all(&state.db).await?;
    tracing::info!(count = feeds.len(), "refreshing feeds");
    for feed in feeds {
        if let Err(e) = fetch_and_store(state, &feed).await {
            tracing::error!(error = %e, feed = %feed.url, "fetch failed");
        }
    }
    Ok(())
}

/// Fetch choke point. Records the crawl outcome to feed_health (#21) then returns
/// the result unchanged. Scheduled refresh, manual refresh, and the initial fetch
/// on create all go through here, so one hook covers every path.
pub async fn fetch_and_store(state: &AppState, feed: &Feed) -> AppResult<()> {
    let result = fetch_and_store_inner(state, feed).await;
    match &result {
        Ok(()) => {
            if let Err(e) =
                crate::features::feed_health::repository::record_success(&state.db, feed.id.0).await
            {
                tracing::warn!(error = %e, feed = %feed.url, "record_success failed");
            }
        }
        Err(e) => {
            if let Err(re) = crate::features::feed_health::repository::record_failure(
                &state.db,
                feed.id.0,
                &e.to_string(),
            )
            .await
            {
                tracing::warn!(error = %re, feed = %feed.url, "record_failure failed");
            }
        }
    }
    result
}

/// Fetch one feed over HTTP, parse it, and upsert its entries as articles.
async fn fetch_and_store_inner(state: &AppState, feed: &Feed) -> AppResult<()> {
    let guard = UrlGuard::from_config(&state.config);
    let resp = safe_get(&state.http_external, &guard, &feed.url, |rb| rb).await?;
    let bytes = read_body_limited(resp, FEED_MAX_BYTES).await?;

    let parsed =
        parser::parse(&bytes[..]).map_err(|e| AppError::Upstream(format!("parse error: {e}")))?;

    let feed_title = parsed.title.as_ref().map(|t| t.content.clone());

    for entry in parsed.entries {
        let url = entry_url(&entry.links).unwrap_or_default();
        if url.is_empty() {
            continue;
        }
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| "(untitled)".to_string());
        let content = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
            .unwrap_or_default();
        let published = entry.published.or(entry.updated);

        articles::repository::upsert(
            &state.db,
            FeedId(feed.id.0),
            &url,
            &title,
            &content,
            published,
        )
        .await?;

        // #28: persist the author so rule conditions can match it (best-effort).
        if let Some(author) = entry.authors.first().map(|p| p.name.clone()) {
            if !author.trim().is_empty() {
                let _ = articles::repository::set_author(&state.db, &url, &author).await;
            }
        }

        // Optional crawl-time full-content extraction (EXTRACT_ON_CRAWL=true).
        // Best-effort + idempotent (skips already-extracted rows). Default off,
        // so behavior is unchanged unless explicitly opted in.
        if state.config.extract_on_crawl {
            if let Some(id) = articles::repository::id_by_url(&state.db, &url).await? {
                crate::features::extraction::service::extract_best_effort(state, id).await;
            }
        }
    }

    repository::touch_fetched(&state.db, feed.id, feed_title.as_deref()).await?;
    // #28: apply automation rules to the freshly ingested articles (best-effort;
    // a failure here must not fail the crawl).
    if let Err(e) =
        crate::features::automation_rules::service::apply_for_feed(state, feed.id.0).await
    {
        tracing::error!(error = %e, feed = %feed.url, "rule application failed");
    }
    Ok(())
}

/// entry の links から記事本体の URL を選ぶ。
/// Blogger 等の Atom は `rel="replies"`（コメントフィード）等が先頭に並ぶため、
/// 先頭を盲目的に取ると「元記事を開く」がフィード URL になる。
/// 優先順: rel="alternate"（Atom の記事本体）→ rel 無し（RSS の通常形）→ 先頭。
fn entry_url(links: &[feed_rs::model::Link]) -> Option<String> {
    links
        .iter()
        .find(|l| l.rel.as_deref() == Some("alternate"))
        .or_else(|| links.iter().find(|l| l.rel.is_none()))
        .or_else(|| links.first())
        .map(|l| l.href.clone())
}

#[cfg(test)]
mod tests {
    use super::entry_url;
    use feed_rs::parser;

    /// create_feed は初回フェッチの完了を待たずに応答すること（遅いフィードでも
    /// POST /api/feeds が即返る）。実 DB が必要なので ignored。実行方法:
    ///   DATABASE_URL=... cargo test create_feed_returns -- --ignored
    #[tokio::test]
    #[ignore]
    async fn create_feed_returns_before_initial_fetch_completes() {
        use crate::shared::config::AppConfig;
        use crate::shared::state::AppState;
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        let db = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").expect("DATABASE_URL"))
            .await
            .expect("connect");
        let mut config = AppConfig::for_test();
        // ローカルテストサーバー(127.0.0.1)を SSRF ガードに通すため
        config.allow_private_networks = true;
        let state = AppState {
            db: db.clone(),
            config: Arc::new(config),
            http: reqwest::Client::new(),
            http_external: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            login_limiter: Arc::new(std::sync::Mutex::new(
                crate::shared::auth::LoginLimiter::default(),
            )),
        };

        // 2秒待ってから RSS を返すローカルサーバー（遅い外部サイトの再現）
        const FETCH_DELAY: Duration = Duration::from_secs(2);
        let item_url = format!("https://blog.example/bg-test-{}", uuid::Uuid::new_v4());
        let rss = format!(
            r#"<?xml version="1.0"?>
            <rss version="2.0"><channel><title>bg test</title>
              <item><title>post</title><link>{item_url}</link></item>
            </channel></rss>"#
        );
        let app = axum::Router::new().route(
            "/feed",
            axum::routing::get(move || {
                let rss = rss.clone();
                async move {
                    tokio::time::sleep(FETCH_DELAY).await;
                    rss
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let started = Instant::now();
        let feed = super::create_feed(&state, &format!("http://{addr}/feed"))
            .await
            .expect("create_feed");
        let elapsed = started.elapsed();

        // 背景フェッチはいずれ完走し、記事が保存されること（最大10秒待つ）
        let mut stored = 0i64;
        for _ in 0..50 {
            let (n,): (i64,) =
                sqlx::query_as("SELECT count(*) FROM articles WHERE feed_id = $1")
                    .bind(feed.id.0)
                    .fetch_one(&db)
                    .await
                    .unwrap();
            if n > 0 {
                stored = n;
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        // 後始末（unique 制約を汚さない）
        sqlx::query("DELETE FROM articles WHERE feed_id = $1")
            .bind(feed.id.0)
            .execute(&db)
            .await
            .unwrap();
        super::delete_feed(&state, feed.id).await.unwrap();

        assert!(stored > 0, "background fetch did not store articles");
        assert!(
            elapsed < FETCH_DELAY,
            "create_feed blocked on the initial fetch: {elapsed:?}"
        );
    }

    fn entry_links(xml: &str) -> Vec<feed_rs::model::Link> {
        let parsed = parser::parse(xml.as_bytes()).expect("parse");
        parsed.entries.into_iter().next().expect("entry").links
    }

    #[test]
    fn atom_prefers_rel_alternate_over_replies() {
        // Blogger 実物の並び: replies (atom) → replies (html) → edit → self → alternate
        let links = entry_links(
            r#"<?xml version="1.0"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
              <id>tag:blog</id><title>t</title><updated>2026-01-01T00:00:00Z</updated>
              <entry>
                <id>tag:post</id><title>post</title><updated>2026-01-01T00:00:00Z</updated>
                <link rel="replies" type="application/atom+xml" href="https://blog.example/feeds/1/comments/default"/>
                <link rel="replies" type="text/html" href="https://blog.example/2026/01/post.html#comment-form"/>
                <link rel="edit" href="https://www.blogger.com/feeds/9/posts/default/1"/>
                <link rel="self" href="https://www.blogger.com/feeds/9/posts/default/1"/>
                <link rel="alternate" type="text/html" href="https://blog.example/2026/01/post.html"/>
              </entry>
            </feed>"#,
        );
        assert_eq!(
            entry_url(&links).as_deref(),
            Some("https://blog.example/2026/01/post.html")
        );
    }

    #[test]
    fn rss_single_link_is_used() {
        let links = entry_links(
            r#"<?xml version="1.0"?>
            <rss version="2.0"><channel><title>t</title>
              <item><title>post</title><link>https://blog.example/post</link></item>
            </channel></rss>"#,
        );
        assert_eq!(
            entry_url(&links).as_deref(),
            Some("https://blog.example/post")
        );
    }

    #[test]
    fn no_links_returns_none() {
        assert_eq!(entry_url(&[]), None);
    }
}
