# 13 記事全文の抽出（readability）

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッションの実装者。本書だけで着手・完了できるよう、再利用資産・SQL・関数シグネチャ・ルート文字列・依存追加まで具体化する。
> **重要な但し書き**: 本文抽出は「フィード本文（`articles.content`）が要約・抜粋だけで、AI 要約/翻訳の品質が頭打ちになる」問題を解く土台機能である。抽出アルゴリズムは DOM ヒューリスティック（readability 系）であり 100% の精度は出ない。**抽出失敗は静かに `full_content = NULL` のまま据え置き、既存の `content` にフォールバックする**（記事閲覧・要約はこれまで通り動く）ことを全体方針とする。

---

## 1. 概要

各記事は現状、フィードが配信する `content`（多くは要約・抜粋・先頭数段落）しか持たない。本機能は記事の **元 URL を `state.http` で取得し、本文 DOM をヒューリスティックに抽出 → サニタイズ → `articles.full_content` 列にキャッシュ**する。これにより:

- **記事ビューで全文を読める**（フィードが抜粋しか流さないサイトでも本文が読める）。
- **要約・翻訳・将来の Ask など全 AI 機能の入力が `content` ではなく `full_content` を優先**するようになり、**全 AI 機能の品質上限を一段引き上げる土台**になる（★本機能の主目的）。

本機能はバックエンドに新スライス `extraction` を1枚追加し、(a) `POST /api/articles/{id}/extract`（オンデマンド抽出 + DB キャッシュ）、(b) クロール時の任意自動抽出（config フラグでオプトイン）を担う。HTTP 取得は既存の共有 `reqwest::Client`（`state.http`）を、サニタイズはフロントの `lib/sanitize.ts`（DOMPurify）の **バックエンド相当（`ammonia`）** を使う。本文抽出は新規 crate（`scraper`）の DOM パースを使い、抽出ロジックは純粋関数に切り出して TDD する。

**抽出は LLM を呼ばない**（純粋に HTTP 取得 + DOM 解析 + サニタイズ）。したがって `POST .../extract` 自体は `ANTHROPIC_API_KEY` を要求しない。**AI 機能（要約/翻訳/将来の Ask）は従来どおり `shared/llm` を再利用し、`ANTHROPIC_API_KEY` 未設定なら `AppError::NotEnabled` を返す**。本機能はその AI 機能群が読む「入力ソース」を `content` → `full_content` 優先に差し替えるだけで、AI 呼び出しの作法（`shared/llm` + DB キャッシュ + `NotEnabled`）は一切変えない（§5.6）。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）
- マイグレーション（番号は **マージ時点の最小空き整数**。最新は `0005_search.sql` なので暫定 **`0006_full_content.sql`**。**着手前に必ず `ls backend/migrations/` で最新番号を確認**し、空きが埋まっていれば繰り上げる。§4）。`articles` に `full_content TEXT` / `extracted_at TIMESTAMPTZ` の2列を **追加**（`ALTER TABLE ... ADD COLUMN`、既存マイグレーションは不編集）。
- 新スライス `backend/src/features/extraction/`（`domain` / `service` / `handler` / `mod`。**`repository.rs` は持たず `articles::repository` を再利用**。理由は §5.0）。
- オンデマンド抽出 `POST /api/articles/{id}/extract`。記事 URL をサーバ側で `articles` から引き、取得・抽出・サニタイズして `full_content`/`extracted_at` を保存。既に抽出済み（`extracted_at` 有り）なら**キャッシュを返し再取得しない**（`force=true` で強制再抽出）。
- 抽出ヒューリスティック（`<article>`/`<main>` 優先 → 本文スコアリング → ノイズタグ除去 → サニタイズ → 最小文字数チェック）を `domain.rs` の純粋関数に切り出し単体テスト。
- クロール時の任意自動抽出: `feeds/service.rs::fetch_and_store` に **config フラグ `EXTRACT_ON_CRAWL`（既定 false）でゲートした best-effort 呼び出し**を1ブロック追加（§5.7）。
- AI 入力の優先順位変更: `articles/service.rs` の `summarize_article` / `translate_article` が `full_content` を優先（無ければ `content`）に変更（§5.6）。**これが本機能の主目的**。
- 新規 crate 2つ（`scraper`、`ammonia`）を `Cargo.toml` に追加（§3 / §11）。
- `shared/config.rs` に抽出系の env を追加（`EXTRACT_ON_CRAWL` / `EXTRACT_MAX_BYTES` / `EXTRACT_MIN_CHARS`）。
- フロント: `lib/api.ts` に `Article` 型の2フィールド追加 + メソッド `extractArticle`、`routes/ArticleView.tsx`（または相当の本文表示）に「全文を取得」ボタンと全文/抜粋トグル。
- 単体テスト（抽出ロジック）+ リポジトリ往復テスト（`#[ignore]` 実 DB）+ HTTP スモークスクリプト。

### 非スコープ（本機能では実装しない）
- 「Ask（記事への質問）」機能本体。**本機能は未実装の Ask が将来 `full_content` を入力に使えるよう土台を整えるだけ**（§5.6 に拡張ポイントを明記）。
- 画像のローカル保存・リライト、AMP/PDF/動画ページの特別扱い、JavaScript レンダリング（ヘッドレスブラウザ）。MVP は静的 HTML のみ。
- サイト別の抽出ルール（custom rules / site-specific selectors）。MVP は汎用ヒューリスティックのみ。将来 `extraction` スライス内に追記可能。
- 抽出のリトライ/バックオフ・ジョブキュー化（apalis 移行の管轄）。本機能は同期 best-effort。
- `content` 列の置き換え・削除。`content`（フィード由来）は**残す**。`full_content` は別列で追加し、表示・AI 入力時に優先するだけ。

---

## 3. 既存実装の調査と再利用

実ファイルを確認済み。以下を **再利用し、車輪の再発明をしない**。

