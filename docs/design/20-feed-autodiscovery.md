# 20 フィード自動検出（Feed Autodiscovery）

> 読み手向けメモ: このドキュメントは「リポジトリは手元にあるが、この会話の文脈を知らない別セッションの実装者」が、これ1枚で着手・完了できる粒度で書いている。裏取りした実ファイル（このセッションで実際に開いて確認した）: `backend/src/features/feeds/{domain,repository,service,handler,mod}.rs`, `backend/src/features/feed_overview/*`（読み取り専用スライスの前例）, `backend/src/features/instapaper/{domain,service}.rs`（外部 HTTP・値オブジェクト・ステータス分類の前例）, `backend/src/features/mod.rs`, `backend/src/shared/{state,config,error}.rs`, `backend/Cargo.toml`（crate 名 `rss-reader-backend` / `reqwest` は `default-features=false` + `json,rustls-tls,gzip,brotli` / `feed-rs = "2"` 既存 / HTML パーサは未導入）, `backend/migrations/`（最新 = `0005_search.sql`）, `frontend/src/lib/api.ts`, `frontend/src/components/layout/{AddFeedDialog,SidebarContent}.tsx`, `frontend/src/components/ui/{button,input,dialog,badge}.tsx`, `scripts/test/`（`api-feeds.sh` / `api-instapaper.sh` などの HTTP 結合テスト群）。

## 1. 概要

フィードの **URL を直接知らなくても**、サイトのトップページや記事ページの URL を貼るだけで購読できるようにする「フィード自動検出」機能。

ユーザーが `https://example.com/blog` のような **HTML ページの URL** を入力すると、バックエンドがそのページを取得して `<head>` 内の `<link rel="alternate" type="application/rss+xml" href="...">`（および Atom / JSON Feed）を解析し、**検出されたフィード候補の一覧**を返す。フロントは候補を一覧表示し、ユーザーが選んだものを **既存の `POST /api/feeds`** で購読する。入力 URL 自体がフィード（`application/rss+xml` 等）だった場合は、それを1件の候補として返す（自己検出）。

設計上のキモは2点:

- **新スライス1枚で閉じる**: 検出は `backend/src/features/feed_discovery/` に縦割りで新設し、`POST /api/feeds/discover` 1本を提供する。**`feeds` スライスは一切編集しない**。購読そのものは既存 `POST /api/feeds` を使う（検出と購読を分離）。`/api/feeds/discover` は静的セグメントなので `feeds` の `/api/feeds/{id}` 等と衝突しない（§5.1）。これは `feed_overview`（`GET /api/feeds/overview`）が同じ `/api/feeds` プレフィックスに別スライスとして相乗りしている前例どおり。
- **解析ロジックは純粋関数に切り出してテストする**: HTML→候補抽出（`extract_feed_links`）、MIME→種別判定（`feed_kind_from_type` / `is_feed_content_type`）を `domain.rs` の純粋関数にし、ネットワーク無しで `#[cfg(test)]` 単体テストする（`FeedUrl::parse` / `classify_add_status` と同じ流儀）。

> **AI（LLM）は使わない**: 本機能は HTML の `<link>` を機械的に解析するだけで、要約・翻訳のような Claude 呼び出しは不要。したがって `shared/llm` 境界・`ANTHROPIC_API_KEY`・`AppError::NotEnabled` は本スライスでは登場しない（この旨を §2 で明記し、レビュー時の「AI 機能はキャッシュ＋NotEnabled に従っているか」の観点が本機能には非該当であることを確定させる）。

## 2. スコープ / 非スコープ

**含む（このチケットでやる）**

- 新スライス `backend/src/features/feed_discovery/`（`domain.rs` / `repository.rs` / `service.rs` / `handler.rs` / `mod.rs`）。
- `POST /api/feeds/discover` 1本。リクエスト `{ "url": "<page url>" }` → レスポンス `{ "candidates": DiscoveredFeed[] }`。
- HTML から `<link rel="alternate" type="application/rss+xml|application/atom+xml|application/feed+json|application/json" href="...">` を抽出し、`href` を**最終 URL（リダイレクト後）基準で絶対化**して返す純粋関数 + 単体テスト。
- 入力 URL 自体がフィード（content-type がフィード MIME）のときの**自己検出**（`feed-rs` で `<title>` も拾う）。
- 既存購読フィードとの突合で各候補に `already_subscribed: bool` を付与（候補 UI で「購読済み」を出すため。読み取り専用のクロスクエリ）。
- 入力 URL の値オブジェクト `DiscoverUrl::parse`（http/https のみ・trim）。不正は `AppError::Validation`(400)。
- 防御的な上限（ボディ最大 5 MiB・リクエストタイムアウト 10s）。
- フロント: `lib/api.ts` に `DiscoveredFeed` 型と `discoverFeeds(url)` を追加。`components/layout/AddFeedDialog.tsx` に「検出 → 候補選択 → 購読」フローを追加（既存の「URL 直接追加」も残す）。
- 結合テスト `scripts/test/api-feeds-discover.sh`（契約・異常系を HTTP で検証）。
- 依存追加: HTML パーサ `scraper`（§4 / §11 で根拠と代替を明記）。

**含まない（別チケット / 別機能）**

- **購読そのもの**（候補の永続化）。既存 `POST /api/feeds`（`feeds` スライス）をそのまま使う。本スライスは候補を返すだけで feeds テーブルへ書き込まない。
- **well-known パス推測**（`/feed`・`/rss`・`/atom.xml`・`/feed.json` を当てに行く総当たり）。`<link rel=alternate>` を持たないサイト向けの将来拡張（§11）。本チケットは `<link>` 解析 + 自己検出に閉じる。
- **OPML 一括インポート / 検出結果のサーバ側キャッシュ**（別機能。キャッシュが要るなら将来 `0006` 以降で。§4）。
- **AI/LLM 連携**（§1 のとおり非該当）。
- フィードのリネーム / フォルダ割当 / per-feed refresh（機能01・`feeds` スライス）。本スライスは触らない。
- マイグレーション（DB 変更なし。§4）。

## 3. 既存実装の再利用

**車輪の再発明をしないため、以下を再利用する。** いずれも本セッションで実ファイルを開いて確認済み。

