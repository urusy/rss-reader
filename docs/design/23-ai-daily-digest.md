# 23 AI デイリーダイジェスト

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッション（effort 低め）の実装者。本書 1 ファイルだけで着手・完了できるよう、再利用資産・完全な SQL・関数シグネチャ・ルート文字列・フロント差分まで具体化する。
> **重要な但し書き**: 本機能は Claude（`shared/llm`）を使う。日次バッチは家庭内サーバの `shared/scheduler.rs`（`tokio::interval`）に相乗りする。SMTP メール送信は **任意機能**（config 未設定なら送らないだけ）。Anthropic Messages API の挙動は `shared/llm/anthropic.rs` の既存実装に準拠する。

---

## 1. 概要

購読中フィードの **直近 24 時間の未読記事** を Claude にまとめさせ、トピック別の要点（Markdown）を **1 日 1 本** 生成・DB キャッシュする「AI デイリーダイジェスト」。要約/翻訳と同じく **オンデマンドではなくスケジュール駆動**だが、生成結果を `digests` テーブルにキャッシュし、同一日付の再要求はトークンを消費せずキャッシュを返す（要約/翻訳のキャッシュ方針と同型）。

実装はバックエンドに **新スライス `digest` を 1 枚**追加し、(a) 日次生成（`shared/scheduler.rs` から起動）、(b) `GET /api/digest/latest`・`GET /api/digest?date=` での取得、(c) `POST /api/digest/refresh` での当日手動生成（テスト容易性・即時確認のため）を担う。LLM 連携は **`shared/llm` を再利用**（`LlmClient` trait に `digest` メソッドを 1 つ追加）し、`ANTHROPIC_API_KEY` 未設定時は `AppError::NotEnabled`（503）を返す「任意機能」パターンに従う（要約/翻訳が取る挙動と同型）。

任意で、生成成功時に **SMTP でメール送信**する（config に SMTP ホスト等が揃っているときだけ。未設定なら静かにスキップ）。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）
- マイグレーション **`0006_digests.sql`**（番号は暫定。**着手前に必ず `ls backend/migrations/` で最新番号を確認し +1 を採る**。現状の最新は `0005_search.sql`。§4.1）。日次ダイジェストを保存する `digests` テーブル（`date` PK）。
- 新スライス `backend/src/features/digest/`（`domain` / `repository` / `service` / `handler` / `mod`）。
- 日次生成: `shared/scheduler.rs` に **当日ダイジェスト生成ループを 1 つ追加**。設定時刻（UTC）に到達し、当日分が未生成なら 1 回だけ生成する（冪等）。
- LLM 連携: `shared/llm` の `LlmClient` trait に **`digest` メソッドを 1 つ追加**し、`anthropic.rs` に実装（既存 private `complete` を再利用）。
- 取得 API: `GET /api/digest/latest`（最新 1 本）/ `GET /api/digest?date=YYYY-MM-DD`（指定日）。無ければ 404。
- 手動生成 API: `POST /api/digest/refresh`（当日分を生成・上書き）。`ANTHROPIC_API_KEY` 未設定なら 503。
- 任意 SMTP メール送信: config に SMTP 設定が揃うときのみ、生成成功後にダイジェスト本文をメール送信（best-effort。失敗はログのみ）。
- フロント `/digest` ルート（`routes/Digest.tsx`）: 最新ダイジェストを Markdown→HTML→sanitize して `prose` 表示。日付選択は URL `?date=`。
- `lib/api.ts` に型 `Digest` と **3 メソッド**（`getLatestDigest` / `getDigest(date)` / `refreshDigest`）。
- `config.rs` に digest 関連の env（生成 ON/OFF・生成時刻・言語・SMTP 一式）を追加。
- ドメイン純粋関数の単体テスト、リポジトリ往復テスト（実 DB・`#[ignore]`）、HTTP スモークスクリプト。

### 非スコープ（本機能では実装しない）
- 記事単位の要約/翻訳（既存 `articles` スライスが担当。本機能は記事の `summary` を **読み取り**で材料に使うだけ）。
- フィード別・フォルダ別ダイジェスト（MVP は全フィード横断 1 本）。
- ダイジェストの編集・手動キュレーション UI。
- メールテンプレートの HTML 装飾（MVP は Markdown を本文に入れたプレーン/簡易 HTML メール）。
- 過去日の一括バックフィル（`POST /api/digest/refresh` は当日のみ）。
- 複数ユーザ・宛先複数管理（単一ユーザ前提。宛先は env 1 つ）。

---

## 3. 既存実装の再利用

実ファイルを確認済み。以下を **再利用し、車輪の再発明をしない**。