| 再利用資産 | 実体（確認済みファイル） | 本機能での使い方 |
|---|---|---|
| 共有 HTTP クライアント | `backend/src/shared/state.rs`（`AppState { db, config, http }`、`http: reqwest::Client`）。`feeds/service.rs::fetch_and_store` が `state.http.get(url).send().await.map_err(\|e\| AppError::Upstream(...))?.error_for_status()...?.bytes()` で取得している | 記事 URL の取得を**同型**で書く。新規 Client は作らない |
| reqwest 直叩き + `Upstream` 整形 | `feeds/service.rs::fetch_and_store`（`.map_err(\|e\| AppError::Upstream(e.to_string()))`、`error_for_status()`） | HTTP 失敗を `AppError::Upstream` に整形（同型） |
| 記事の読み取り | `articles/repository.rs::get(pool, ArticleId) -> AppResult<Article>`（`SELECT * ... ok_or(AppError::NotFound)`） | 抽出対象の URL/既存状態を引く。`extraction` から呼ぶ |
| 記事への書き込みは `articles::repository` 経由 | `feeds/service.rs` が `articles::repository::upsert(...)` を呼んで articles に書く**前例**（クロススライス書き込みは articles 自身のリポジトリ関数経由で行う、が確立パターン） | `articles::repository::save_full_content(...)`（**本機能で新設**）を `extraction` から呼ぶ。articles アグリゲートの書き込み所有を移さない（§5.0） |
| 既存スライス拡張＝同一アグリゲート書き込みのみ | foundation 方針 / `articles` スライス | `articles` への触りは「同一アグリゲート（articles 行）への列追加・書き込み・読み出し優先順位」に限定し、明示フラグ（§5.6）で正当化 |
| 任意機能 = config ゲート | `articles/service.rs::llm_client()`（`anthropic_api_key` 無し → `NotEnabled`）、`create_feed` の best-effort 初回 fetch（失敗を `tracing::warn!` で握りつぶす） | 自動抽出は `EXTRACT_ON_CRAWL` で gate し、失敗は warn で握りつぶす（best-effort）。`create_feed` の初回 fetch と同型 |
| AI 呼び出し作法 | `articles/service.rs`（`shared/llm` の `LlmClient`、キャッシュ命中で再呼び出ししない、`NotEnabled`） | **一切変えない**。入力を `content` → `full_content` 優先に差し替えるだけ |
| 値オブジェクト `parse() -> Result<_, String>` | `feeds/domain.rs::FeedUrl::parse`（`http://`/`https://` 検査 + trim、`#[cfg(test)]`） | 取得 URL 検証 `FetchUrl::parse` を `extraction` 内に同型で新設 |
| `AppError` 6 バリアント | `shared/error.rs`（`NotFound`/404, `Validation`/400, `NotEnabled`/503, `Upstream`/502, `Database`/500, `Other`/500、`IntoResponse` で `Json({"error": <Display>})`） | 新バリアントを足さず既存で表現（§5.5）。**`error.rs` は編集しない** |
| スライス構成 + `routes()` + 合成 | `articles/mod.rs`・`feeds/mod.rs`（`fn routes() -> Router<AppState>`）、`features/mod.rs`（`pub mod ...;` + `.merge(...::routes())`） | `extraction` を同型で作り、`features/mod.rs` に2行追加。既存スライスのルートは触らない |
| フロント HTML サニタイズ | `frontend/src/lib/sanitize.ts`（DOMPurify、`<style>`/inline style/`<script>`/`on*`/`javascript:` を除去） | バックエンド側のサニタイズはこの**相当**を `ammonia`（Rust 版 DOMPurify）で行う。フロントは表示時に従来どおり `sanitizeArticleHtml` を二重がけ（多層防御） |
| フロント API クライアント | `frontend/src/lib/api.ts`（`http<T>()` は 204→`undefined`、`api` に `動詞+リソース` 命名でメソッド集約。`Article` 型あり） | `http<T>()` を再利用しメソッド1つ追加。`Article` 型に2フィールド追加 |
| HTTP スモークの慣習 | `scripts/test/api-*.sh`（稼働スタックに curl、HTTP コードと JSON キーを assert） | `scripts/test/api-extraction.sh` を同型で新設（§9.3） |
| 自動マイグレーション実行 | `main.rs` 起動時 `sqlx::migrate!("./migrations").run(pool)` | ファイルを置くだけで適用。番号順序に注意（§4） |

> **新規 crate（§11 で詳述）**: 本機能は DOM パース用 `scraper` と HTML サニタイズ用 `ammonia` を追加する。`Cargo.toml` への依存追加は本コードベースで前例が少ないため、**着手時に crates.io / `cargo add` で最新の安定版を確認**（本書では `scraper = "0.20"` / `ammonia = "4"` を暫定採用。API は版で変わりうるので使用箇所を実装時に確認）。`reqwest`/`uuid`/`chrono`/`sqlx`/`serde` は既存依存で足りる。

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方

`main.rs` の `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を設定していないため、**適用済み最大バージョンより小さい未適用マイグレーションを後から追加すると起動時に out-of-order エラー**になる（家庭内サーバの永続 DB で実害）。

**ルール（必ず守る）**:
- 着手直前に `ls backend/migrations/` で**現状の最大番号 +1** を採番する。本書執筆時点の最新は `0005_search.sql` なので暫定 **`0006_full_content.sql`**。
- 既存ファイル（`0001`〜`0005`）は**編集しない**（追記のみ）。
- 並行で apalis 移行や他機能が `0006` を先に取った場合は、本機能を `0007_*` 以降へ繰り上げる。

### 4.2 スキーマ

新規ファイル **`backend/migrations/0006_full_content.sql`**:

```sql
-- Full article body extracted on demand from the article's source URL
-- (DOM heuristic + sanitize). NULL until extraction succeeds; on failure we
-- leave it NULL and keep falling back to articles.content.
-- AI features (summarize/translate/future Ask) read full_content when present.
ALTER TABLE articles
    ADD COLUMN IF NOT EXISTS full_content TEXT,
    ADD COLUMN IF NOT EXISTS extracted_at TIMESTAMPTZ;