- **`feed_overview` スライスが「読み取り専用・新スライス・`/api/feeds` プレフィックス相乗り」の前例**（`backend/src/features/feed_overview/mod.rs` の `Router::new().route("/api/feeds/overview", get(...))`）。本スライスも同形で `POST /api/feeds/discover` を1本生やし、`features/mod.rs` に `.merge()` 1行を足すだけ。`feeds` スライスは不編集。
- **`instapaper/service.rs` の外部 HTTP 呼び出しパターン**: `state.http.get(...).send().await.map_err(|e| AppError::Upstream(e.to_string()))?.error_for_status().map_err(...)?` という reqwest → `AppError::Upstream`(502) への畳み込みを踏襲（`feeds/service.rs::fetch_and_store` も同型）。新しい HTTP クライアントは作らず `state.http`（`reqwest::Client`）を使い回す。
- **`feeds/service.rs::fetch_and_store` の `feed_rs::parser::parse(&bytes[..])`**: 自己検出時の `<title>` 取得にそのまま使う（`feed-rs` は既存依存。新規追加なし）。
- **値オブジェクトの流儀**（`feeds/domain.rs::FeedUrl::parse` / `instapaper/domain.rs::SaveUrl::parse`）: http/https チェック + trim の `DiscoverUrl` をスライス内に閉じて新設（スライス越境の型結合を避けるため `FeedUrl` を import しない、`SaveUrl` と同方針）。
- **純粋関数 + `#[cfg(test)]` のテスト流儀**（`instapaper/domain.rs::classify_add_status` 群、`feeds/domain.rs::FeedUrl::parse` 群）。本スライスの解析ロジックも純粋関数化して同じ場所でテストする。
- **`shared/error.rs::AppError` + `AppResult`**: `Validation`(400) / `Upstream`(502) / `Database`(500, `sqlx::Error` から `#[from]`) のみ使う。**新バリアントは追加しない**（`shared/error.rs` 不編集）。
- **`feeds/repository.rs` の生 sqlx クエリ流儀**: `already_subscribed` 用の `SELECT url FROM feeds` も runtime クエリ（`query_as` / `query!` マクロは使わない）。これは `feed_overview` が `feeds`/`articles` を直接読むのと同じ CQRS-lite 読み取りであり、禁止される「越境共通レイヤー」ではない。
- **フロント `lib/api.ts` の `http<T>()` ヘルパ**（非2xx は `Error("<status> ...")` を throw、`errorStatus()` で status 抽出）。`addFeed` と同型の POST メソッドを1つ足すだけ。**購読は既存 `api.addFeed(url)` を再利用**。
- **`AddFeedDialog.tsx`（機能08 の成果物）**: 「フィードを追加」ダイアログが既にサイドバー下部にある（`SidebarContent.tsx` でマウント）。ここに検出フローを足すのが最も自然な置き場所。`Button` / `Input` / `Dialog` / `Badge`（`components/ui/`）をそのまま使う。
- **結合テストの実慣習**: `scripts/test/api-*.sh`（起動済みスタックへ HTTP を投げ、`jq` で assert）。`api-instapaper.sh`（資格情報未設定時の挙動）や `api-feeds.sh` を雛形にする。`backend/tests/` ディレクトリは**現状存在しない**（本 crate はバイナリ専用で library target が無く、`tests/*.rs` から内部 fn を呼べない）。純粋ロジックは `#[cfg(test)]`、HTTP 契約は `scripts/test/*.sh` の2方式に従う。

## 4. データモデルとマイグレーション

**DB 変更なし（マイグレーション追加なし）。**

理由: 検出はステートレスな読み取り操作で、フィード本体の HTML を取得→解析して候補を返すだけ。永続化が必要なのは「候補をユーザーが選んで購読したフィード」だが、それは既存 `POST /api/feeds`（`feeds` テーブル・`0001_init.sql`）が担う。`already_subscribed` の付与も `feeds.url` を**読むだけ**で、新テーブル・新カラムは不要。

> **マイグレーション番号についての注記（着手前に必ず確認）**: 本機能は新規マイグレーションを使わないが、リポジトリの最新は `backend/migrations/0005_search.sql` である。将来、検出結果のキャッシュ（例: `feed_discovery_cache(page_url, discovered_at, candidates_json)`）や well-known パス推測のヒット記録を持たせる拡張に踏み込む場合は、**空き番号 `0006` 以降**で新規ファイルを追加する。**着手直前に `ls backend/migrations/` で最新番号を再確認**し、`apalis` 移行など並行タスクと番号が衝突しないようにすること（マイグレーションは追記のみ・既存ファイルは不編集が鉄則）。本チケットの範囲ではいずれも不要。

## 5. バックエンド設計

新スライス `backend/src/features/feed_discovery/`。**書き込みなし・読み取り専用**（HTML を外部取得し、`feeds.url` を読むのみ）。新 trait / dyn は追加しない。

### 5.1 ルート設計と衝突回避

`POST /api/feeds/discover` は静的セグメント `discover`。`feeds` スライスは実コードで `GET/POST /api/feeds`、`DELETE/PATCH /api/feeds/{id}`、`POST /api/feeds/{id}/refresh` を持ち、`feed_overview` が `GET /api/feeds/overview` を持つ（いずれも確認済み）。本スライスの `POST /api/feeds/discover` は **method+path がどれとも重複しない**。axum 0.8（matchit 0.8）は静的セグメント `discover` を動的 `{id}` より優先してマッチするため、仮に将来 `POST /api/feeds/{id}` が足されても `discover` が先にマッチする。複数スライスが同一プレフィックス `/api/feeds` に `.merge()` するのは結合ではない（`feed_overview` の前例どおり）。

### 5.2 `domain.rs`