| 再利用資産 | 実体（確認済みファイル） | 本機能での使い方 |
|---|---|---|
| LLM 抽象境界（唯一の trait） | `backend/src/shared/llm/mod.rs`（`LlmClient` trait＋`SummarizeRequest`/`TranslateRequest`）、`anthropic.rs`（`AnthropicClient`、private `complete(system, user)`、Messages API 直叩き） | trait に **`digest` メソッドを 1 つ追加**し、`anthropic.rs` で `complete` を再利用して実装。新しい trait は作らない（既存の唯一の境界を拡張） |
| 任意機能 = `NotEnabled` + `llm_client()` 生成 | `articles/service.rs::llm_client()`（`anthropic_api_key` 無し時に `NotEnabled("ANTHROPIC_API_KEY is not set")`、有り時 `AnthropicClient::new(state.http.clone(), key, model)`） | `digest/service.rs` に同型の `llm_client(state)` を持ち、未設定なら `NotEnabled` |
| キャッシュして再課金しない方針 | `articles/service.rs::summarize_article`（同一 lang のキャッシュ命中時は API を呼ばず返す） | `digests` を date でキャッシュ。scheduler は当日分が在れば再生成しない |
| `AppState { db, config, http }` | `backend/src/shared/state.rs`（`#[derive(Clone)]`） | `state.db` / `state.http`（UA・30s timeout 設定済み）/ `state.config` をそのまま使う |
| `AppError` 6 バリアント | `backend/src/shared/error.rs`（`NotFound`/404, `Validation(String)`/400, `NotEnabled(String)`/503, `Upstream(String)`/502, `Database`/500, `Other`/500、`IntoResponse` で `Json({"error": <Display>})`） | 新バリアントを足さず既存で表現（§5.7）。**`error.rs` は編集しない** |
| 値オブジェクト `parse() -> Result<_, String>` | `feeds/domain.rs::FeedUrl::parse`（検査＋`#[cfg(test)] mod tests`） | 日付入力の値オブジェクト `DigestDate::parse`（`YYYY-MM-DD`）を同型でスライス内に新設。`Err(String)` は `map_err(AppError::Validation)` |
| スライス構成 + `routes()` | `articles/mod.rs`・`feeds/`・`instapaper/`（`domain/repository/service/handler/mod`、`fn routes() -> Router<AppState>`） | 同じ 5 ファイル構成で `digest` を作る |
| `features/mod.rs` の合成 | `pub mod ...;` 群 + `router()` の `.merge(...::routes())` チェーン | `pub mod digest;` と `.merge(digest::routes())` を 1 行ずつ追加。既存スライスは触らない |
| sqlx ランタイムクエリ + upsert | `instapaper/repository.rs`（`fetch_optional`、`INSERT ... ON CONFLICT (id) DO UPDATE`）、`articles/repository.rs`（`fetch_optional().ok_or(AppError::NotFound)`） | digest 取得は `fetch_optional`、保存は `ON CONFLICT (date) DO UPDATE`、記事材料は読み取り SQL |
| クロステーブル read を自スライス内 SQL で完結 | `feed_overview`（feeds+articles JOIN read）、`instapaper/repository.rs::get_article_ref`（`articles` を読み取り専用 SQL で参照） | `digest` から `articles` を **読み取り専用 SQL** で引く（直近 24h の未読を材料化）。書き込み所有は移さない（§5.2） |
| 日次バッチの置き場所 | `backend/src/shared/scheduler.rs`（`tokio::interval` で `feeds::service::refresh_all_feeds` を定期実行。`main.rs` が `scheduler::spawn(state.clone())`） | 同ファイルに **digest 生成ループ**を 1 つ追加。`main.rs` から `scheduler::spawn_digest(state.clone())` を 1 行で起動（§5.6） |
| 設定の env マッピング | `backend/src/shared/config.rs`（1 field = 1 env、`Option`/`unwrap_or` パターン） | digest 関連 env を同型で追加（§5.8） |
| reqwest 共有クライアント | `main.rs` の `reqwest::Client`（UA・30s timeout） | LLM 呼び出しに `state.http` を使う（`anthropic.rs` 経由） |
| フロント API クライアント | `frontend/src/lib/api.ts`（`http<T>()`：204→`undefined`、`api` に `動詞+リソース` 命名で集約） | 既存 `http<T>()` をそのまま使い 3 メソッド追加 |
| HTML 浄化（XSS 対策） | `frontend/src/lib/sanitize.ts::sanitizeArticleHtml`（DOMPurify、`<style>`/inline style/`<script>` 除去） | Markdown→HTML 変換後に同関数で浄化してから `prose` に流し込む |
| 自前 UI 部品 | `frontend/src/components/ui/{button,card,badge}.tsx`（`cn`+Tailwind、oklch トークン） | `Digest.tsx` で `card.tsx`/`button.tsx`/`badge.tsx` を流用 |
| HTTP スモークテストの慣習 | `scripts/test/api-*.sh`（稼働スタックに curl、HTTP コードと JSON キーを assert） | `scripts/test/api-digest.sh` を同型で新設（§9.3） |

> **新規依存（要追加）**:
> - バックエンド SMTP: `lettre`（任意機能。SMTP 設定が無ければ呼ばれないので、SMTP を後回しにするなら依存追加自体を見送ってよい。§5.5・§11）。
> - フロント Markdown 変換: `marked`（Markdown→HTML）。変換結果は **必ず** 既存 `sanitizeArticleHtml` に通す（§6.3）。
>
> その他（`chrono`/`uuid`/`serde`/`sqlx`/`reqwest`/`axum`）は既存依存で足りる。`sqlx` は `chrono` feature 有効済みなので `DATE` 列は `chrono::NaiveDate` に直接マップできる。

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方