```

設計判断:
- **`content` を置き換えず別列にする理由**: `content`（フィード由来）は常に取得できる確実なフォールバック。抽出は失敗・劣化しうるので、`full_content` が NULL/失敗でも閲覧・要約が壊れないよう分離する。
- **`extracted_at` をキャッシュ判定キーにする**: 非 NULL なら「抽出試行済み（成功）」。`POST .../extract` は `extracted_at` 有り（かつ `force` 無し）ならキャッシュを返し再フェッチしない（要約のキャッシュ命中と同型の節約）。
- **抽出失敗時は両列を NULL のまま据え置く**: 「成功した本文だけ」を `full_content` に入れる。失敗を空文字で塗らない（フォールバック判定が単純になる）。
- **インデックス不要**: `full_content` は全文検索（機能=検索スライスの `pg_trgm`）の対象にはしない（MVP）。将来検索を全文へ広げるなら別マイグレーションで追加。

他テーブルへの変更は無い。`full_content` は `articles` アグリゲートの一部であり、書き込みは `articles::repository` 経由で行う（§5.0）。

---

## 5. バックエンド設計

### 5.0 スライス境界と「repository を持たない」判断

`extraction` は**新規テーブルを持たず、`articles` 行の一部（`full_content`/`extracted_at`）を読み書きする**。本コードベースの確立パターンは「articles への書き込みは `articles::repository` の関数経由で行う」（`feeds/service.rs` が `articles::repository::upsert` を呼ぶ前例）である。したがって:

- `extraction` は **自前の `repository.rs` を持たない**（articles のための SQL を別スライスに重複させない）。
- 読み出しは `articles::repository::get`、書き込みは **新設する `articles::repository::save_full_content`** を呼ぶ。
- これは「5ファイル構成」からの**意図的な逸脱**（05 のテスト配置逸脱と同じ性質の、根拠ある逸脱）。`extraction` のファイルは `domain.rs` / `service.rs` / `handler.rs` / `mod.rs` の4枚。
- articles への触りは「同一アグリゲート（articles 行）への列追加・専用書き込み関数・AI 入力の優先順位」に限定し、§5.6 で明示フラグする。articles アグリゲートの**所有は移していない**ので「越境共通レイヤー」には当たらない。

### 5.1 `extraction/domain.rs`（値オブジェクト + 純粋ロジック + テスト）

抽出の中核ロジックを純粋関数に切り出し、ネットワーク無しで TDD する。`scraper`（DOM パース）と `ammonia`（サニタイズ）はどちらもオフライン・決定的なので単体テスト可能。

```rust
use ammonia::Builder;
use scraper::{Html, Selector};

/// 取得対象 URL の値オブジェクト。articles.url は本来 http(s) のはずだが防御的に通す。
#[derive(Debug, Clone)]
pub struct FetchUrl(String);

impl FetchUrl {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into();
        let t = s.trim();
        if !(t.starts_with("http://") || t.starts_with("https://")) {
            return Err("url must start with http:// or https://".into());
        }
        Ok(Self(t.to_string()))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

/// 抽出結果。成功時のみ html を持つ。プレーンテキスト長で「意味のある本文か」を判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extracted {
    /// サニタイズ済み本文 HTML。
    Ok(String),
    /// 本文が見つからない／短すぎる（full_content は NULL のまま据え置く）。
    TooThin,
}

/// 生 HTML（フェッチ済み文字列）から本文を抽出してサニタイズする入口。
/// 1) DOM パース → 2) 本文コンテナ選定 → 3) サニタイズ（ノイズタグは内容ごと除去）
/// → 4) 最小文字数チェック。
pub fn extract_main_content(raw_html: &str, min_chars: usize) -> Extracted {
    let doc = Html::parse_document(raw_html);
    let candidate = pick_main_html(&doc).unwrap_or_else(|| raw_html.to_string());
    let clean = sanitize_content(&candidate);
    if plain_text_len(&clean) >= min_chars {
        Extracted::Ok(clean)
    } else {
        Extracted::TooThin
    }
}

/// 本文コンテナの HTML を選ぶ。優先順位:
///   1) <article>（最初の1つ） 2) <main> 3) スコア最大のブロック要素
/// スコア = 配下 <p> のテキスト総文字数 − リンクテキスト文字数（リンク密度ペナルティ）。
pub fn pick_main_html(doc: &Html) -> Option<String> {
    if let Some(html) = first_inner_html(doc, "article") {
        return Some(html);
    }
    if let Some(html) = first_inner_html(doc, "main") {
        return Some(html);
    }
    // フォールバック: div/section の中で本文スコア最大の要素を選ぶ。
    let block_sel = Selector::parse("div, section").ok()?;
    let mut best: Option<(i64, String)> = None;
    for el in doc.select(&block_sel) {
        let score = score_node_text(&el.text().collect::<String>(), link_text_len(&el));
        if score > best.as_ref().map(|(s, _)| *s).unwrap_or(0) {
            best = Some((score, el.inner_html()));
        }
    }
    best.map(|(_, html)| html)
}

fn first_inner_html(doc: &Html, sel: &str) -> Option<String> {
    let s = Selector::parse(sel).ok()?;
    doc.select(&s).next().map(|el| el.inner_html())
}

/// リンク内のテキスト長（リンク密度の指標）。
fn link_text_len(el: &scraper::ElementRef) -> i64 {
    let a = Selector::parse("a").unwrap();
    el.select(&a).map(|x| x.text().collect::<String>().chars().count() as i64).sum()
}

/// 本文スコア（純粋関数 = テスト対象）。本文長からリンク文字数を引く。
/// 短すぎる断片は 0 に丸める。
pub fn score_node_text(text: &str, link_len: i64) -> i64 {
    let len = text.chars().filter(|c| !c.is_whitespace()).count() as i64;
    if len < 25 { return 0; }
    (len - link_len).max(0)
}

