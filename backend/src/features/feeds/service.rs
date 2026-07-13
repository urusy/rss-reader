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
    extract_full_content: Option<bool>,
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
    repository::update(
        &state.db,
        id,
        title.as_deref(),
        folder_id,
        priority,
        extract_full_content,
    )
    .await
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

    // エントリを集めて1クエリで一括 upsert（記事ごとの直列 3 クエリをやめ、
    // エントリの多いフィードの取込みを速くする）。author（#28 ルール条件用）も
    // 同じ INSERT に畳む（既存値優先の意味論は upsert_batch 側が持つ）。
    let items: Vec<articles::repository::NewArticle> = parsed
        .entries
        .into_iter()
        .filter_map(|entry| {
            let url = entry_url(&entry.links)?;
            if url.is_empty() {
                return None;
            }
            Some(articles::repository::NewArticle {
                url,
                title: entry
                    .title
                    .as_ref()
                    .map(|t| t.content.clone())
                    .unwrap_or_else(|| "(untitled)".to_string()),
                content: entry
                    .content
                    .as_ref()
                    .and_then(|c| c.body.clone())
                    .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
                    .unwrap_or_default(),
                published_at: entry.published.or(entry.updated),
                author: entry
                    .authors
                    .first()
                    .map(|p| p.name.clone())
                    .filter(|a| !a.trim().is_empty()),
            })
        })
        .collect();
    let stored = articles::repository::upsert_batch(&state.db, FeedId(feed.id.0), &items).await?;

    // タイトル確定と #28 ルール適用は抽出より先に済ませる。抽出は記事ごとの
    // 直列 HTTP 取得で数分かかりうるため、後ろに置くと登録直後のフィードが
    // タイトル無しのまま見える（ルールも feed 由来の content にしか依存しない）。
    repository::touch_fetched(&state.db, feed.id, feed_title.as_deref()).await?;
    // #28: apply automation rules to the freshly ingested articles (best-effort;
    // a failure here must not fail the crawl).
    if let Err(e) =
        crate::features::automation_rules::service::apply_for_feed(state, feed.id.0).await
    {
        tracing::error!(error = %e, feed = %feed.url, "rule application failed");
    }

    // Crawl-time full-content extraction (best-effort + idempotent: skips
    // already-extracted rows). 最も遅い処理なので必ず最後。
    //   - 全記事: グローバル(EXTRACT_ON_CRAWL) or フィード個別(extract_full_content)
    //   - 本文がない記事のみ: 常時（ヘッドラインのみのフィード対策。設定不要）
    let extract_all =
        auto_extract_enabled(state.config.extract_on_crawl, feed.extract_full_content);
    let thin_urls: std::collections::HashSet<&str> = items
        .iter()
        .filter(|i| content_is_thin(&i.content, state.config.extract_min_chars))
        .map(|i| i.url.as_str())
        .collect();
    for (id, url) in &stored {
        if extract_all || thin_urls.contains(url.as_str()) {
            crate::features::extraction::service::extract_best_effort(state, *id).await;
        }
    }
    Ok(())
}

/// クロール時の全文自動抽出を「フィード全記事」に対して行うか。
/// グローバル設定(EXTRACT_ON_CRAWL)とフィード個別設定(feeds.extract_full_content)の OR。
/// これが false でも、本文がない記事（`content_is_thin`）は個別に自動抽出される。
fn auto_extract_enabled(global: bool, per_feed: bool) -> bool {
    global || per_feed
}