`main.rs` の `db::run_migrations` → `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を設定していないため、**適用済み最大バージョンより小さい未適用マイグレーションを後から足すと起動が壊れる**（out-of-order）。よって:

- ファイル名は **着手時点の最小空き整数**。`ls backend/migrations/` で最大番号を確認し +1。**現状の最新は `0005_search.sql` なので暫定 `0006_digests.sql`**。
- 並行作業（apalis 移行ジョブテーブル等）が先に `0006` を取った場合は本機能を `0007` 以降へ繰り上げる。
- 既存マイグレーションは**編集しない**（追記のみ）。

本書では以降 **`0006_digests.sql`** と表記する（採番は着手時に再確認）。

### 4.2 スキーマ

新規ファイル **`backend/migrations/0006_digests.sql`**:

```sql
-- AI daily digest. One row per calendar day (UTC). The digest aggregates the
-- previous 24h of unread articles into topic-grouped Markdown via Claude and is
-- cached here so re-requests for the same day cost no tokens (mirrors the
-- summary/translation caching in the articles slice).
CREATE TABLE IF NOT EXISTS digests (
    -- Calendar day the digest covers (UTC). Single-user app => one digest/day.
    date          DATE PRIMARY KEY,
    -- Generated digest body in Markdown. May be the "no new articles" note
    -- (see EMPTY_DIGEST_MD) when there was nothing to summarize.
    markdown      TEXT NOT NULL,
    -- Anthropic model id used (e.g. "claude-sonnet-4-6"), or "(none)" when the
    -- digest was the empty-state note and no LLM call was made.
    model         TEXT NOT NULL,
    -- Number of source articles fed to the model. 0 for the empty-state note.
    article_count INTEGER NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- "latest" lookups (GET /api/digest/latest) order by date DESC; the PK index
-- already serves this, but make the intent explicit for range scans.
CREATE INDEX IF NOT EXISTS idx_digests_date_desc ON digests (date DESC);
```

設計判断:
- **`date` を PK** にして「1 日 1 本」を DB 制約で保証。`ON CONFLICT (date) DO UPDATE` で「無ければ挿入、有れば上書き」を 1 クエリで表現（手動 refresh の上書きにも対応）。
- **`model` 列**: 監査・将来のモデル移行追跡用。empty-state では `"(none)"`。
- **`article_count` 列**: UI に「N 件の記事から生成」を出すため、かつ empty 判定の根拠。
- 他テーブル（`feeds`/`articles`）への列追加は無い。材料は読み取りのみ。

---

## 5. バックエンド設計

新スライス **`backend/src/features/digest/`**。5 ファイル構成。

### 5.1 `domain.rs`（値オブジェクト + 純粋ロジック + 単体テスト対象）

```rust
use chrono::NaiveDate;
use serde::Serialize;

/// DB 行に対応するダイジェスト。GET API がそのまま JSON で返す。
/// date は NaiveDate → serde で "YYYY-MM-DD" にシリアライズされる。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Digest {
    pub date: NaiveDate,
    pub markdown: String,
    pub model: String,
    pub article_count: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// `?date=YYYY-MM-DD` の値オブジェクト。不正な日付は構築時に弾く。
#[derive(Debug, Clone, Copy)]
pub struct DigestDate(NaiveDate);

impl DigestDate {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, String> {
        let s = raw.as_ref().trim();
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(DigestDate)
            .map_err(|_| "date must be in YYYY-MM-DD format".to_string())
    }
    pub fn date(&self) -> NaiveDate {
        self.0
    }
}

/// LLM に渡す 1 記事ぶんの材料（読み取り射影）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DigestSource {
    pub title: String,
    pub url: String,
    /// summary があれば summary、無ければ本文の先頭。プロンプト材料用。
    pub snippet: String,
}

/// 新着が無かった日に保存する本文（LLM を呼ばずに使う）。
pub const EMPTY_DIGEST_MD: &str = "## 本日の新着記事はありませんでした\n";