/// HTML をサニタイズ（Rust 版 DOMPurify = ammonia）。
/// - script/style/nav/header/footer/aside/form/noscript は **内容ごと**除去。
/// - 許可タグのみ残し、リンクには rel=noopener を付与。
/// - frontend lib/sanitize.ts（<style>/inline style/script を落とす）と同方針。
pub fn sanitize_content(raw_html: &str) -> String {
    Builder::default()
        .clean_content_tags(
            ["script", "style", "nav", "header", "footer", "aside", "form", "noscript", "iframe"]
                .into_iter()
                .collect(),
        )
        .link_rel(Some("noopener noreferrer"))
        .clean(raw_html)
        .to_string()
}

/// サニタイズ済み HTML のおおよそのプレーンテキスト文字数（空白除く）。
pub fn plain_text_len(html: &str) -> usize {
    let doc = Html::parse_fragment(html);
    doc.root_element()
        .text()
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .count()
}
```

> ステータス分類ではなく「コンテナ選定・スコア・サニタイズ・文字数判定」を純粋関数に切り出すのは、ネットワーク無しで Red→Green を回すため（MEMORY の「書いたら必ず実行」「バグ修正もテスト先行」）。`ammonia` の `clean_content_tags` / `link_rel` の正確な API は版で変わりうるので実装時に確認（§11）。

### 5.2 `articles/repository.rs` への追加（同一アグリゲートの書き込み関数）

`articles/repository.rs` の末尾に **1関数だけ**追加（既存関数は触らない）:

```rust
/// 抽出した本文をキャッシュ。成功時のみ呼ぶ（失敗時は呼ばず NULL 据え置き）。
pub async fn save_full_content(
    pool: &PgPool,
    id: ArticleId,
    full_content: &str,
) -> AppResult<()> {
    let res = sqlx::query(
        r#"UPDATE articles
           SET full_content = $2, extracted_at = now()
           WHERE id = $1"#,
    )
    .bind(id.0)
    .bind(full_content)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}
```

> `query!` コンパイル時マクロは使わない（ランタイム `query` のみ）。`save_summary`/`save_translation` と同じ作法。`feeds/service.rs` が `articles::repository::upsert` を呼ぶのと同じく、`extraction` はこの関数を呼んで articles に書く（書き込み所有は articles に残す）。

### 5.3 `extraction/service.rs`（`&AppState` を取り fetch + domain を統合）

```rust
use uuid::Uuid;

use super::domain::{extract_main_content, Extracted, FetchUrl};
use crate::features::articles::domain::{Article, ArticleId};
use crate::features::articles::repository as articles_repo;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// 抽出結果のメタ（handler が Article と一緒に返せるよう保持。今回は Article を返す方針）。
pub struct ExtractOutcome {
    pub article: Article,
    pub extracted: bool, // true=今回 full_content を更新, false=キャッシュ命中 or thin
}

/// オンデマンド抽出の本体。順序:
///   (1) 記事取得（無ければ NotFound）
///   (2) force=false かつ extracted_at 有り → キャッシュ返却（再フェッチしない）
///   (3) URL 取得 → 抽出 → サニタイズ → 文字数判定
///   (4) Ok なら save_full_content、TooThin なら据え置き（NULL のまま）
///   (5) 更新後の Article を返す
pub async fn extract_article(
    state: &AppState,
    id: ArticleId,
    force: bool,
) -> AppResult<ExtractOutcome> {
    let article = articles_repo::get(&state.db, id).await?; // 無ければ NotFound

    if !force && article.extracted_at.is_some() {
        return Ok(ExtractOutcome { article, extracted: false });
    }

    let url = FetchUrl::parse(article.url.clone()).map_err(AppError::Validation)?;
    let html = fetch_html(state, &url).await?;

    match extract_main_content(&html, state.config.extract_min_chars) {
        Extracted::Ok(content) => {
            articles_repo::save_full_content(&state.db, id, &content).await?;
            let article = articles_repo::get(&state.db, id).await?;
            Ok(ExtractOutcome { article, extracted: true })
        }
        Extracted::TooThin => {
            // 本文が薄い → full_content は NULL のまま。content にフォールバックできる。
            Ok(ExtractOutcome { article, extracted: false })
        }
    }
}

/// クロール時の自動抽出（best-effort）。feeds 側から呼ぶ。失敗は握りつぶす。
/// 既に extracted_at 有りの記事はスキップ（force=false）。
pub async fn extract_best_effort(state: &AppState, id: ArticleId) {
    if let Err(e) = extract_article(state, id, false).await {
        tracing::warn!(error = %e, article = %id.0, "auto extraction failed");
    }
}