```rust
use reqwest::Url; // reqwest が url クレートを再エクスポート（追加依存なしで使える）
use serde::Serialize;

/// 検出されたフィード候補1件（API レスポンス要素）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveredFeed {
    /// 絶対 URL に解決済みのフィード URL（購読時はこれを既存 POST /api/feeds に渡す）。
    pub url: String,
    /// <link title="..."> もしくはフィード <title> 由来の表示名。無ければ None。
    pub title: Option<String>,
    /// フィード種別（type 属性 / content-type 由来）。
    pub kind: FeedKind,
    /// 既に購読済みなら true（候補 UI で「購読済み」バッジを出すため）。service 層で付与。
    pub already_subscribed: bool,
}

/// フィード種別。serde で小文字文字列（"rss"/"atom"/"json"/"unknown"）にシリアライズされ、
/// フロントの DiscoveredFeed["kind"] と一致する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedKind {
    Rss,
    Atom,
    Json,
    Unknown,
}

/// 入力 URL の値オブジェクト。FeedUrl と同じスキーム検査だが、スライス越境の型結合を
/// 避けるため feed_discovery 内に閉じる（instapaper の SaveUrl と同じ方針）。
#[derive(Debug, Clone)]
pub struct DiscoverUrl(String);

impl DiscoverUrl {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into();
        let t = s.trim();
        if !(t.starts_with("http://") || t.starts_with("https://")) {
            return Err("url must start with http:// or https://".into());
        }
        Ok(Self(t.to_string()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// type 属性 / content-type の MIME から種別を判定（純粋関数）。
/// 先頭の MIME 部分のみ見る（`; charset=utf-8` などのパラメタを許容）。
/// 汎用 XML（application/xml・text/xml）は「フィードかもしれないが種別不明」として Unknown。
pub fn feed_kind_from_type(type_attr: &str) -> FeedKind {
    match mime_of(type_attr).as_str() {
        "application/rss+xml" => FeedKind::Rss,
        "application/atom+xml" => FeedKind::Atom,
        "application/feed+json" | "application/json" => FeedKind::Json,
        _ => FeedKind::Unknown,
    }
}

/// content-type が「フィード本体（HTML ではない）」を示すか（純粋関数）。
/// 入力 URL 自体がフィードのときの自己検出に使う。
/// application/json 単体はフィードとは限らない（API 応答の可能性）ため自己検出には含めない。
pub fn is_feed_content_type(content_type: &str) -> bool {
    matches!(
        mime_of(content_type).as_str(),
        "application/rss+xml"
            | "application/atom+xml"
            | "application/feed+json"
            | "application/xml"
            | "text/xml"
    )
}

fn mime_of(s: &str) -> String {
    s.split(';').next().unwrap_or("").trim().to_ascii_lowercase()
}

/// HTML から <link rel="alternate" type="<feed mime>" href="..."> を抽出し、
/// href を base（最終取得 URL）に対して絶対 URL へ解決して返す。
/// **純粋関数・ネットワーク不要**（base は呼び出し側が決めた URL を渡す）。
/// - rel トークンに "alternate" を含む（大文字小文字無視・空白区切り複数トークン対応）。
/// - type がフィード MIME（rss / atom / json feed）に解決できるものだけ採用。
/// - href が空 / 解決不能なものはスキップ。
/// - 同一絶対 URL は最初の1件だけ残して重複排除。
pub fn extract_feed_links(html: &str, base: &Url) -> Vec<DiscoveredFeed> {
    use scraper::{Html, Selector};
    use std::collections::HashSet;

    let doc = Html::parse_document(html);
    let sel = Selector::parse("link").expect("static 'link' selector is valid");
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<DiscoveredFeed> = Vec::new();

    for el in doc.select(&sel) {
        let rel = el.value().attr("rel").unwrap_or_default();
        let is_alternate = rel
            .split_whitespace()
            .any(|t| t.eq_ignore_ascii_case("alternate"));
        if !is_alternate {
            continue;
        }
        let kind = feed_kind_from_type(el.value().attr("type").unwrap_or_default());
        if matches!(kind, FeedKind::Unknown) {
            continue; // application/xhtml+xml などのフィード以外の alternate を除外
        }
        let href = el.value().attr("href").unwrap_or_default().trim();
        if href.is_empty() {
            continue;
        }
        let Ok(abs) = base.join(href) else {
            continue; // 解決不能な href はスキップ
        };
        let url = abs.to_string();
        if !seen.insert(url.clone()) {
            continue; // 重複排除
        }
        let title = el
            .value()
            .attr("title")
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        out.push(DiscoveredFeed {
            url,
            title,
            kind,
            already_subscribed: false, // service 層で付与
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/blog/").unwrap()
    }

    // ---- DiscoverUrl ----
    #[test]
    fn discover_url_accepts_http_and_https() {
        assert!(DiscoverUrl::parse("http://example.com").is_ok());
        assert!(DiscoverUrl::parse("https://example.com/blog").is_ok());
    }
    #[test]
    fn discover_url_trims_and_rejects_missing_scheme() {
        assert_eq!(
            DiscoverUrl::parse("  https://example.com  ").unwrap().as_str(),
            "https://example.com"
        );
        assert!(DiscoverUrl::parse("example.com").is_err());
        assert!(DiscoverUrl::parse("").is_err());
    }

    // ---- feed_kind_from_type ----
    #[test]
    fn feed_kind_maps_known_mimes() {
        assert_eq!(feed_kind_from_type("application/rss+xml"), FeedKind::Rss);
        assert_eq!(feed_kind_from_type("application/atom+xml"), FeedKind::Atom);
        assert_eq!(feed_kind_from_type("application/feed+json"), FeedKind::Json);
        assert_eq!(feed_kind_from_type("application/json"), FeedKind::Json);
    }
    #[test]
    fn feed_kind_ignores_charset_param_and_case() {
        assert_eq!(
            feed_kind_from_type("Application/RSS+XML; charset=utf-8"),
            FeedKind::Rss
        );
    }
    #[test]
    fn feed_kind_unknown_for_non_feed() {
        assert_eq!(feed_kind_from_type("text/html"), FeedKind::Unknown);
        assert_eq!(feed_kind_from_type("application/xml"), FeedKind::Unknown);
        assert_eq!(feed_kind_from_type(""), FeedKind::Unknown);
    }

    // ---- is_feed_content_type ----
    #[test]
    fn is_feed_content_type_true_for_feed_mimes() {
        assert!(is_feed_content_type("application/rss+xml; charset=utf-8"));
        assert!(is_feed_content_type("application/atom+xml"));
        assert!(is_feed_content_type("text/xml"));
        assert!(is_feed_content_type("application/feed+json"));
    }
    #[test]
    fn is_feed_content_type_false_for_html_and_bare_json() {
        assert!(!is_feed_content_type("text/html"));
        assert!(!is_feed_content_type("application/json")); // API 応答かもしれないので自己検出には含めない
        assert!(!is_feed_content_type(""));
    }

    // ---- extract_feed_links ----
    #[test]
    fn extracts_rss_and_atom_links() {
        let html = r#"<html><head>
            <link rel="alternate" type="application/rss+xml" title="RSS" href="https://example.com/rss.xml">
            <link rel="alternate" type="application/atom+xml" href="https://example.com/atom.xml">
        </head></html>"#;
        let got = extract_feed_links(html, &base());
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].url, "https://example.com/rss.xml");
        assert_eq!(got[0].kind, FeedKind::Rss);
        assert_eq!(got[0].title.as_deref(), Some("RSS"));
        assert_eq!(got[1].kind, FeedKind::Atom);
        assert_eq!(got[1].title, None);
    }
    #[test]
    fn resolves_relative_href_against_base() {
        let html = r#"<head><link rel="alternate" type="application/rss+xml" href="../feed.xml"></head>"#;
        let got = extract_feed_links(html, &base()); // base = https://example.com/blog/
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].url, "https://example.com/feed.xml");
    }
    #[test]
    fn ignores_non_feed_alternate() {
        let html = r#"<head>
            <link rel="alternate" type="application/xhtml+xml" href="/m.html">
            <link rel="stylesheet" href="/style.css">
            <link rel="alternate" hreflang="en" href="/en/">
        </head>"#;
        assert!(extract_feed_links(html, &base()).is_empty());
    }
    #[test]
    fn dedups_same_resolved_url() {
        let html = r#"<head>
            <link rel="alternate" type="application/rss+xml" href="/rss.xml">
            <link rel="alternate" type="application/rss+xml" href="https://example.com/rss.xml">
        </head>"#;
        assert_eq!(extract_feed_links(html, &base()).len(), 1);
    }
    #[test]
    fn handles_multi_token_rel_and_skips_empty_href() {
        let html = r#"<head>
            <link rel="alternate home" type="application/rss+xml" href="/a.xml">
            <link rel="alternate" type="application/rss+xml" href="">
        </head>"#;
        let got = extract_feed_links(html, &base());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].url, "https://example.com/a.xml");
    }
    #[test]
    fn empty_when_no_link_tags() {
        assert!(extract_feed_links("<html><body>no head links</body></html>", &base()).is_empty());
    }
}
```