/// LLM に渡す入力テキストを組み立てる純粋関数（= 単体テスト対象）。
/// 各記事を「- [タイトル](URL): 抜粋」の Markdown 箇条書きに整形する。
pub fn build_digest_input(items: &[DigestSource]) -> String {
    items
        .iter()
        .map(|it| {
            let snippet = it.snippet.trim();
            format!("- [{}]({}): {}", it.title.trim(), it.url.trim(), snippet)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
```

### 5.2 `repository.rs`（`&PgPool` を取る free async fn、ランタイムクエリのみ）

```rust
use chrono::NaiveDate;
use sqlx::PgPool;

use super::domain::{Digest, DigestSource};
use crate::shared::error::AppResult;

/// 最新（date 降順の先頭）のダイジェストを 1 本返す。
pub async fn get_latest(pool: &PgPool) -> AppResult<Option<Digest>> {
    let row = sqlx::query_as::<_, Digest>(
        "SELECT date, markdown, model, article_count, created_at
         FROM digests ORDER BY date DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 指定日のダイジェストを返す。
pub async fn get_by_date(pool: &PgPool, date: NaiveDate) -> AppResult<Option<Digest>> {
    let row = sqlx::query_as::<_, Digest>(
        "SELECT date, markdown, model, article_count, created_at
         FROM digests WHERE date = $1",
    )
    .bind(date)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// ダイジェストを保存（無ければ挿入、有れば上書き）。
pub async fn upsert(
    pool: &PgPool,
    date: NaiveDate,
    markdown: &str,
    model: &str,
    article_count: i32,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO digests (date, markdown, model, article_count, created_at)
           VALUES ($1, $2, $3, $4, now())
           ON CONFLICT (date) DO UPDATE
             SET markdown = EXCLUDED.markdown,
                 model = EXCLUDED.model,
                 article_count = EXCLUDED.article_count,
                 created_at = now()"#,
    )
    .bind(date)
    .bind(markdown)
    .bind(model)
    .bind(article_count)
    .execute(pool)
    .await?;
    Ok(())
}

/// 直近 `hours` 時間の未読記事を材料として読み取る（読み取り専用クロステーブル参照）。
/// snippet は summary 優先、無ければ本文先頭 800 文字。新しい順・最大 100 件で
/// トークンを抑制する。published_at が NULL の記事は created_at で代替。
pub async fn recent_unread(pool: &PgPool, hours: i32) -> AppResult<Vec<DigestSource>> {
    let rows = sqlx::query_as::<_, DigestSource>(
        r#"SELECT title,
                  url,
                  COALESCE(NULLIF(summary, ''), LEFT(content, 800)) AS snippet
           FROM articles
           WHERE is_read = false
             AND COALESCE(published_at, created_at) >= now() - make_interval(hours => $1)
           ORDER BY COALESCE(published_at, created_at) DESC
           LIMIT 100"#,
    )
    .bind(hours)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
```

> **`articles` を読むことの正当化**: ダイジェストは記事横断の集約 read であり、`feed_overview`（feeds+articles JOIN read）や `instapaper::get_article_ref`（articles 読み取り）と同じ「読み取りのクロステーブル参照」。`articles` の **書き込み所有は移していない**ので越境共通レイヤーには当たらない。`query!` コンパイル時マクロは使わず `query`/`query_as` のみ。

### 5.3 `shared/llm` への `digest` メソッド追加（唯一の抽象境界を拡張）

`backend/src/shared/llm/mod.rs` に型と trait メソッドを **追記**:

```rust
#[derive(Debug, Clone)]
pub struct DigestRequest {
    /// build_digest_input() が組み立てた記事一覧（Markdown 箇条書き）。
    pub items: String,
    /// 出力言語（例 "ja"）。
    pub target_lang: String,
}

// LlmClient trait に 1 メソッド追加:
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn summarize(&self, req: SummarizeRequest) -> AppResult<String>;
    async fn translate(&self, req: TranslateRequest) -> AppResult<String>;
    async fn digest(&self, req: DigestRequest) -> AppResult<String>; // ← 追加
}
```

`backend/src/shared/llm/anthropic.rs` の `impl LlmClient for AnthropicClient` に **追記**（既存 private `complete(system, user)` を再利用）:

```rust
async fn digest(&self, req: DigestRequest) -> AppResult<String> {
    let system = format!(
        "You are an editor compiling a daily news digest in {}. Group the \
         following articles by topic. For each topic, write a short heading \
         (Markdown '## ') and 2-4 concise bullet points capturing the key \
         points. Keep each article's source link. Output Markdown only.",
        req.target_lang
    );
    self.complete(&system, &req.items).await
}
```

> trait は **新設しない**。既存の唯一の境界 `LlmClient` に `digest` を足すだけ（要約/翻訳と同型）。これは CLAUDE.md「抽象境界は `shared/llm` のみ」に整合する拡張。`complete` の `max_tokens` は現状 1024 固定で、長い digest は切り詰められうる（§11 に緩和策）。

### 5.4 `service.rs`（`&AppState` を取り repository + LLM + メールを統合）

```rust
use chrono::{NaiveDate, Utc};

use super::domain::{build_digest_input, Digest, EMPTY_DIGEST_MD};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{DigestRequest, LlmClient};
use crate::shared::state::AppState;

/// 材料に使う遡及時間。要望どおり 24h。
const WINDOW_HOURS: i32 = 24;

/// config から Anthropic クライアントを作る。未設定なら NotEnabled。
/// （articles/service.rs::llm_client と同型）
fn llm_client(state: &AppState) -> AppResult<AnthropicClient> {
    let key = state
        .config
        .anthropic_api_key
        .clone()
        .ok_or_else(|| AppError::NotEnabled("ANTHROPIC_API_KEY is not set".into()))?;
    Ok(AnthropicClient::new(
        state.http.clone(),
        key,
        state.config.anthropic_model.clone(),
    ))
}

pub async fn get_latest(state: &AppState) -> AppResult<Digest> {
    repository::get_latest(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn get_by_date(state: &AppState, date: NaiveDate) -> AppResult<Digest> {
    repository::get_by_date(&state.db, date)
        .await?
        .ok_or(AppError::NotFound)
}

/// 指定日のダイジェストを生成して保存し、生成結果を返す（上書き＝冪等）。
/// 順序: (1) APIキー有り? 無ければ NotEnabled、(2) 材料取得、(3) 空なら
///       LLM を呼ばず empty-note を保存、(4) 非空なら Claude で生成して保存。
/// 戻り値は保存後の行（GET と同じ形）。
pub async fn generate_for_date(state: &AppState, date: NaiveDate) -> AppResult<Digest> {
    // 機能ゲートを先に判定（材料の有無に関わらず未設定なら 503）。
    let client = llm_client(state)?;

    let sources = repository::recent_unread(&state.db, WINDOW_HOURS).await?;
    let count = sources.len() as i32;

    let (markdown, model) = if sources.is_empty() {
        (EMPTY_DIGEST_MD.to_string(), "(none)".to_string())
    } else {
        let items = build_digest_input(&sources);
        let md = client
            .digest(DigestRequest {
                items,
                target_lang: state.config.digest_lang.clone(),
            })
            .await?;
        (md, state.config.anthropic_model.clone())
    };

    repository::upsert(&state.db, date, &markdown, &model, count).await?;

    // 任意: SMTP 設定が揃い、かつ実際に生成された（empty でない）ときだけ送信。
    if count > 0 {
        if let Err(e) = super::email::maybe_send(state, date, &markdown).await {
            tracing::warn!(error = %e, "digest email send failed (non-fatal)");
        }
    }

    repository::get_by_date(&state.db, date)
        .await?
        .ok_or(AppError::NotFound)
}

/// scheduler 用: 当日分が未生成なら生成、在れば何もしない（再課金しない）。
pub async fn ensure_today(state: &AppState) -> AppResult<()> {
    let today = Utc::now().date_naive();
    if repository::get_by_date(&state.db, today).await?.is_some() {
        return Ok(());
    }
    generate_for_date(state, today).await.map(|_| ())
}
```

> `generate_for_date` は handler（手動 refresh）と scheduler（`ensure_today` 経由）の両方から呼ばれるので `-D warnings` でも未使用にならない。HTTP 呼び出しは `AnthropicClient`（trait 実装）に閉じ、本スライスに新しい trait/dyn は足さない。

### 5.5 `email.rs`（スライス内・任意 SMTP 送信。SMTP を後回しにするなら空実装可）

```rust
use chrono::NaiveDate;

use crate::shared::error::AppResult;
use crate::shared::state::AppState;

/// config に SMTP 設定（host / from / to）が揃っているときだけメール送信する。
/// 揃っていなければ Ok(()) で静かにスキップ（任意機能）。
/// lettre 依存を追加したくない初期段階は、本関数を「常に Ok(()) を返す」
/// スタブにしておき、SMTP は後続タスクで有効化してよい（§11）。
pub async fn maybe_send(state: &AppState, date: NaiveDate, markdown: &str) -> AppResult<()> {
    let cfg = &state.config;
    let (Some(host), Some(from), Some(to)) =
        (cfg.smtp_host.as_ref(), cfg.digest_email_from.as_ref(), cfg.digest_email_to.as_ref())
    else {
        return Ok(()); // 未設定 → 送らない
    };

    // ↓ lettre を使った実装例（依存追加が前提）。詳細 API は lettre のドキュメントで確認。
    // use lettre::{Message, AsyncTransport, AsyncSmtpTransport, Tokio1Executor};
    // let email = Message::builder()
    //     .from(from.parse().map_err(|e| AppError::Other(anyhow::anyhow!("{e}")))?)
    //     .to(to.parse().map_err(|e| AppError::Other(anyhow::anyhow!("{e}")))?)
    //     .subject(format!("Daily Digest {date}"))
    //     .body(markdown.to_string())
    //     .map_err(|e| AppError::Other(anyhow::anyhow!("{e}")))?;
    // let creds = ...; // smtp_username / smtp_password
    // let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?
    //     .port(cfg.smtp_port).credentials(creds).build();
    // mailer.send(email).await.map_err(|e| AppError::Upstream(e.to_string()))?;

    let _ = (host, from, to, date, markdown); // スタブ時の未使用警告抑止
    Ok(())
}
```

> 送信失敗は **致命傷にしない**（`service.rs` 側で `warn!` してダイジェスト自体は成功扱い）。SMTP は完全に任意で、env が無ければ実行経路に入らない。

### 5.6 `mod.rs`（routes）と scheduler 起動

`backend/src/features/digest/mod.rs`:

```rust
pub mod domain;
pub mod email;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/digest/latest", get(handler::latest))
        // GET /api/digest?date=YYYY-MM-DD、および POST /api/digest/refresh（当日生成）。
        .route("/api/digest", get(handler::by_date))
        .route("/api/digest/refresh", post(handler::refresh))
}
```

`backend/src/shared/scheduler.rs` に **生成ループを追記**（既存 `spawn` はそのまま）:

```rust
use chrono::{Timelike, Utc};

/// Daily digest loop. Wakes hourly; when the UTC hour matches the configured
/// hour and digests are enabled, ensures today's digest exists (idempotent).
pub fn spawn_digest(state: AppState) {
    if !state.config.digest_enabled {
        tracing::info!("daily digest disabled (DIGEST_ENABLED is not true)");
        return;
    }
    let target_hour = state.config.digest_hour_utc;
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(3600));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if Utc::now().hour() == target_hour {
                tracing::info!("daily digest tick");
                if let Err(e) = crate::features::digest::service::ensure_today(&state).await {
                    tracing::error!(error = %e, "daily digest generation failed");
                }
            }
        }
    });
}
```

`backend/src/main.rs` に **1 行追加**（`scheduler::spawn(state.clone());` の直後）:

```rust
scheduler::spawn_digest(state.clone());
```

`backend/src/features/mod.rs` に **2 行追加**:

```rust
pub mod digest; // 既存 pub mod 群に追加
// router() の .merge チェーンに追加:
        .merge(digest::routes())
```

既存スライス（feeds/articles/instapaper/...）は一切触らない。触れるのは横断インフラ（`shared/llm`・`shared/scheduler`・`shared/config`）と合成点（`features/mod.rs`・`main.rs`）のみ。

### 5.7 `handler.rs`（axum ハンドラ）

```rust
use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use super::domain::{Digest, DigestDate};
use super::service;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn latest(State(state): State<AppState>) -> AppResult<Json<Digest>> {
    Ok(Json(service::get_latest(&state).await?))
}

#[derive(Debug, Deserialize)]
pub struct DateQuery {
    pub date: String,
}

pub async fn by_date(
    State(state): State<AppState>,
    Query(q): Query<DateQuery>,
) -> AppResult<Json<Digest>> {
    let date = DigestDate::parse(q.date).map_err(AppError::Validation)?;
    Ok(Json(service::get_by_date(&state, date.date()).await?))
}

pub async fn refresh(State(state): State<AppState>) -> AppResult<Json<Digest>> {
    let today = chrono::Utc::now().date_naive();
    Ok(Json(service::generate_for_date(&state, today).await?))
}
```

### 5.8 `config.rs` への追加（env マッピング）

`AppConfig` に追記（既存フィールドの並びに足す）:

```rust
    /// AI デイリーダイジェストの日次生成を有効化するか。
    pub digest_enabled: bool,
    /// 日次生成を走らせる UTC 時刻（0-23）。既定 21（= JST 翌 6 時）。
    pub digest_hour_utc: u32,
    /// ダイジェストの出力言語。既定 "ja"。
    pub digest_lang: String,
    /// 任意 SMTP 設定（揃っていれば生成後にメール送信）。
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub digest_email_from: Option<String>,
    pub digest_email_to: Option<String>,
```

`from_env` に追記:

```rust
        let digest_enabled = std::env::var("DIGEST_ENABLED")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let digest_hour_utc = std::env::var("DIGEST_HOUR_UTC")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|h| *h <= 23)
            .unwrap_or(21);
        let digest_lang = std::env::var("DIGEST_LANG").unwrap_or_else(|_| "ja".to_string());
        let smtp_host = std::env::var("SMTP_HOST").ok().filter(|v| !v.is_empty());
        let smtp_port = std::env::var("SMTP_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(587);
        let smtp_username = std::env::var("SMTP_USERNAME").ok().filter(|v| !v.is_empty());
        let smtp_password = std::env::var("SMTP_PASSWORD").ok().filter(|v| !v.is_empty());
        let digest_email_from = std::env::var("DIGEST_EMAIL_FROM").ok().filter(|v| !v.is_empty());
        let digest_email_to = std::env::var("DIGEST_EMAIL_TO").ok().filter(|v| !v.is_empty());
```

（`Self { ... }` の構築にも各フィールドを追加すること。）`.env.example` にも同名キーを追記する。

### 5.9 AppError の使い分け（`error.rs` は不編集）

| 状況 | バリアント | HTTP | レスポンス `error` 文字列（Display） |
|---|---|---|---|
| `latest`/`by_date` で該当ダイジェストが無い | `NotFound` | 404 | `resource not found` |
| `?date=` が `YYYY-MM-DD` でない | `Validation` | 400 | `invalid input: date must be in YYYY-MM-DD format` |
| `refresh` 時に `ANTHROPIC_API_KEY` 未設定 | `NotEnabled` | 503 | `feature not yet enabled: ANTHROPIC_API_KEY is not set` |
| Claude API 障害・非 2xx | `Upstream` | 502 | `upstream request failed: anthropic 5xx: ...` |
| DB エラー | `Database`（`?` で自動 `From`） | 500 | `internal error` |

> SMTP 送信失敗は HTTP エラーにしない（`warn!` のみ）。機能ゲート（APIキー）は材料取得より**先に**判定し、未設定なら材料の有無に関わらず 503。新バリアントは追加しない。

---

## 6. フロントエンド設計

### 6.1 `lib/api.ts` への追加（型 1 + メソッド 3）

型を追加（backend JSON をミラー）:

```ts
export interface Digest {
  date: string;          // "YYYY-MM-DD"
  markdown: string;
  model: string;
  article_count: number;
  created_at: string;
}
```

`api` オブジェクトにメソッド追加（既存 `http<T>()` を再利用、命名は `動詞+リソース`）:

```ts
  getLatestDigest: () => http<Digest>("/api/digest/latest"),
  getDigest: (date: string) =>
    http<Digest>(`/api/digest?date=${encodeURIComponent(date)}`),
  refreshDigest: () => http<Digest>("/api/digest/refresh", { method: "POST" }),
```

> 404（未生成）は `http<T>()` が throw する。`Digest.tsx` 側で `errorStatus(e) === 404` を「まだダイジェストがありません」表示に振り分ける（`api.ts` の既存 `errorStatus` ヘルパを使う）。

### 6.2 Markdown レンダリング

ダイジェスト本文は Markdown。新規依存 **`marked`** で HTML 化し、**必ず**既存 `sanitizeArticleHtml`（`lib/sanitize.ts`）に通してから `innerHTML` で `prose` コンテナに流す（記事本文と同じ XSS 対策経路）。

```ts
// 例（Digest.tsx 内）:
import { marked } from "marked";
import { sanitizeArticleHtml } from "@/lib/sanitize";
const html = () => sanitizeArticleHtml(marked.parse(digest()?.markdown ?? "") as string);
```

### 6.3 新規ルート `routes/Digest.tsx`

最新ダイジェスト（または `?date=` 指定日）を表示。状態は **ローカル**（`createResource`）。グローバルストア変更は不要。

骨子:
- ルート search param `date` を読む（`@solidjs/router` の `useSearchParams`）。
- `const [digest] = createResource(() => searchParams.date ?? "__latest__", (key) => key === "__latest__" ? api.getLatestDigest() : api.getDigest(key));`
- ヘッダ: 日付（`digest()?.date`）、`badge.tsx` で「{article_count} 件の記事から生成」、`button.tsx`「再生成」（`await api.refreshDigest()` → resource を `refetch`。実行中は disabled、失敗時は 503=APIキー未設定/502=障害をメッセージ表示）。
- 本体: `card.tsx` 内に `<div class="prose dark:prose-invert" innerHTML={html()} />`。
- エラー分岐: `digest.error` のとき `errorStatus(err) === 404` なら「まだ本日のダイジェストはありません。『再生成』で作成できます」を表示。

### 6.4 ルーティング `index.tsx`

既存 `<Router>` 内に 1 ルート追加:

```tsx
import Digest from "./routes/Digest";
// ...
<Route path="/digest" component={Digest} />
```

ナビ導線（Sidebar/ヘッダのリンク）は二ペインレイアウト（機能 10）の置き場所に 1 リンク足すか、暫定で `App.tsx` ヘッダに `/digest` リンクを 1 つ足す（任意）。`/digest` を直接開けば使える状態であれば足りる。

### 6.5 Ark UI について

本機能で必要な UI は card / button / badge と `prose` 表示のみで、いずれも自前 Tailwind + 既存部品で賄える。**Ark UI 部品は不要**。

---

## 7. API 契約

> すべて `/api` プレフィックス。日付は ISO `YYYY-MM-DD`（UTC 暦日）。

### 7.1 `GET /api/digest/latest` — 最新ダイジェスト
レスポンス（200）:
```json
{
  "date": "2026-06-30",
  "markdown": "## AI\n- [記事タイトル](https://example.com/a): 要点...\n\n## セキュリティ\n- ...",
  "model": "claude-sonnet-4-6",
  "article_count": 12,
  "created_at": "2026-06-30T21:00:03Z"
}
```
エラー:
- 404 `{ "error": "resource not found" }`（まだ 1 本も生成されていない）

### 7.2 `GET /api/digest?date=YYYY-MM-DD` — 指定日ダイジェスト
リクエスト例: `GET /api/digest?date=2026-06-29`
レスポンス（200）: 7.1 と同形
エラー:
- 400 `{ "error": "invalid input: date must be in YYYY-MM-DD format" }`
- 404 `{ "error": "resource not found" }`（その日付の行が無い）

### 7.3 `POST /api/digest/refresh` — 当日分を生成（上書き）
リクエスト: ボディ無し
レスポンス（200）: 生成後の行（7.1 と同形）。新着が無かった日は:
```json
{ "date": "2026-06-30", "markdown": "## 本日の新着記事はありませんでした\n", "model": "(none)", "article_count": 0, "created_at": "..." }
```
エラー:
- 503 `{ "error": "feature not yet enabled: ANTHROPIC_API_KEY is not set" }`（APIキー未設定）
- 502 `{ "error": "upstream request failed: anthropic 500 ..." }`（Claude 障害）

---

## 8. 依存関係

- **本機能が依存する機能**: 機能上の **ハード依存は無い**（`digest` スライスは自己完結）。読み取りで `articles` テーブルを参照するが、これは既存。横断インフラ（`shared/llm`・`shared/scheduler`・`shared/config`）を拡張する。
- **ソフトな協調**:
  - 機能 10（二ペイン）/ ナビ: `/digest` への導線をナビに足せると良い（無くても直接 URL で動く）。
  - 機能 04（ダークテーマ）: `prose dark:prose-invert` で整合（追加作業不要）。
  - 記事要約（`articles`）: 記事に `summary` があれば材料の `snippet` に優先採用される（無くても本文先頭で代替）。
- **本機能をブロックする機能**: 無し。
- 既存スライスへの変更は無し。接触点は `features/mod.rs`（2 行）・`main.rs`（1 行）・`shared/scheduler.rs`（生成ループ追記）・`shared/llm/{mod,anthropic}.rs`（`digest` メソッド追記）・`shared/config.rs`（env 追記）。

---

## 9. テスト計画（TDD）

> 配置方針は既存前例に合わせる: 純粋ロジックは各 `.rs` の `#[cfg(test)] mod tests`、DB を触る往復は `repository.rs` 内の `#[ignore]` テスト（binary crate で `lib.rs` 無しのため `backend/tests/` から内部関数は呼べない）、HTTP 表面は shell スクリプト。

### 9.1 単体テスト（`#[cfg(test)] mod tests` in `domain.rs`、外部 API も DB も不要）

| テスト | 意図 |
|---|---|
| `digest_date_parses_valid` | `"2026-06-30"` を `Ok` にし、`date()` が一致 |
| `digest_date_rejects_bad_format` | `"2026/06/30"` / `"30-06-2026"` / `""` を `Err` |
| `digest_date_rejects_impossible_date` | `"2026-13-40"` を `Err`（chrono が弾く） |
| `build_digest_input_formats_bullets` | 2 件入力が `- [title](url): snippet` 2 行に整形される |
| `build_digest_input_trims_fields` | title/url/snippet の前後空白が除去される |
| `build_digest_input_empty_is_empty_string` | 空配列で空文字を返す |
| `empty_digest_md_is_markdown_heading` | `EMPTY_DIGEST_MD` が `##` で始まる（UI が `prose` で表示できる前提の固定） |

### 9.2 リポジトリ往復テスト（`#[cfg(test)] mod tests` in `repository.rs`、実 DB / `#[ignore]`）

`DATABASE_URL`（`just dev-db` の DB、マイグレーション適用済み）で実 DB に接続。`#[tokio::test]` + `#[ignore]`。`cargo test -- --ignored` で実行。

雛形:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use sqlx::postgres::PgPoolOptions;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL for repo tests");
        PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap()
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres (DATABASE_URL)"]
    async fn digest_upsert_get_latest_roundtrip() {
        let pool = pool().await;
        let d1 = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2000, 1, 2).unwrap();

        upsert(&pool, d1, "## a", "m1", 3).await.unwrap();
        let got = get_by_date(&pool, d1).await.unwrap().expect("row");
        assert_eq!(got.markdown, "## a");
        assert_eq!(got.article_count, 3);

        // 同一日の再 upsert は上書き
        upsert(&pool, d1, "## a2", "m2", 5).await.unwrap();
        let got = get_by_date(&pool, d1).await.unwrap().expect("row");
        assert_eq!(got.markdown, "## a2");
        assert_eq!(got.article_count, 5);

        // latest は date 降順の先頭
        upsert(&pool, d2, "## b", "m3", 1).await.unwrap();
        let latest = get_latest(&pool).await.unwrap().expect("row");
        assert_eq!(latest.date, d2);
    }
}
```

| テスト | 意図 |
|---|---|
| `digest_upsert_get_latest_roundtrip` | upsert→get_by_date（挿入）→ 再 upsert（上書き）→ latest（date 降順先頭）を network 抜きで自動カバー |
| `recent_unread_returns_only_recent_unread`（任意） | 既読/古い記事を除外し新しい順で返すことを、テストデータを挿入して検証（後片付け込み） |

### 9.3 HTTP スモークテスト（稼働スタックへの shell スクリプト）

`scripts/test/api-digest.sh` を新設（`scripts/test/api-*.sh` と同型。nginx 経由）。**Claude を叩かない範囲**を決定的に検証:

| 手順 / アサーション | 意図 |
|---|---|
| `GET /api/digest?date=not-a-date` → 400 | 日付バリデーション配線 |
| `GET /api/digest/latest` → 200 もしくは 404（どちらも許容、JSON 形を assert） | スライス合成 + 取得経路 |
| `ANTHROPIC_API_KEY` 未設定環境で `POST /api/digest/refresh` → 503 | `NotEnabled` を **APIキー判定で先に**返す配線 |

> `POST /api/digest/refresh` の成功パス（実 Claude 呼び出し）と SMTP 送信は CI 自動化しない（ライブ APIキー/SMTP が必要）。手動手順は §10 step 11。

### 9.4 フロント（手動 + 型）
- `tsc`（`just lint`）で `api.ts` / `Digest.tsx` の型整合を確認。`marked` の型も解決すること。
- 手動: `/digest` を開く → 未生成なら 404 文言、「再生成」で生成 → Markdown が `prose` で表示、ダークでも可読。

---

## 10. 実装手順（順序付きチェックリスト）

1. **マイグレーション採番**: `ls backend/migrations/` で最大番号を確認（現状 `0005_search.sql`）。`0006_digests.sql` を §4.2 の SQL で新規作成（既存は触らない）。
2. **shared/llm 拡張（Red 先行可）**: `shared/llm/mod.rs` に `DigestRequest` と trait メソッド `digest` を追加、`anthropic.rs` に実装（§5.3）。`complete` 再利用。
3. **config 拡張**: `shared/config.rs` に digest 関連フィールドと `from_env` 解析を追加（§5.8）。`.env.example` も更新。
4. **ドメイン（Red 先行）**: `features/digest/domain.rs` を §5.1 で作成 + §9.1 の `#[cfg(test)] mod tests`。落ちる→実装で Green。
5. **repository**: `repository.rs` を §5.2（`query`/`query_as` のみ）。§9.2 の `#[ignore]` テストも書く。
6. **service**: `service.rs` を §5.4。`llm_client`（NotEnabled）・`generate_for_date`・`ensure_today`。
7. **email**: `email.rs` を §5.5。SMTP を後回しにするならスタブ（常に `Ok(())`）で開始。
8. **handler + mod + 合成**: `handler.rs`（§5.7）、`mod.rs`（§5.6）。`features/mod.rs` に `pub mod digest;` と `.merge(digest::routes())`。`shared/scheduler.rs` に `spawn_digest`、`main.rs` に `scheduler::spawn_digest(state.clone());`（§5.6）。
9. **ビルド & lint**: `just lint`（clippy `-D warnings` / tsc）。SMTP 実装するなら `lettre` を `Cargo.toml` に追加。
10. **DB & テスト**: `just dev-db` → 起動で自動 migrate（または `just migrate`）→ `cargo test`（単体）→ `DATABASE_URL=... cargo test -- --ignored`（往復）。`scripts/test/api-digest.sh` を作成・`chmod +x`・実行。
11. **手動 E2E**: `ANTHROPIC_API_KEY` を設定して起動 → `POST /api/digest/refresh` → `GET /api/digest/latest` で Markdown を確認。`DIGEST_ENABLED=true` + `DIGEST_HOUR_UTC` を直近時刻にして scheduler 経由生成も確認。SMTP 設定時はメール受信を目視。
12. **フロント**: `lib/api.ts`（型 + 3 メソッド、§6.1）、`marked` 追加、`routes/Digest.tsx`（§6.3）、`index.tsx` にルート、任意でナビリンク。`just lint` の tsc を通す。
13. **コミット**: マイグレーション・スライス・shared 拡張・スクリプト・フロントをまとめて。`.env`/秘密はコミットしない。