/// 記事 URL を取得し HTML テキストを返す。サイズ上限・content-type を防御的に確認。
async fn fetch_html(state: &AppState, url: &FetchUrl) -> AppResult<String> {
    let resp = state
        .http
        .get(url.as_str())
        .header("accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::Upstream(e.to_string()))?;

    // content-type が HTML でないものは弾く（PDF/画像などを抽出に回さない）。
    if let Some(ct) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
        let ct = ct.to_str().unwrap_or_default();
        if !ct.contains("html") {
            return Err(AppError::Validation(format!("not an HTML page: {ct}")));
        }
    }

    let bytes = resp.bytes().await.map_err(|e| AppError::Upstream(e.to_string()))?;
    if bytes.len() > state.config.extract_max_bytes {
        return Err(AppError::Validation("page too large to extract".into()));
    }
    // reqwest の bytes をUTF-8 ロッシーでデコード（charset 厳密対応は §11）。
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
```

> HTTP を `service.rs` に閉じるのは、本スライスに trait/dyn を作らない方針（抽象境界は `shared/llm` のみ）に沿うため。差し替え予定が無いので struct 化も trait 化もしない。`ExtractOutcome` を返すのは将来「抽出はしたが薄かった」を UI に区別表示したくなった時の余地（今回は handler で Article を返す）。

### 5.4 `extraction/handler.rs`（axum ハンドラ）

```rust
use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::service;
use crate::features::articles::domain::{Article, ArticleId};
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct ExtractBody {
    /// 既に抽出済みでも再抽出するか。省略時 false（キャッシュ優先）。
    #[serde(default)]
    pub force: bool,
}

/// POST /api/articles/{id}/extract
/// 成功・キャッシュ命中・本文薄のいずれも 200 + 更新後 Article を返す。
/// （full_content が NULL のままなら「抽出できなかった」を意味する。要約等は content にフォールバック）
pub async fn extract(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Option<Json<ExtractBody>>,
) -> AppResult<Json<Article>> {
    let force = body.map(|Json(b)| b.force).unwrap_or(false);
    let outcome = service::extract_article(&state, ArticleId(id), force).await?;
    Ok(Json(outcome.article))
}
```

> `summarize`/`translate` が更新後 `Article` を返すのに合わせ、`extract` も `Article` を返す（フロントはレスポンスで `full_content` の有無を判定できる）。body は任意（`Option<Json<_>>`）にして、ボディ無し POST でも `force=false` で動く（`mark_all_read` と同型）。

### 5.5 `extraction/mod.rs`（routes）

```rust
pub mod domain;
pub mod handler;
pub mod service;

use axum::routing::post;
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // articles スライスの /api/articles/{id} 系とはパス末尾が異なるので衝突しない。
        .route("/api/articles/{id}/extract", post(handler::extract))
}
```

### 5.6 `features/mod.rs` への追加（2行）と AI 入力の優先順位変更

`features/mod.rs`（既存スライスのルートは触らない）:

```rust
pub mod extraction; // 既存 pub mod 群に追加
// router() 内の .merge チェーンに追加:
        .merge(extraction::routes())
```

**AI 入力の優先順位変更（本機能の主目的・articles 同一アグリゲートへの最小編集）**:

`articles/domain.rs` の `Article` 構造体に2フィールド追加（`SELECT *` で列がマップされ、フロントへも serialize される）:

```rust
    pub full_content: Option<String>,
    pub extracted_at: Option<chrono::DateTime<chrono::Utc>>,
```

`articles/service.rs` の `summarize_article` / `translate_article` で、LLM に渡す本文を `full_content` 優先に変更（各1行）:

```rust
    // before: title/content をそのまま渡していた
    let ai_content = article
        .full_content
        .clone()
        .unwrap_or_else(|| article.content.clone());
    // summarize: SummarizeRequest { title, content: ai_content, target_lang }
    // translate: TranslateRequest { content: ai_content, target_lang }
```

> **これが「全 AI 機能の品質上限を底上げする」核心**。`full_content` があればそれを、無ければ従来の `content` を使う。キャッシュ判定（`summary`/`summary_lang` が一致すれば再呼び出ししない）と `NotEnabled`（`ANTHROPIC_API_KEY` 未設定）の作法は**一切変えない**。
>
> **将来の Ask 機能への土台**: Ask（記事への質問）を実装する別スライスも、同じ `full_content.unwrap_or(content)` の優先順位で LLM 入力を組み立てればよい。この優先順位ロジックは1行なのでクロススライス共通関数化はしない（trait/共通レイヤーを増やさない方針）。
>
> これら articles への編集は「同一アグリゲート（articles 行）への列追加と、その行に対する AI 入力の選択」に限定されており、新スライス（`extraction`）が articles の所有を奪うものではない。`Article` への列追加・`save_full_content` の追加・AI 入力差し替えの3点が articles への全接触点。

### 5.7 クロール時の自動抽出（feeds への best-effort 追記）

`feeds/service.rs::fetch_and_store` のループ内、`articles::repository::upsert(...)` の直後に **config ゲートした best-effort 抽出**を追記する。`upsert` は記事 id を返さないので、URL から id を引いて渡す（最小変更）か、新規記事のみを対象に絞る。MVP は「config 有効時のみ・失敗握りつぶし」で軽く入れる:

```rust
        articles::repository::upsert(&state.db, FeedId(feed.id.0), &url, &title, &content, published).await?;

        // 任意自動抽出（EXTRACT_ON_CRAWL=true のときだけ）。best-effort。
        if state.config.extract_on_crawl {
            if let Some(id) = articles::repository::id_by_url(&state.db, &url).await? {
                crate::features::extraction::service::extract_best_effort(state, id).await;
            }
        }
```

これに伴い `articles/repository.rs` に補助関数 `id_by_url` を追加（同一アグリゲートの読み取り）:

```rust
pub async fn id_by_url(pool: &PgPool, url: &str) -> AppResult<Option<ArticleId>> {
    let row: Option<(uuid::Uuid,)> =
        sqlx::query_as("SELECT id FROM articles WHERE url = $1")
            .bind(url)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(id,)| ArticleId(id)))
}
```

> **代替案（§11）**: feeds に触りたくない場合、`extraction` スライス側に「`extracted_at IS NULL` の記事を定期的に少数ずつ抽出する」軽量 sweep を `shared/scheduler.rs` 相当で持たせる方式もある。MVP は実装量が小さく即時性のある「クロール直後 best-effort」を既定とし、feeds への追記は config 既定 false で無効化しておく（オプトイン）。`extract_best_effort` は `extracted_at` 有りをスキップするので冪等。

---

## 6. フロントエンド設計

### 6.1 `lib/api.ts` への追加（型2フィールド + メソッド1）

`Article` インターフェースに2フィールド追加（backend の `Article` JSON をミラー）:

```ts
export interface Article {
  // ...既存フィールド...
  full_content: string | null;
  extracted_at: string | null;
}
```

`api` オブジェクトにメソッド追加（既存 `http<T>()` を再利用、命名は `動詞+リソース`）:

```ts
  // 記事本文をサーバ側で抽出して full_content をキャッシュ。更新後 Article を返す。
  extractArticle: (id: string, force = false) =>
    http<Article>(`/api/articles/${id}/extract`, {
      method: "POST",
      body: JSON.stringify({ force }),
    }),