> clippy 注意: `extract_feed_links` 内の `Selector::parse(...).expect(...)` は静的な正当セレクタなので panic しない。`just lint`（`cargo clippy --all-targets -- -D warnings`、既定 lint）で警告にならない。`reqwest::Url` は reqwest が `url` クレートを再エクスポートしているため、`Cargo.toml` に `url` を直接足す必要はない（`use reqwest::Url;`）。

### 5.3 `repository.rs`

```rust
use std::collections::HashSet;

use sqlx::PgPool;

use crate::shared::error::AppResult;

/// 既存購読フィードの URL 集合（候補へ already_subscribed を付けるための読み取り専用）。
/// feed_overview が feeds/articles を直接読むのと同じ CQRS-lite 読み取り。runtime クエリのみ。
pub async fn existing_feed_urls(pool: &PgPool) -> AppResult<HashSet<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT url FROM feeds")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(u,)| u).collect())
}
```

設計ノート: `query_as::<_, (String,)>` の runtime クエリ（`query!` マクロ不使用、CLAUDE.md）。突合は**正規化なしの文字列完全一致**で行う（後述 §11 の既知の限界。`/feed` と `/feed/` の差異等は別物扱いになりうる）。

### 5.4 `service.rs`

```rust
use std::time::Duration;

use feed_rs::parser;

use super::domain::{
    extract_feed_links, feed_kind_from_type, is_feed_content_type, DiscoverUrl, DiscoveredFeed,
};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// 取得ボディの上限（防御的）。巨大ページ / 誤って巨大ファイルを掴んでもメモリを守る。
const MAX_BODY_BYTES: usize = 5 * 1024 * 1024; // 5 MiB
/// 1 回の取得タイムアウト（共有クライアントに既定が無くても確実に打ち切る）。
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// 入力 URL を取得し、フィード候補を返す。書き込みなし。
/// - content-type がフィードなら入力 URL 自体を1候補として返す（自己検出, feed-rs で title 取得）。
/// - そうでなければ HTML として <link rel=alternate> を解析して候補化。
/// - 既存購読フィードと突合して already_subscribed を付与。
pub async fn discover(state: &AppState, raw_url: &str) -> AppResult<Vec<DiscoveredFeed>> {
    let input = DiscoverUrl::parse(raw_url).map_err(AppError::Validation)?;

    let resp = state
        .http
        .get(input.as_str())
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::Upstream(e.to_string()))?;

    // リダイレクト後の最終 URL。相対 href 解決の base に使う。
    let base = resp.url().clone();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = resp
        .bytes()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?;
    let body = if body.len() > MAX_BODY_BYTES {
        body.slice(0..MAX_BODY_BYTES)
    } else {
        body
    };

    let mut candidates = if is_feed_content_type(&content_type) {
        // 入力 URL 自体がフィード。feed-rs で <title> を拾う（失敗しても URL は返す）。
        let title = parser::parse(&body[..])
            .ok()
            .and_then(|f| f.title.map(|t| t.content));
        vec![DiscoveredFeed {
            url: base.to_string(),
            title,
            kind: feed_kind_from_type(&content_type),
            already_subscribed: false,
        }]
    } else {
        // HTML として解析。href/type は ASCII なので非 UTF-8 ページでも from_utf8_lossy で十分。
        let html = String::from_utf8_lossy(&body);
        extract_feed_links(&html, &base)
    };

    // already_subscribed を付与（読み取り専用クロスクエリ）。
    let existing = repository::existing_feed_urls(&state.db).await?;
    for c in candidates.iter_mut() {
        c.already_subscribed = existing.contains(&c.url);
    }

    Ok(candidates)
}
```

設計ノート:
- 候補ゼロ件は**エラーにせず空 Vec**（200 で `{"candidates": []}`）。フロントは「候補が見つかりませんでした」を表示。
- `resp.url()`（reqwest `Response::url`）はリダイレクト追従後の最終 URL。reqwest 既定はリダイレクトを最大10回追従するので、最終ページ基準で相対 href を解決できる。
- `is_feed_content_type` は `feed_kind_from_type` が Unknown を返す `application/xml`/`text/xml` も自己検出対象に含める（汎用 XML フィード対応）。その場合 `kind = Unknown` になるが URL は返す。
- `application/json` 単体は自己検出に含めない（API 応答誤検出を避ける）。HTML 内の `<link type="application/json">`（JSON Feed）は `extract_feed_links` 側で拾う。