---

## 11. リスク・未決事項・代替案

- **【要確認】`max_tokens` による digest 切り詰め**: `anthropic.rs::complete` は `max_tokens: 1024` 固定。記事が多い日は digest が途中で切れうる。**緩和策**: `complete` を `complete_with(system, user, max_tokens)` に小改修し、`digest` だけ 2048〜4096 を渡す（既存 `summarize`/`translate` は 1024 のまま）。あるいは材料の `LIMIT 100` / `LEFT(content, 800)` をさらに絞る。MVP は 1024 で開始し、運用を見て調整。
- **トークン費用と材料量**: 全フィード横断・直近 24h・最大 100 件を 1 回の生成にまとめるためコストは記事数に比例。`LIMIT` と `snippet` 長で上限を固定済み。フィード/フォルダ別ダイジェストは非スコープ（将来、`recent_unread` に絞り込み引数を足すだけで拡張可）。
- **生成タイミングの重複・取りこぼし**: `spawn_digest` は「毎時 tick で `hour == DIGEST_HOUR_UTC` のとき生成」方式。`ensure_today` が当日分の存在を確認してから生成するので、同一時間帯に複数 tick が起きても二重生成しない（in-process では tick は毎時 1 回）。サーバが該当時刻に停止していた日は当日分が欠落するが、起動後の次回 tick が同日の対象時刻に入れば生成される（または `POST /api/digest/refresh` で手動補完）。日跨ぎ・タイムゾーンは **UTC 暦日**に統一（`date_naive()`）。JST 表示が要るなら `DIGEST_HOUR_UTC=21`（JST 翌 06:00 相当）を既定とする。
- **SMTP は完全任意**: env（`SMTP_HOST`/`DIGEST_EMAIL_FROM`/`DIGEST_EMAIL_TO`）が揃わなければ送信経路に入らない。`lettre` 依存を当初入れたくなければ `email::maybe_send` をスタブ（常に `Ok(())`）で出荷し、後続で有効化。送信失敗はダイジェスト生成を失敗させない（`warn!` のみ）。`lettre` の正確な API（`AsyncSmtpTransport`・`starttls_relay` vs `relay`・認証）は実装時にドキュメントで確認。
- **`shared/llm` trait 拡張の影響**: `LlmClient` にメソッドを足すと、もし他にモック実装があれば追従が必要。現状 trait 実装は `AnthropicClient` のみ（テストモックは未存在）なので影響なし。新規モックを作る場合は `digest` も実装すること。
- **Markdown レンダリングの XSS**: LLM 出力は信頼境界外として扱い、`marked` の HTML 化結果を **必ず** `sanitizeArticleHtml` に通す（記事本文と同じ経路）。生 `innerHTML` を sanitize 無しで使わない。
- **空ダイジェストの扱い**: 新着 0 件の日は LLM を呼ばず `EMPTY_DIGEST_MD` を保存（`model="(none)"`, `article_count=0`）。これにより「その日は生成済み（再課金しない）」と「新着なし」を区別できる。`POST /api/digest/refresh` で APIキー未設定なら、新着の有無に関わらず先に 503（機能ゲート優先・テスト決定性のため）。
- **マイグレーション番号衝突（apalis 等）**: §4.1 のとおり out-of-order は起動を壊す。**着手直前に最新番号を再確認**し、`0006` が埋まっていれば繰り上げる。