```

### 6.2 記事ビュー（`routes/ArticleView.tsx` 相当）への導線

記事本文表示に「全文を取得」ボタンと、全文/抜粋の表示切り替えを足す。状態は**ローカル**（`createSignal`）で足り、グローバルストアは不要。

- 表示する HTML は `article.full_content ?? article.content` を **`sanitizeArticleHtml()`（`lib/sanitize.ts`）に通して** `prose` クラスでレンダリング（多層防御。バックエンドでサニタイズ済みでもフロントで再度浄化する既存方針を維持）。
- 「全文を取得」ボタン: `full_content` が未取得（`extracted_at == null`）のとき表示。クリックで `busy` を立て `const updated = await api.extractArticle(article.id)` → 返ってきた `Article` でローカル表示を差し替え。`updated.full_content` が `null` のまま（薄かった/失敗）なら「全文を取得できませんでした（抜粋を表示中）」を `text-xs text-muted-foreground` で表示。
- 既に `full_content` 有りなら「全文 / 抜粋」トグル（`Button variant="ghost"`）を出し、ユーザーが見比べられるようにする。`force=true` での再取得は「再抽出」メニュー（任意）に置く。
- ボタンは `components/ui/button.tsx` を使用（新規 UI 部品は不要）。

> `full_content` が要約/翻訳の入力に使われることはバックエンドで自動なので、フロントは「全文を取得→（必要なら）要約/翻訳」の順をユーザーに促すだけでよい。要約済み記事に後から全文を取得した場合、要約は古い `content` ベースのキャッシュのままになる点は §11 に注記（必要なら `force` 要約で更新）。

### 6.3 Ark UI について

本機能で必要な UI はボタンとトグルのみで自前 Tailwind / 既存 `button.tsx` で賄える。**Ark UI 部品は不要**。

---

## 7. API 契約

> すべて `/api` プレフィックス。

### 7.1 `POST /api/articles/{id}/extract` — 記事本文を抽出・キャッシュ

リクエスト（body 任意）:
```json
{ "force": false }
```
- `force` 省略時 false。`extracted_at` 有りかつ `force=false` ならキャッシュを返し再フェッチしない。

レスポンス（200、更新後 Article。抜粋）:
```json
{
  "id": "1f1c0e8a-...",
  "url": "https://example.com/post",
  "title": "...",
  "content": "<p>フィード由来の抜粋…</p>",
  "full_content": "<article><p>抽出された全文…</p></article>",
  "extracted_at": "2026-06-30T12:34:56Z",
  "is_read": false,
  "summary": null,
  "summary_lang": null,
  "translation": null,
  "translation_lang": null,
  "processed_at": null,
  "created_at": "2026-06-29T00:00:00Z"
}
```

- **抽出できなかった場合も 200** を返すが `full_content` は `null`・`extracted_at` も `null` のまま（クライアントは「抜粋にフォールバック」と判断）。

エラー:
- 404 `{ "error": "resource not found" }`（`id` に該当記事なし）
- 400 `{ "error": "invalid input: not an HTML page: application/pdf" }`（HTML でない）
- 400 `{ "error": "invalid input: page too large to extract" }`（サイズ上限超過）
- 502 `{ "error": "upstream request failed: ..." }`（取得失敗・ネットワーク障害・元サイトの 4xx/5xx）

> **`ANTHROPIC_API_KEY` 不要**: このエンドポイントは LLM を呼ばない（HTTP 取得 + DOM 抽出のみ）。`NotEnabled` は返さない。AI 機能（要約/翻訳）が `full_content` を読むのは別エンドポイント（`/summarize`・`/translate`）で、そちらは従来どおり `ANTHROPIC_API_KEY` 未設定で `NotEnabled`（503）を返す。

### 7.2 既存エンドポイントへの影響（契約拡張のみ・破壊なし）

- `GET /api/articles` / `GET /api/articles/{id}` のレスポンス `Article` に `full_content` / `extracted_at` が**追加**される（既存フィールドは不変。後方互換）。
- `POST /api/articles/{id}/summarize` / `.../translate` は入力に `full_content` を優先するようになるが、**リクエスト/レスポンスの形は不変**。`full_content` 取得後に要約すると品質が上がる。

---

## 8. 依存関係

- **本機能が依存する（既存・実装済み）**:
  - `articles` スライス（`get` / `Article` / 新設 `save_full_content`・`id_by_url`）と `articles` テーブル。AI 入力差し替えは `articles/service.rs` で行う。
  - `shared/state`（`state.http`）・`shared/config`・`shared/error`。
- **本機能を活かす（ソフト・整合）**:
  - 要約/翻訳（既存 AI 機能）— `full_content` を入力に使うので、本機能後に要約すると品質が上がる。
  - 記事ビュー UI（二ペイン=機能 10 / ミニマル=機能 07）— 「全文を取得」ボタンの置き場所。10/07 が未マージでも既存 `ArticleView` に置けば動く。
  - 将来の **Ask** 機能 — `full_content` 優先の入力規約を共有（§5.6）。
- **既存スライスへの変更点（接触面）**:
  - `articles`（同一アグリゲート）: `Article` に2フィールド・`repository` に `save_full_content`/`id_by_url`・`service` の AI 入力差し替え。
  - `feeds`（任意・config 既定 false）: `fetch_and_store` に best-effort 抽出を1ブロック追記。
  - `features/mod.rs`: 2行追加。`shared/config.rs`: env 3つ追加。`Cargo.toml`: crate 2つ追加。
- **番号衝突注意**: apalis 移行・他機能とマイグレーション番号（`0006`）が競合しうる。着手前に最新番号を確認（§4.1）。

---

## 9. テスト計画（TDD）

> **テスト配置の方針（05 と同じ前例に従う）**: 本クレートは binary crate（`lib.rs` 無し）なので `backend/tests/` の別クレートから内部関数を呼べない。よって (1) 純粋ロジックは各 `.rs` 内 `#[cfg(test)] mod tests`、(2) DB を触る往復テストは `repository.rs` 内 `#[cfg(test)]` + `#[ignore]`（実 DB）、(3) HTTP 表面は shell スクリプト、の3段で置く。