/// 「本文がない」判定。content の平文長（タグを除いた文字数）が min_chars
/// （EXTRACT_MIN_CHARS、抽出結果を意味のある本文とみなす既存しきい値）未満なら
/// thin。ヘッドラインのみのフィード（description がタイトルの丸写し）が典型で、
/// この場合は設定なしでもクロール時に全文を自動抽出する。
fn content_is_thin(content_html: &str, min_chars: usize) -> bool {
    crate::features::extraction::domain::plain_text_len(content_html) < min_chars
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
    use super::{auto_extract_enabled, content_is_thin, entry_url};
    use feed_rs::parser;

    /// クロール時の全文自動抽出は、グローバル設定(EXTRACT_ON_CRAWL)か
    /// フィード個別設定(feeds.extract_full_content)のどちらかが有効なら行う。
    #[test]
    fn auto_extract_enabled_is_global_or_per_feed() {
        assert!(!auto_extract_enabled(false, false));
        assert!(auto_extract_enabled(true, false)); // グローバル一括
        assert!(auto_extract_enabled(false, true)); // フィード個別
        assert!(auto_extract_enabled(true, true));
    }

    /// 「本文がない」判定: content の平文長が min_chars 未満なら thin。
    /// ヘッドラインのみのフィード（description がタイトルの丸写し）はここに
    /// 落ち、設定なしでもクロール時に自動抽出される。
    #[test]
    fn content_is_thin_detects_headline_only_entries() {
        // 空・タイトル丸写し程度 → thin
        assert!(content_is_thin("", 200));
        assert!(content_is_thin(
            "Claude Cowork is coming to mobile and web",
            200
        ));
        // HTML タグは数えない（タグで水増しされた空要素も thin）
        assert!(content_is_thin("<p></p><div><span></span></div>", 200));
        // 十分な長さの本文 → thin ではない
        let body = format!("<p>{}</p>", "あ".repeat(300));
        assert!(!content_is_thin(&body, 200));
        // 境界: ちょうど min_chars は thin ではない
        assert!(!content_is_thin(&"x".repeat(200), 200));
        assert!(content_is_thin(&"x".repeat(199), 200));
    }

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
            let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM articles WHERE feed_id = $1")
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

    /// フィードタイトルの確定（touch_fetched）は本文なし記事の自動全文抽出より
    /// 先に行われること。抽出は記事ごとの直列 HTTP 取得で数分かかりうるため、
    /// 後回しにしないと登録直後のフィードがタイトル無しのまま見える。
    /// 実 DB が必要なので ignored。実行方法:
    ///   DATABASE_URL=... cargo test feed_title_is_set -- --ignored
    #[tokio::test]
    #[ignore]
    async fn feed_title_is_set_before_auto_extraction_completes() {
        use crate::shared::config::AppConfig;
        use crate::shared::state::AppState;
        use std::sync::Arc;
        use std::time::Duration;

        let db = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").expect("DATABASE_URL"))
            .await
            .expect("connect");
        let mut config = AppConfig::for_test();
        config.allow_private_networks = true; // ローカルテストサーバーを SSRF ガードに通す
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

        // ヘッドラインのみのフィード（即応答）＋ 3秒かかる記事ページ。
        // description がタイトル丸写し → thin 判定 → クロール時自動抽出が走る。
        const EXTRACT_DELAY: Duration = Duration::from_secs(3);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let rss = format!(
            r#"<?xml version="1.0"?>
            <rss version="2.0"><channel><title>title-before-extract</title>
              <item><title>slow post</title><link>http://{addr}/article.html</link>
                <description>slow post</description></item>
            </channel></rss>"#
        );
        let article = format!("<article><p>{}</p></article>", "real body ".repeat(100));
        let app = axum::Router::new()
            .route(
                "/feed",
                axum::routing::get(move || {
                    let rss = rss.clone();
                    async move { rss }
                }),
            )
            .route(
                "/article.html",
                axum::routing::get(move || {
                    let article = article.clone();
                    async move {
                        tokio::time::sleep(EXTRACT_DELAY).await;
                        axum::response::Html(article)
                    }
                }),
            );
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let feed = super::repository::insert(&db, &format!("http://{addr}/feed"))
            .await
            .expect("insert feed");
        let feed_id = feed.id;
        let crawl_state = state.clone();
        let crawl = tokio::spawn(async move { super::fetch_and_store(&crawl_state, &feed).await });

        // 抽出（3秒）が終わるより十分前に、タイトルが確定していること
        let mut title: Option<String> = None;
        for _ in 0..15 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let (t,): (Option<String>,) = sqlx::query_as("SELECT title FROM feeds WHERE id = $1")
                .bind(feed_id.0)
                .fetch_one(&db)
                .await
                .unwrap();
            if t.is_some() {
                title = t;
                break;
            }
        }

        // クロール完走を待ってから後始末（unique 制約を汚さない）
        crawl.await.unwrap().unwrap();
        sqlx::query("DELETE FROM articles WHERE feed_id = $1")
            .bind(feed_id.0)
            .execute(&db)
            .await
            .unwrap();
        super::delete_feed(&state, feed_id).await.unwrap();

        assert_eq!(
            title.as_deref(),
            Some("title-before-extract"),
            "feed title was not set before auto extraction completed"
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