### 5.5 `handler.rs`

```rust
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::domain::DiscoveredFeed;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub candidates: Vec<DiscoveredFeed>,
}

pub async fn discover(
    State(state): State<AppState>,
    Json(body): Json<DiscoverRequest>,
) -> AppResult<Json<DiscoverResponse>> {
    let candidates = service::discover(&state, &body.url).await?;
    Ok(Json(DiscoverResponse { candidates }))
}
```

> `Json<DiscoverRequest>` は `url` キーが欠落 / ボディが不正 JSON のとき axum の抽出リジェクトで **422 Unprocessable Entity** を返す（`AppError` を通らない）。`url` はあるがスキーム不正のときは `service` 内の `DiscoverUrl::parse` が `AppError::Validation` → **400**。この差は §9 のテストで明示する。

### 5.6 `mod.rs`

```rust
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::post;
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/feeds/discover", post(handler::discover))
}
```

### 5.7 `features/mod.rs` への合成（追加2行のみ）

`backend/src/features/mod.rs` は現在 `articles / feed_overview / feeds / folders / health / instapaper / search / stats` の8枚を `.merge()` している。**2行足すだけ**:

```rust
pub mod feed_discovery; // ← 追加（アルファベット順で feed_overview の前あたり）
// ...
        .merge(feeds::routes())
        .merge(feed_discovery::routes()) // ← 追加（feeds の隣でよい）
```

既存スライス（feeds / feed_overview / articles / …）には一切手を入れない。

### 5.8 `Cargo.toml`（依存追加）

HTML パースのため `scraper`（html5ever ベースの定番クローラ向けパーサ）を追加する。`reqwest`/`feed-rs` 系の隣に1行:

```toml
# HTML parsing for feed autodiscovery (<link rel="alternate">)
scraper = "0.20"   # 着手時に crates.io で最新安定版を確認して採用すること
```

- バージョンは執筆時点の目安。**実装時に最新安定版を確認**（`cargo add scraper` でよい）。
- `scraper` は `html5ever` + `selectors` を引き込むため依存はやや重いが、HTML を正しく（壊れたマークアップ込みで）パースするには妥当な選択。手書きの正規表現スキャン（`regex` も未導入のため新依存になる）より堅牢で、純粋関数として単体テストしやすい。代替案は §11。
- `scraper` 利用は `domain.rs::extract_feed_links` 内に閉じる（他スライスへ波及させない）。

### 5.9 AppError の使い分け

- 候補ゼロ件は **`NotFound` を返さない**（空配列で 200）。
- 入力 URL のスキーム不正 → `AppError::Validation`（400）。
- 取得失敗（DNS / 接続不可 / 非2xx / タイムアウト）→ `AppError::Upstream`（502）。
- `feeds.url` 読み取りの DB エラー → `sqlx::Error` → `AppError::Database`（500、`#[from]` で自動・`?` 伝播）。
- **`NotEnabled` は使わない**（LLM も外部資格情報も不要な機能。§1）。新バリアント追加なし（`shared/error.rs` 不編集）。

## 6. フロントエンド設計

> 方針: 検出 UI は既存の **`AddFeedDialog.tsx`（機能08）に同居**させる。ダイアログに「検出」ボタンを足し、サイト URL からフィード候補を取得 → 一覧表示 → ユーザーが候補を選んで購読（既存 `api.addFeed`）。「URL を直接入力して追加」の従来動線も残す。新しいグローバル状態は不要（ローカル signal）。装飾は意味トークン（`text-muted-foreground` 等）のみ、`components/ui/` を再利用。

### 6.1 `lib/api.ts`（型 + メソッド追加）

```ts
export interface DiscoveredFeed {
  url: string;
  title: string | null;
  kind: "rss" | "atom" | "json" | "unknown";
  already_subscribed: boolean;
}

// api オブジェクト内に追加（addFeed の隣）。
// サイト URL からフィード候補を検出するだけ（購読はしない。選択後に addFeed を呼ぶ）。
discoverFeeds: (url: string) =>
  http<{ candidates: DiscoveredFeed[] }>("/api/feeds/discover", {
    method: "POST",
    body: JSON.stringify({ url }),
  }),
```

- 命名は既存規約「動詞 + リソース camelCase」（`listFeeds` / `addFeed`）に揃え `discoverFeeds`。
- 購読は既存 `api.addFeed(url)` をそのまま使う（重複実装しない）。

### 6.2 `components/layout/AddFeedDialog.tsx`（検出フロー追加・全文）

既存ファイルを以下に置き換える（従来の「直接追加」も維持）。`Badge` を新規 import（`components/ui/badge.tsx` は既存）。