### 9.1 単体テスト（`extraction/domain.rs` 内、ネットワーク不要）

`backend/src/features/extraction/domain.rs` 末尾。Red を先に書く。`scraper`/`ammonia` はオフライン・決定的なので外部 I/O 無しで回る。

| テスト | 意図 |
|---|---|
| `fetch_url_accepts_http_and_https` | `http://`/`https://` を受理 |
| `fetch_url_rejects_missing_scheme` | スキーム無しを拒否 |
| `fetch_url_trims` | 前後空白を除去 |
| `pick_main_prefers_article_tag` | `<article>` がある HTML で `<article>` 配下を選ぶ |
| `pick_main_falls_back_to_main_tag` | `<article>` 無し・`<main>` 有りで `<main>` を選ぶ |
| `pick_main_scores_highest_p_density_block` | `<article>`/`<main>` 無し時、`<p>` 本文が最多の `<div>` を選ぶ |
| `score_node_text_penalizes_links` | リンク文字数が多いノードのスコアが下がる |
| `score_node_text_zero_for_tiny` | 25 文字未満は 0 |
| `sanitize_strips_script_and_style` | `<script>`/`<style>` を内容ごと除去 |
| `sanitize_strips_nav_footer_aside` | `<nav>`/`<footer>`/`<aside>` を内容ごと除去 |
| `sanitize_adds_rel_noopener_to_links` | `<a href>` に `rel="noopener noreferrer"` が付く |
| `extract_main_returns_too_thin_when_below_min` | 本文が `min_chars` 未満で `Extracted::TooThin` |
| `extract_main_returns_ok_for_real_body` | 十分な長さの本文で `Extracted::Ok(html)`、`html` に本文段落を含む |
| `plain_text_len_ignores_tags_and_whitespace` | タグ・空白を除いた文字数を返す |

雛形（一部）:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html><body>
        <nav><a href="/">home</a></nav>
        <article><p>これは十分に長い本文の段落です。意味のある文章が続きます。</p>
        <p>二つ目の段落も本文として抽出されるべきです。</p></article>
        <footer><a href="/about">about</a></footer>
        <script>alert(1)</script>
    </body></html>"#;

    #[test]
    fn extract_main_returns_ok_for_real_body() {
        match extract_main_content(PAGE, 10) {
            Extracted::Ok(html) => {
                assert!(html.contains("本文の段落"));
                assert!(!html.contains("alert(1)"));
                assert!(!html.to_lowercase().contains("<nav"));
            }
            Extracted::TooThin => panic!("expected Ok"),
        }
    }
}
```

### 9.2 リポジトリ往復テスト（`articles/repository.rs` 内、実 DB / `#[ignore]`）

`save_full_content` / `id_by_url` を実 DB で検証。`DATABASE_URL`（`just dev-db`）に接続し `#[tokio::test]` + `#[ignore]`。マイグレーション適用済み DB を前提。`articles` への INSERT には feed が要るので、テスト内で feed を1件作ってから記事を upsert する。

| テスト | 意図 |
|---|---|
| `save_full_content_sets_full_content_and_extracted_at` | upsert→`get` で `full_content` null 確認 → `save_full_content` → `get` で `full_content`/`extracted_at` が入る |
| `save_full_content_missing_id_is_not_found` | 存在しない id で `AppError::NotFound` |
| `id_by_url_roundtrip` | upsert 後に URL から id を引ける／存在しない URL で `None` |

### 9.3 HTTP スモークテスト（稼働スタックへの shell スクリプト）

`scripts/test/api-extraction.sh` を `scripts/test/api-*.sh` と同型で新設（nginx 経由）。**外部サイトを叩かない範囲**を決定的に検証:

| 手順 / アサーション | 意図 |
|---|---|
| `POST /api/articles/00000000-0000-0000-0000-000000000000/extract` → 404 | 記事不在で `NotFound`（スライス合成 + ルーティング配線確認） |
| `GET /api/articles` の各 Article JSON に `full_content` / `extracted_at` キーが存在 | レスポンス契約拡張の確認（値は null 可） |

> 実在記事の抽出成功パスは外部サイト到達が要るため自動 CI では検証しない。手動手順は §10 step 11。

### 9.4 フロント（手動 + 型）
- `tsc` 型チェック（`just lint`）で `api.ts`（`Article` 拡張・`extractArticle`）と `ArticleView` の整合を確認。
- 手動: 抜粋しか流さないフィードの記事を開く → 「全文を取得」→ 全文表示に切替わる → 要約すると全文ベースになる、を確認。

---

## 10. 実装手順（順序付きチェックリスト）