```tsx
import { createSignal, For, Show } from "solid-js";
import { useApp } from "@/lib/store";
import { api, type DiscoveredFeed } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";

/**
 * フィード追加（機能08）＋ 自動検出（機能20）。
 * - 「検出」: サイト/記事 URL から POST /api/feeds/discover で候補を取得し一覧表示。
 *   候補を選ぶと既存 api.addFeed(url) で購読。
 * - 「追加」: 入力がフィード URL そのものなら従来どおり直接購読。
 */
export function AddFeedDialog() {
  const app = useApp();
  const [url, setUrl] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [open, setOpen] = createSignal(false);
  const [candidates, setCandidates] = createSignal<DiscoveredFeed[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  const reset = () => {
    setUrl("");
    setCandidates(null);
    setError(null);
  };

  const discover = async () => {
    const v = url().trim();
    if (!v) return;
    setBusy(true);
    setError(null);
    setCandidates(null);
    try {
      const res = await api.discoverFeeds(v);
      setCandidates(res.candidates);
      if (res.candidates.length === 0) {
        setError("このページからフィードを検出できませんでした。");
      }
    } catch (e) {
      setError(`検出に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const addDirect = async () => {
    const v = url().trim();
    if (!v) return;
    setBusy(true);
    try {
      await api.addFeed(v);
      app.refetchFeeds();
      reset();
      setOpen(false);
    } catch (e) {
      setError(`追加に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const subscribe = async (c: DiscoveredFeed) => {
    setBusy(true);
    try {
      await api.addFeed(c.url);
      app.refetchFeeds();
      // 購読済みに更新（候補リストはそのまま残し、当該行だけ印を付ける）。
      setCandidates((cs) =>
        (cs ?? []).map((x) => (x.url === c.url ? { ...x, already_subscribed: true } : x)),
      );
    } catch (e) {
      setError(`購読に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Button size="sm" class="w-full" onClick={() => setOpen(true)}>
        フィードを追加
      </Button>
      <Dialog
        open={open()}
        onOpenChange={(d) => {
          setOpen(d.open);
          if (!d.open) reset();
        }}
      >
        <DialogContent>
          <DialogTitle>フィードを追加</DialogTitle>
          <div class="mt-4 space-y-3">
            <Input
              placeholder="サイトURL または フィードURL"
              value={url()}
              onInput={(e) => setUrl(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && discover()}
            />
            <div class="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setOpen(false)}>
                キャンセル
              </Button>
              <Button variant="outline" onClick={addDirect} disabled={busy()}>
                直接追加
              </Button>
              <Button onClick={discover} disabled={busy()}>
                {busy() ? "検出中…" : "検出"}
              </Button>
            </div>

            <Show when={error()}>
              <p class="text-sm text-destructive">{error()}</p>
            </Show>

            <Show when={(candidates()?.length ?? 0) > 0}>
              <ul class="divide-y divide-border border-t border-border">
                <For each={candidates()!}>
                  {(c) => (
                    <li class="flex items-center justify-between gap-3 py-2">
                      <div class="min-w-0">
                        <p class="truncate text-sm font-medium">
                          {c.title ?? c.url}
                        </p>
                        <p class="truncate text-xs text-muted-foreground">
                          {c.kind.toUpperCase()} ・ {c.url}
                        </p>
                      </div>
                      <Show
                        when={!c.already_subscribed}
                        fallback={<Badge variant="secondary">購読済み</Badge>}
                      >
                        <Button
                          size="sm"
                          onClick={() => subscribe(c)}
                          disabled={busy()}
                        >
                          購読
                        </Button>
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
```

ポイント:
- `import { createSignal, For, Show } from "solid-js"` を明記（コピペ即実装）。`@/` エイリアス・既存 `api`/`useApp`/`Button`/`Input`/`Badge`/`Dialog` を再利用。
- 検出（`discover`）と購読（`subscribe`）を分離。購読は**既存 `api.addFeed(c.url)`** を呼ぶだけ（本機能はフィードを書き込まない）。
- 購読成功時は `app.refetchFeeds()`（既存ストア API）でサイドバーのフィード一覧を更新。当該候補行は `already_subscribed: true` に更新して「購読済み」バッジへ切替。
- `Badge` の `variant` 名（`secondary` 等）は `components/ui/badge.tsx` の実装に合わせる。存在しない variant なら省略 or 既存の variant に合わせること（実装時に1度確認）。
- 装飾は意味トークン（`text-muted-foreground` / `text-destructive` / `divide-border`）のみ。生 hex・新色は持ち込まない。長い title/url は `min-w-0 truncate` で破綻防止。

### 6.3 状態管理・配置

- **グローバル状態（`store.tsx`）の変更は不要**。検出結果・入力・busy・error はすべて `AddFeedDialog` ローカルの `createSignal` に閉じる。購読後の一覧更新は既存 `app.refetchFeeds()` を使う。
- 配置は既存どおり `SidebarContent.tsx` の `<AddFeedDialog />`（変更不要）。新ルート・新コンポーネントファイルは追加しない（既存ダイアログの拡張に閉じる）。

## 7. API 契約

### `POST /api/feeds/discover`

- 認証 / 有効化フラグ: なし（常に有効。LLM・外部資格情報に依存しない）。
- リクエストボディ:

```json
{ "url": "https://example.com/blog" }
```

- レスポンス `200 OK`（候補ゼロでも 200・空配列）:

```json
{
  "candidates": [
    {
      "url": "https://example.com/feed.xml",
      "title": "Example Blog",
      "kind": "rss",
      "already_subscribed": false
    },
    {
      "url": "https://example.com/atom.xml",
      "title": null,
      "kind": "atom",
      "already_subscribed": true
    }
  ]
}
```

- フィールド意味:
  - `url`: 絶対 URL へ解決済みのフィード URL。**購読時はこの値を既存 `POST /api/feeds` の `{ "url": ... }` に渡す**。
  - `title`: `<link title="...">`（HTML 検出時）または フィード `<title>`（自己検出時）。無ければ `null`。
  - `kind`: `"rss" | "atom" | "json" | "unknown"`。自己検出で content-type が汎用 XML のときは `"unknown"`。
  - `already_subscribed`: その URL が `feeds` テーブルに既存なら `true`（文字列完全一致）。
- エラー:
  - `400 {"error":"invalid input: url must start with http:// or https://"}` — スキーム不正（`Validation`）。
  - `422` — `url` キー欠落 / ボディが不正 JSON（axum 抽出リジェクト。`AppError` を通らない素のリジェクト）。
  - `502 {"error":"upstream request failed: ..."}` — 取得失敗（DNS・接続不可・非2xx・タイムアウト）。
  - `500 {"error":"internal error"}` — DB 障害（`already_subscribed` 突合時）。

### 購読（既存・本機能では新設しない）

- `POST /api/feeds`（`feeds` スライス）にて `{ "url": "<candidate.url>" }` → `201 Created` で `Feed` を返す。検出と購読はこの2段で完結する。

## 8. 依存関係

- **このチケットが依存する機能: なし（`dependsOn` 実質空）。**
  - バックエンドは既存 `feeds` テーブル（`0001_init.sql`）と `state.http` のみで完結。新マイグレーション無し。
  - フロントの購読は既存 `POST /api/feeds` / `api.addFeed`（機能なし＝初期実装から存在）に乗るだけ。
  - 置き場所として **機能08（`AddFeedDialog` のサイドバー配置）**が既に着地済みなのを利用する（ソフト依存。仮に `AddFeedDialog` が無くても、本機能のロジックは `lib/api.ts` + 任意のダイアログ/ページから呼べる自己完結）。
- **このチケットがブロックする / 土台になる機能**: 特になし（独立機能）。将来「well-known パス推測」や「OPML インポート」を足すなら本スライスを拡張する（§11）。
- 関係するが触れないもの: 機能02（フォルダ）。検出結果に folder は含めない（購読後に既存 `PATCH /api/feeds/{id}` でフォルダ割当する別動線）。

## 9. テスト計画（TDD）

**Red → 理解 → Green の順。書いたら必ず実行する。**

> テスト配置の前提: 本 crate はバイナリ専用（`src/lib.rs`・`[lib]` 無し）で `backend/tests/*.rs` から内部 fn を `use` できない。よって**純粋ロジックは `#[cfg(test)] mod tests`**（§5.2 に同梱・DB/ネットワーク不要）、**HTTP 契約・異常系は `scripts/test/api-feeds-discover.sh`**（起動済みスタックへ HTTP）で検証する。検出の「正常系（実 HTML から候補抽出）」は外部サイト or ローカル HTTP サーバが要りネットワーク非決定なため**結合テストでは扱わず、§5.2 の純粋関数単体テストが本体**（サンプル HTML 文字列で網羅）。

### 9.1 単体テスト（`#[cfg(test)] mod tests`、§5.2 に全文同梱・DB 不要）

`backend/src/features/feed_discovery/domain.rs` に純粋関数のテストを**先に**書く（Red）。

| テスト | 意図 |
|--------|------|
| `discover_url_accepts_http_and_https` | 入力 URL の受理（http/https） |
| `discover_url_trims_and_rejects_missing_scheme` | trim と スキーム無し/空の拒否（→ 400 の根拠） |
| `feed_kind_maps_known_mimes` | rss/atom/feed+json/json → 各 FeedKind |
| `feed_kind_ignores_charset_param_and_case` | `; charset=` と大文字を吸収 |
| `feed_kind_unknown_for_non_feed` | html / 汎用xml / 空 → Unknown |
| `is_feed_content_type_true_for_feed_mimes` | 自己検出で拾う MIME 群 |
| `is_feed_content_type_false_for_html_and_bare_json` | html / 素の json は自己検出しない |
| `extracts_rss_and_atom_links` | 複数 `<link>` の抽出・kind・title |
| `resolves_relative_href_against_base` | 相対 href の絶対化（`../feed.xml`） |
| `ignores_non_feed_alternate` | xhtml/stylesheet/hreflang を除外 |
| `dedups_same_resolved_url` | 相対/絶対が同一に解決されたら1件 |
| `handles_multi_token_rel_and_skips_empty_href` | `rel="alternate home"` 対応・空 href スキップ |
| `empty_when_no_link_tags` | `<link>` 無しは空 Vec |

実行: `cd backend && cargo test feed_discovery`（DB/ネット不要）。`just lint`（clippy `-D warnings` + `pnpm typecheck`）も通す。

### 9.2 結合テスト（`scripts/test/api-feeds-discover.sh`、新規・契約と異常系）

`api-instapaper.sh` / `api-feeds.sh` を雛形に、起動済みスタックへ HTTP を投げる。**正常系の HTML 解析は §9.1 の単体テストが担保**するため、ここでは「エンドポイントの存在」「Validation(400)」「Upstream(502)」「422」を検証する（ネットワーク非決定性を持ち込まない）。

```bash
#!/usr/bin/env bash
# Contract/error tests for feed autodiscovery (POST /api/feeds/discover).
# 正常系の HTML 解析は backend の #[cfg(test)] 単体テストが担保。ここでは契約と異常系のみ。
# Requires: running stack (nginx :8081), curl, jq.
set -uo pipefail

BASE="${BASE:-http://localhost:8081}"
URL="$BASE/api/feeds/discover"

fail() { echo "FAIL: $1"; exit 1; }

# 1) スキーム不正 → 400 (AppError::Validation)
code="$(curl -s -m 10 -o /dev/null -w '%{http_code}' \
  -H 'Content-Type: application/json' \
  -d '{"url":"not-a-url"}' "$URL")"
[ "$code" = "400" ] || fail "invalid scheme: expected 400, got $code"

# 2) url キー欠落 → 422 (axum JSON 抽出リジェクト)
code="$(curl -s -m 10 -o /dev/null -w '%{http_code}' \
  -H 'Content-Type: application/json' \
  -d '{}' "$URL")"
[ "$code" = "422" ] || fail "missing url: expected 422, got $code"

# 3) 到達不能ホスト → 502 (AppError::Upstream)。127.0.0.1:1 は接続拒否される想定。
code="$(curl -s -m 15 -o /dev/null -w '%{http_code}' \
  -H 'Content-Type: application/json' \
  -d '{"url":"http://127.0.0.1:1/nope"}' "$URL")"
[ "$code" = "502" ] || fail "unreachable host: expected 502, got $code"

# 4) エンドポイントが存在する（= 404 ではない）ことの最終確認は 1) が満たす。
echo "PASS: /api/feeds/discover contract (400 invalid, 422 missing, 502 unreachable)"
```

- **Red**: 実装前は `/api/feeds/discover` が 404 → ケース1が「expected 400, got 404」で落ちる。実装後 Green。
- 環境変数 `BASE` で接続先を上書き可能。`jq` は将来の値検証用に前提に挙げるが本スクリプトでは未使用でも可。
- 到達不能ホストのステータスは環境差を避けるため `127.0.0.1:1`（予約ポート・接続拒否）を使う。タイムアウト系で 502 になることもあるが、いずれも `AppError::Upstream` で 502 に正規化される。
- 必要なら `run-all.sh` にこのスクリプトを追記（既存パターンに合わせる）。

### 9.3 手動 / フロント（型）

- `tsc` 型チェック（`just lint` の `pnpm typecheck`）で `DiscoveredFeed` 型・`discoverFeeds()`・`AddFeedDialog.tsx` の整合を確認。
- 手動シナリオ:
  1. `<link rel=alternate>` を持つ実サイト（例: 著名なブログのトップ URL）を入力 → 「検出」→ 候補が出る → 「購読」→ サイドバーにフィード追加・行が「購読済み」へ。
  2. フィード URL そのもの（`*.xml`）を入力 → 「検出」で自己検出1件 or 「直接追加」で購読。
  3. フィードを持たないページ → 「このページからフィードを検出できませんでした。」表示。
  4. 既に購読済みのフィードを含むサイト → 該当候補に「購読済み」バッジ。

## 10. 実装手順（順序付きチェックリスト）

1. ブランチを切る（例 `feat/feed-autodiscovery`）。`main` 直コミットしない。
2. `backend/Cargo.toml` に `scraper`（最新安定版を確認）を追加（§5.8）。`cargo build` で取得確認。
3. `backend/src/features/feed_discovery/` を作成し5ファイルを置く:
   - `domain.rs`（§5.2。**まず `#[cfg(test)] mod tests` を書いて Red** → `DiscoverUrl` / `FeedKind` / `DiscoveredFeed` / `feed_kind_from_type` / `is_feed_content_type` / `extract_feed_links`）。
   - `repository.rs`（§5.3 `existing_feed_urls`）。
   - `service.rs`（§5.4 `discover`）。
   - `handler.rs`（§5.5）。
   - `mod.rs`（§5.6 `routes()`）。
4. `cd backend && cargo test feed_discovery` で単体テストを Green に。
5. `backend/src/features/mod.rs` に `pub mod feed_discovery;` と `.merge(feed_discovery::routes())` を1行ずつ追加（§5.7）。他スライスは触らない。
6. `cargo build` → `just lint`（`clippy -D warnings` + `pnpm typecheck`）→ `cargo fmt`。
7. スタックを起動（`just up`、または `just dev-db` + `just back`）。
8. `scripts/test/api-feeds-discover.sh` を追加（§9.2）して実行 → 400/422/502 を Green で確認。手でも `curl -d '{"url":"<実サイト>"}' http://localhost:8081/api/feeds/discover | jq` を見る。
9. フロント: `lib/api.ts` に `DiscoveredFeed` 型と `discoverFeeds()` を追加（§6.1）。
10. `components/layout/AddFeedDialog.tsx` を §6.2 の全文へ置換（検出フロー追加）。`Badge` の variant 名を `components/ui/badge.tsx` に合わせて確認。
11. `just lint`（tsc）を通し、§9.3 の手動シナリオ1〜4を目視確認。
12. ユーザーが望むタイミングでコミット（メッセージ末尾に `Co-Authored-By` 行）。**新規マイグレーションが無いこと**を最終確認。

## 11. リスク・未決事項・代替案

| 項目 | 内容 / リスク | 対処 |
|------|---------------|------|
| **SSRF（サーバ側で任意 URL 取得）** | `discover` はユーザー入力 URL をサーバから GET する。内部ネットワーク（`http://192.168.x.x`・`http://localhost`・クラウドメタデータ等）を叩かせられる。 | 既存 `feeds/service.rs::fetch_and_store` も購読 URL を同様に取得しており**リスクの姿勢は既存と同等**（家庭内 LAN・単一ユーザ前提）。本機能ではタイムアウト10s・ボディ5MiB上限で被害を限定。LAN 外公開時はプライベート IP 拒否のアロー/ブロックリスト導入を別チケットで検討。`features/mod.rs` の `CorsLayer::permissive()` 同様、公開前に締める方針と整合。 |
| **HTML パーサ依存（scraper）** | `scraper`（html5ever 系）は依存がやや重い。 | フィード検出には壊れた HTML も正しく扱える本格パーサが妥当。**代替案**: (a) `lol_html` で `<link>` だけストリーム抽出（軽量だが API が低レベル）、(b) 正規表現スキャン（`regex` も新依存・壊れた属性順で誤検出しやすく非推奨）。当面 `scraper` を採用し `extract_feed_links` 内に閉じる（差し替え時の影響は1関数）。 |
| **文字エンコーディング** | Shift_JIS/EUC-JP の日本語ページを `from_utf8_lossy` で読むと `title` 等が文字化けしうる。 | `href`/`type`/`rel` は ASCII なので**候補抽出自体は正しく動く**（URL は壊れない）。化けるのは表示用 `title` のみで実害小。厳密化が要れば `<meta charset>`/`Content-Type` の charset を見て `encoding_rs` でデコードする拡張を将来検討。 |
| **`application/xml` の扱い** | フィードを `type="application/xml"` で宣言する `<link>` は `feed_kind_from_type` が Unknown を返し**抽出対象外**になる（取りこぼし）。自己検出（content-type）側では `application/xml`/`text/xml` を拾う。 | `<link>` 解析を厳格にして誤検出を避ける現方針を採用。取りこぼしが実データで問題になれば `extract_feed_links` 側でも汎用 XML を `Unknown` 種別の候補として許容するよう緩める（1関数の変更）。 |
| **`already_subscribed` の URL 突合** | `feeds.url` との**文字列完全一致**。`/feed` と `/feed/`、`http`↔`https`、クエリ順序などの差で「未購読」と誤表示しうる。 | 表示用フラグなので実害は小（重複購入は `POST /api/feeds` 側の `ON CONFLICT (url) DO UPDATE` で1行に収斂）。正規化が要れば突合前に末尾スラッシュ除去等の正規化関数を `domain.rs` に足す（純粋関数・テスト容易）。 |
| **候補ゼロ件 / well-known 未対応** | `<link rel=alternate>` を出さないサイトは候補ゼロになる。 | 本チケットは非スコープ（§2）。将来 `/feed`・`/rss`・`/atom.xml`・`/feed.json` を順に HEAD/GET で当てる「well-known パス推測」を `service.rs` に追加（フォールバック・並行取得）。`extract_feed_links` の結果が空のときだけ走らせる設計にすれば段階導入できる。 |
| **content-type を返さないサーバ** | content-type ヘッダ欠落だと `is_feed_content_type=false` で常に HTML 解析へ回る。実体がフィードでも `<link>` が無く候補ゼロになりうる。 | 当面許容。必要なら「HTML 解析が空 かつ ボディ先頭が `<?xml`/`<rss`/`<feed`」のときに自己検出へフォールバックする分岐を `service.rs` に足す（純粋関数 `looks_like_feed_body(&[u8])` を `domain.rs` に追加してテスト）。 |
| **リクエストタイムアウト** | 共有 `state.http` クライアントに既定タイムアウトが無いと遅いサイトでハングしうる。 | 本機能では取得ビルダに `.timeout(Duration::from_secs(10))` を明示（§5.4）。`main.rs` のクライアント既定設定には手を入れない（横断変更回避）。 |
| **重複取得（キャッシュ無し）** | 同じページを何度も検出すると毎回フェッチする。 | 単一ユーザ・手動操作なので問題なし。多用されるなら検出結果のサーバ側 TTL キャッシュ（§4 の `0006` 以降の新マイグレーション or インメモリ）を将来検討。 |
| **`Badge` variant 名** | §6.2 が使う `variant="secondary"` が `components/ui/badge.tsx` に存在しない可能性。 | 実装時に `badge.tsx` の `cva` variant を1度確認し、無ければ既存 variant か `class` 直書きに置換（見た目トークンは維持）。 |