1. **依存追加**: `cargo add scraper ammonia`（`backend/`）。版を確認（暫定 `scraper = "0.20"` / `ammonia = "4"`）。`just lint`（clippy）が通る空実装で一旦ビルド確認。
2. **マイグレーション番号採番**: `ls backend/migrations/` で最大番号 +1（暫定 `0006_full_content.sql`）。既存ファイルは触らない。
3. **マイグレーション作成**: §4.2 の `ALTER TABLE` を新規ファイルに。
4. **config**: `shared/config.rs` に `extract_on_crawl: bool`（`EXTRACT_ON_CRAWL`、既定 false）・`extract_max_bytes: usize`（`EXTRACT_MAX_BYTES`、既定 `3_000_000`）・`extract_min_chars: usize`（`EXTRACT_MIN_CHARS`、既定 `200`）を追加（既存フィールドの読み取り方に合わせる）。
5. **articles 拡張（同一アグリゲート）**: `articles/domain.rs` の `Article` に `full_content`/`extracted_at` を追加。`articles/repository.rs` に `save_full_content` と `id_by_url` を追加。§9.2 の `#[cfg(test)] mod tests`（`#[ignore]`）も書く。
6. **AI 入力差し替え**: `articles/service.rs` の `summarize_article`/`translate_article` で `full_content.unwrap_or(content)` を LLM 入力に（§5.6）。
7. **extraction ドメイン（Red 先行）**: `backend/src/features/extraction/domain.rs` を §5.1 で作成し、§9.1 のテストを先に書いて落ちることを確認 → 実装で Green。`cargo test` 実行。
8. **extraction service / handler / mod**: §5.3〜§5.5 を作成。`service` は `state.http` + `extract_main_content` + `articles::repository`。
9. **合成 + 自動抽出**: `features/mod.rs` に `pub mod extraction;` と `.merge(extraction::routes())`（§5.6）。`feeds/service.rs` に config ゲートした best-effort 抽出を1ブロック追記（§5.7、既定 false なので挙動は変わらない）。
10. **ビルド & lint**: `just lint`（clippy `-D warnings` / tsc）。`scraper`/`ammonia` の API は実装時にドキュメントで確認。
11. **DB & 手動 E2E**: `just dev-db` →（起動で自動 migrate or `just migrate`）。実記事で `POST /api/articles/{id}/extract` → `full_content` が入るか、薄いページで null 据え置きか、`force` で再抽出されるかを確認。`DATABASE_URL=... cargo test -- --ignored` で §9.2 を Green に。
12. **HTTP スモーク**: `scripts/test/api-extraction.sh` を §9.3 で作成・`chmod +x`・実行（404 と JSON キー存在を assert）。
13. **フロント**: `lib/api.ts`（`Article` 2フィールド + `extractArticle`）、`ArticleView` に「全文を取得」+トグル（§6）。表示は `sanitizeArticleHtml(full_content ?? content)`。`just lint` の tsc を通す。
14. **コミット**: マイグレーション・スライス・articles 拡張・config・依存・フロント・スクリプトをまとめて。秘密情報/`.env` はコミットしない。

---

## 11. リスク・未決事項・代替案

- **【要確認】新規 crate の API**: `scraper`（`Html::parse_document` / `Selector::parse` / `ElementRef::inner_html` / `.text()`）と `ammonia`（`Builder::default().clean_content_tags(...).link_rel(...).clean(...)`）の正確なシグネチャは版で変わりうる。**実装時に crates.io / docs.rs で確認**。`clean_content_tags` の引数型（`HashSet<&str>` か）も版確認。
- **抽出精度の限界（DOM ヒューリスティック）**: 汎用スコアリングは SPA・JS レンダリング・特殊レイアウトで本文を取り違える/取りこぼす。**緩和策**: 失敗・薄い時は `full_content` を NULL 据え置きにし `content` へフォールバック（閲覧・要約は壊れない）。`min_chars` で「薄い本文」を弾く。将来サイト別ルールを `extraction` スライスに追記可能。代替に readability 専用 crate（例 `readability` / `dom_smoothie`）の採用も検討余地（精度↑/依存とメンテ性のトレードオフ）。
- **文字コード**: `String::from_utf8_lossy` は非 UTF-8（Shift_JIS 等）ページで文字化けしうる。MVP は UTF-8 前提。改善するなら `Content-Type` の charset や `<meta charset>` を見て `encoding_rs` でデコードする（依存追加・別タスク）。
- **要約キャッシュの陳腐化**: 先に `content` で要約 → 後から `full_content` を取得しても、`summary`（`content` ベース）はキャッシュのまま。**緩和策**: フロントは「全文取得後は `force` 付きで要約し直せる」導線を出す、または要約サービスを「`full_content` が新しく入ったら無効化」する拡張（要約の `processed_at` と `extracted_at` の比較）。MVP はユーザー操作（再要約）に委ねる。
- **自動抽出の負荷・行儀**: `EXTRACT_ON_CRAWL=true` だとクロールごとに全新着記事へ外部 GET が飛ぶ（負荷・相手サイトへの配慮・robots）。**既定 false でオプトイン**。`extract_max_bytes` で巨大ページを弾く。代替案: クロールに同期で挟まず、`extraction` 側で `extracted_at IS NULL` を少数ずつ拾う軽量 sweep（§5.7）。レート制御が要るなら apalis 移行後にジョブ化。
- **マイグレーション番号の順序ハザード**: `run_migrations` は `set_ignore_missing` を呼ばないため、先に高い番号を適用した永続 DB に後から低い番号を足すと起動が壊れる。着手前に最小空き整数を採番（§4.1）。
- **SSRF / 内部ネットワーク到達**: 記事 URL は購読フィード由来で基本信頼できるが、`extract` は任意の URL に対しサーバが GET する経路になる。家庭内 LAN・単一ユーザー前提では許容。気にするなら取得先のホスト/IP をパブリックレンジに制限する検証を `FetchUrl::parse` 後に足す（将来拡張）。
- **`articles` への接触が3点ある点**: 「新スライス1枚」の理想からは articles 編集（domain/repository/service）が増えるが、いずれも**同一アグリゲート（articles 行）への列追加・専用書き込み関数・AI 入力選択**に限定され、`feeds` が `articles::repository::upsert` を呼ぶ確立パターンと同性質。越境共通レイヤーは作っていない（§5.0 / §5.6）。
- **ルート衝突**: `extraction` の `/api/articles/{id}/extract` と articles の `/api/articles/{id}` 系は末尾セグメントが異なるため axum の `Router::merge` で衝突しない。実装後に `GET /api/articles/{id}` と `.../extract` が両方 200 することをスモークで確認。
