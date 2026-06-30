# 22 Ask Claude（記事への対話 Q&A）

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッションの実装者。本書だけで着手・完了できるよう、再利用資産・SQL・関数シグネチャ・ルート文字列・フロント変更まで具体化する。
> **重要な但し書き（マイグレーション番号）**: 本書では新規マイグレーションを **`0006_article_notes.sql`** と表記するが、これは執筆時点（最新 = `0005_search.sql`）の暫定採番である。**着手前に必ず `ls backend/migrations/` で最新番号を確認し、最小空き整数 +1 を採ること**（apalis 移行や他機能が先にマージされて番号が進んでいる可能性がある。§4.1）。

---

## 1. 概要

本機能はこのリーダーの **旗艦機能**。記事を読みながら **Claude に自由に質問できる対話 Q&A** を追加する。ユーザーが記事ビューでチャット欄に質問を入力すると、サーバは **その記事本文を context に詰めて** Claude（Messages API）へ送り、回答を返す。会話は追問（マルチターン）でき、ユーザーは「この記事の論点は？」「3 行でまとめて」「この主張の根拠は？」のように深掘りできる。

要約（`summarize`）/翻訳（`translate`）が **片方向の単発処理** なのに対し、Ask は **マルチターンの対話**である点が新しい。既存の `shared/llm`（唯一の抽象境界）に **`chat` メソッドを1つ足して** 対話を表現し、記事本文の取り回し・会話の検証・NotEnabled ゲート・任意の DB 保存を **新スライス `ask` 1枚** に閉じる。

設計の柱:

- **新スライス `backend/src/features/ask/`**（`domain` / `repository` / `service` / `handler` / `mod`）を1枚追加し、`features/mod.rs` に `.merge(ask::routes())` を1行足す。**既存スライス（articles 等）は一切触らない。**
- **AI 呼び出しは `shared/llm` を再利用**。Anthropic アダプタ（`anthropic.rs`）に多ターン対応の `chat` を足す。`ANTHROPIC_API_KEY` 未設定なら要約/翻訳と同型で `AppError::NotEnabled`（503）を返す。
- **context は記事本文を優先**（`full_content` 優先 → 無ければ `content`。§4.3）。長文はトークン上限を意識して **純粋関数で切り詰める**（テスト可能）。
- **任意の永続化**: リクエストの `save: true` で Q&A を新テーブル `article_notes` に保存し、`GET /api/articles/{id}/notes` で後から読み返せる。保存は **オプトイン**（既定は保存しない＝トークンも DB 行も消費しない）。
- **拡張: 複数記事の横断 Ask**。`POST /api/articles/ask { ids[], messages[] }` で複数記事を1つの context にまとめ、記事をまたいだ比較・要約を質問できる。

エンドポイント:

| メソッド | パス | 役割 |
|---|---|---|
| `POST` | `/api/articles/{id}/ask` | 単一記事への対話 Q&A（旗艦） |
| `POST` | `/api/articles/ask` | 複数記事の横断 Q&A（拡張） |
| `GET` | `/api/articles/{id}/notes` | 保存済み Q&A 履歴の取得（`save` 利用時） |

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）

- 新スライス `backend/src/features/ask/`（5ファイル）。
- `shared/llm` の **最小拡張**: `ChatMessage` / `ChatRequest` 型と `LlmClient::chat(&self, ChatRequest) -> AppResult<String>` の追加、`anthropic.rs` での実装。**唯一の抽象境界の範囲内**なので方針逸脱ではない（§3 で正当化）。
- `POST /api/articles/{id}/ask`: 記事本文 + 会話履歴 → Claude 応答。`ANTHROPIC_API_KEY` 未設定で `NotEnabled`、記事不在で `NotFound`、会話不正で `Validation`。
- `POST /api/articles/ask`: 複数記事 `ids[]` を context にまとめて Q&A（拡張）。
- 任意保存: リクエスト `save: true` のとき直近の user 質問と assistant 回答を `article_notes` に追記。
- `GET /api/articles/{id}/notes`: 保存済み履歴を時系列で返す。
- マイグレーション **`0006_article_notes.sql`**（暫定番号。§4.1）: `article_notes` テーブル。
- フロント: `lib/api.ts` に型3 + メソッド3、チャット UI コンポーネント `components/article/ArticleAsk.tsx` を新設し、`ArticleDetail.tsx` に**1行で差し込む**（既存の要約/翻訳ボタン群の下）。
- 会話検証・context 切り詰め・role 分類を **純粋関数** に切り出し、外部 API を叩かずに TDD（§9.1）。

### 非スコープ（本機能では実装しない）

- **ストリーミング応答**（SSE / トークン逐次表示）。MVP は応答完了後にまとめて返す（`anthropic.rs` の既存 `complete` と同じ非ストリーミング方式）。将来拡張は §11。
- **会話の自動再開・セッション永続**。`save` は「読み返し用のログ保存」であって、保存履歴を次回リクエストへ自動で前置きはしない（クライアントが `messages[]` を毎回送る、ステートレス契約）。
- **記事本文の抽出強化**（`full_content` の生成）。ロードマップ「記事本文の抽出強化」の担当。本機能は `full_content` カラムが存在すれば優先利用するだけ（§4.3）。
- **トークン課金メータ / レート制限 UI**。
- **要約/翻訳スライス（articles）の変更**。Ask は独立スライス。articles の `content` を**読み取り専用**で参照するのみ。
- **複数ユーザ / 会話の権限管理**（単一ユーザ前提）。

---

## 3. 既存実装の再利用

実ファイルを確認済み。以下を再利用し、車輪の再発明をしない。

| 再利用資産 | 実体（確認済みファイル） | 本機能での使い方 |
|---|---|---|
| LLM 抽象境界 + Anthropic アダプタ | `backend/src/shared/llm/mod.rs`（`LlmClient` trait, `SummarizeRequest`/`TranslateRequest`）、`anthropic.rs`（`AnthropicClient::complete` が `model`/`max_tokens`/`system`/`messages` の JSON を組み、`x-api-key`/`anthropic-version` ヘッダで POST、content 配列の先頭 text を取り出す） | **唯一の抽象境界**に `chat` を1メソッド追加（§5.0）。`messages` 配列を多ターンにするだけで `complete` のパターンを踏襲。trait を増やさず既存 trait を拡張 |
| 任意機能 = `NotEnabled` パターン | `articles/service.rs::llm_client()`（`anthropic_api_key` 無し時に `NotEnabled("ANTHROPIC_API_KEY is not set")`、`AnthropicClient::new(state.http.clone(), key, state.config.anthropic_model.clone())`） | **同じ `llm_client` 構築ロジックを ask/service.rs に複製**（articles を import せず自スライス内に閉じる。3 行の小関数なので重複許容） |
| `AppState { db, config, http }` | `backend/src/shared/state.rs`（`#[derive(Clone)]`、`config: Arc<AppConfig>`、`http: reqwest::Client`） | `state.config.anthropic_api_key` / `anthropic_model` / `state.http` をそのまま使う |
| `AppError` 6 バリアント | `backend/src/shared/error.rs`（`NotFound`/404, `Validation(String)`/400, `NotEnabled(String)`/503, `Upstream(String)`/502, `Database`/500, `Other`/500。`IntoResponse` で `Json({"error": <Display>})`） | 新バリアントを足さず既存で表現（§5.7）。**`error.rs` は編集しない** |
| 主キー newtype | `articles/domain.rs::ArticleId`（`#[derive(... sqlx::Type)] #[sqlx(transparent)]`、`pub ArticleId(pub Uuid)`） | ハンドラのパス抽出は **素の `Uuid`** で受け、repository へ bind する（instapaper スライスが `articles` を素の `Uuid` で読む前例に倣う） |
| クロステーブル read を自スライス内 SQL で完結 | `instapaper/repository.rs::get_article_ref`（`SELECT url, title FROM articles WHERE id=$1` の読み取り専用射影）、`feed_overview`（feeds+articles JOIN 読み） | `ask/repository.rs` で `SELECT title, content FROM articles WHERE id=$1` を読み取り専用に持つ。**articles の書き込み所有は移さない** |
| UUID はアプリ生成 | `0001_init.sql`（`feeds.id`/`articles.id` は `UUID PRIMARY KEY` で **DB デフォルト無し**＝アプリ側で生成） | `article_notes.id` も `uuid::Uuid::new_v4()` をアプリで生成して bind（pgcrypto 拡張に依存しない） |
| スライス構成 + `routes()` | `articles/mod.rs`（`fn routes() -> Router<AppState>`、`.route("/api/articles/{id}/summarize", post(...))`） | 同じ5ファイル構成で `ask` を作る。`{id}` パスパラメータの書式（axum 0.8 は `{id}`）を踏襲 |
| `features/mod.rs` の合成 | `pub mod ...;` + `.merge(...::routes())`（既存8スライスを `router()` で merge） | `pub mod ask;` と `.merge(ask::routes())` を1行ずつ追加。既存スライスは触らない |
| sqlx ランタイムクエリ | `articles/repository.rs`（`query_as::<_, T>` / `fetch_optional().ok_or(AppError::NotFound)` / `query(...).bind(...).execute`） | `article_notes` の INSERT / SELECT、記事 context の SELECT に同型を使う。`query!` マクロは使わない |
| フロント API クライアント | `frontend/src/lib/api.ts`（`http<T>()`、`api` オブジェクトに `動詞+リソース` 命名でメソッド集約。既存 `getArticle`/`summarize`/`translate`/`saveForLater`） | `http<T>()` を再利用し3メソッド追加（§6.1） |
| 記事ビュー（差し込み先） | `frontend/src/components/article/ArticleDetail.tsx`（`createResource(() => props.id, api.getArticle)`、要約/翻訳/後で読むの `<Button>` 群、`prose prose-sm dark:prose-invert` で本文描画） | `ArticleAsk` を本文ブロックの末尾に `<ArticleAsk articleId={a().id.…} />` 1行で差し込む |
| 自前 UI 部品 | `frontend/src/components/ui/`（`button.tsx`/`card.tsx`/`input.tsx`/`badge.tsx`、`cn(@/lib/utils)`、oklch トークン `bg-background`/`text-muted-foreground` 等） | チャット UI は `Input` + `Button` + `Card` を組み合わせる。新規 UI 部品は不要 |
| HTTP スモークテストの慣習 | `scripts/test/api-*.sh`（稼働スタックに curl、HTTP コードと JSON キーを assert） | `scripts/test/api-ask.sh` を同型で新設（§9.3） |

> **`shared/llm` を編集することの正当化**: CLAUDE.md は「抽象化（trait）は差し替える具体的理由が見えている境界だけ。現状その対象は `shared/llm` のみ」と明記している。`chat` 追加はこの**唯一の sanctioned 境界の内側**での拡張であり、新しい trait/dyn を増やすものではない。要約/翻訳と同じ `LlmClient` に多ターン能力を足すのは境界の本来の目的（プロバイダ差し替え・テストのモック）に合致する。**articles スライスや他の機能スライスは一切触らない。**

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方（必読）

`main.rs` 起動時の `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を呼んでいないため、**適用済み最大バージョンより小さい未適用マイグレーションを後から足すと `VersionMissing`（out-of-order）で起動が壊れる**（家庭内サーバの永続 DB で実害）。

**ルール**:
- 着手直前に `ls backend/migrations/` で最大番号を確認し、**最小空き整数（最大 +1）**を採る。執筆時点の最新は `0005_search.sql` なので暫定で **`0006_article_notes.sql`**。
- 既存マイグレーション（`0001`〜`0005`）は**編集しない**（追記のみ）。
- 並行開発中の apalis 移行や他機能が `0006` を先取りしていたら `0007` 以降へ繰り上げる。

本書では以下、ファイルを **`0006_article_notes.sql`**（= 確認後の最小空き整数）と表記する。

### 4.2 スキーマ

新規ファイル **`backend/migrations/0006_article_notes.sql`**:

```sql
-- Ask Claude: optional persisted Q&A log per article.
-- Only written when a request sets save=true; the chat itself is stateless
-- (the client resends the full messages[] each turn). This table is a
-- read-back log, not a session store.
CREATE TABLE IF NOT EXISTS article_notes (
    id          UUID PRIMARY KEY,                       -- app-generated (uuid::Uuid::new_v4)
    article_id  UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    role        TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content     TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Fetch a single article's notes in chronological order.
CREATE INDEX IF NOT EXISTS idx_article_notes_article_id
    ON article_notes (article_id, created_at);
```

設計判断:
- **`id` はアプリ生成**（`0001_init.sql` の `feeds`/`articles` が DB デフォルト無しの `UUID PRIMARY KEY` である慣習に合わせ、pgcrypto / uuid-ossp 拡張に依存しない）。`uuid::Uuid::new_v4()` を Rust で生成して bind する。
- **`role` を `user`/`assistant` に CHECK 制約**（Anthropic Messages API の role と一致。`system` は別扱いなので保存対象外）。
- **`ON DELETE CASCADE`**: 記事が消えたら紐づく Q&A も消す（孤児行を残さない。`articles` の既存 FK 慣習と同じ）。
- **複合インデックス `(article_id, created_at)`**: `GET /notes` の「記事 id で絞り → 時系列ソート」を1インデックスで賄う。
- 他テーブル（`feeds`/`articles`）への列追加は無い。Ask は articles を**読み取り専用**で参照するだけで、`full_content` 等の新列は本機能では足さない（§4.3）。

### 4.3 context のソース（`full_content` 優先）

Claude へ渡す記事本文は次の優先順で解決する:

1. `articles.full_content`（**存在し、空でなければ**）— ロードマップ「記事本文の抽出強化」が将来追加する列。
2. `articles.content` — 現状フィードの content/summary をそのまま入れている既存列。

**現状 `full_content` カラムは未実装**（`0001_init.sql` に無い）。よって本機能の SQL は当面 `content` のみを SELECT する。`full_content` が将来追加されたら、repository の SELECT を `COALESCE(NULLIF(full_content, ''), content)` に1行で差し替えるだけで優先利用に切り替わる（§5.2 にコメントで明記）。**本機能では `full_content` 列を作らない**（抽出強化機能の責務を侵さない）。

---

## 5. バックエンド設計

### 5.0 `shared/llm` の拡張（唯一の抽象境界・最小差分）

`backend/src/shared/llm/mod.rs` に多ターン型と trait メソッドを追加（既存 `SummarizeRequest`/`TranslateRequest`/`summarize`/`translate` は不変）:

```rust
/// 1 ターン分の会話メッセージ。role は "user" | "assistant"（system は別フィールド）。
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,    // "user" | "assistant"
    pub content: String,
}

/// 多ターン対話の要求。system は会話には入れず別枠で渡す（Anthropic Messages API と同形）。
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub system: String,
    pub messages: Vec<ChatMessage>,
    /// 応答の最大トークン。None なら実装側の既定（2048）を使う。
    pub max_tokens: Option<u32>,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn summarize(&self, req: SummarizeRequest) -> AppResult<String>;
    async fn translate(&self, req: TranslateRequest) -> AppResult<String>;
    /// 多ターン対話。messages は user で始まり user で終わる（呼び出し側で検証済み）。
    async fn chat(&self, req: ChatRequest) -> AppResult<String>;
}
```

`backend/src/shared/llm/anthropic.rs` に実装を追加（既存 `complete` のパターンを踏襲。`chat` は `messages` 配列を多ターンにするだけ）:

```rust
const CHAT_MAX_TOKENS: u32 = 2048;

impl AnthropicClient {
    // 既存の complete(...) はそのまま。多ターン版を追加:
    async fn complete_messages(
        &self,
        system: &str,
        messages: &[super::ChatMessage],
        max_tokens: u32,
    ) -> AppResult<String> {
        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect();

        let body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": msgs,
        });

        // 以降の送信・ステータス判定・content[0].text 抽出は complete() と同一ロジック。
        // （重複を避けたいなら complete をこの関数に委譲する小リファクタも可。挙動は不変。）
        let resp = self
            .http
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Upstream(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Upstream(format!("anthropic {status}: {text}")));
        }

        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Upstream(e.to_string()))?;

        let text = value
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
            })
            .and_then(|b| b.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| AppError::Upstream("unexpected anthropic response shape".into()))?;

        Ok(text.to_string())
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    // 既存 summarize / translate はそのまま。
    async fn chat(&self, req: super::ChatRequest) -> AppResult<String> {
        let max = req.max_tokens.unwrap_or(CHAT_MAX_TOKENS);
        self.complete_messages(&req.system, &req.messages, max).await
    }
}
```

> **trait を増やさない方針との整合**: 新しい trait/dyn は作らない。既存 `LlmClient` に1メソッド足すだけ。テスト用モックを書きたい場合も同じ trait を実装すればよい（境界の本来の目的どおり）。

### 5.1 `ask/domain.rs`（値オブジェクト + 純粋ロジック = 単体テスト対象）

```rust
use serde::{Deserialize, Serialize};

/// context に詰める記事本文の最大文字数。トークン上限と応答余地を考慮した安全弁。
/// （家庭内・単一ユーザ。厳密なトークン計算はせず文字数で素朴に切る。）
pub const MAX_CONTEXT_CHARS: usize = 12_000;
/// 横断 Ask で 1 記事あたりに割り当てる最大文字数（記事数で割って使う）。
pub const MAX_CONTEXT_CHARS_MULTI: usize = 16_000;

/// クライアントから受け取る 1 メッセージ（リクエスト JSON をそのまま deserialize）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AskMessage {
    pub role: String,    // "user" | "assistant"
    pub content: String,
}

/// 記事本文の射影（repository が返す）。
#[derive(Debug, Clone)]
pub struct ArticleContext {
    pub title: String,
    pub body: String,
}

/// 会話列の妥当性検証（純粋関数）。Anthropic Messages API の制約に合わせる:
/// - 空でない
/// - role は "user" | "assistant" のみ
/// - 先頭は user、末尾は user（末尾 user に対して assistant 応答を得る）
/// - user / assistant が交互（同一 role の連続を許さない）
/// - 各 content は空でない
pub fn validate_conversation(messages: &[AskMessage]) -> Result<(), String> {
    if messages.is_empty() {
        return Err("messages must not be empty".into());
    }
    for (i, m) in messages.iter().enumerate() {
        if m.role != "user" && m.role != "assistant" {
            return Err(format!("message[{i}].role must be 'user' or 'assistant'"));
        }
        if m.content.trim().is_empty() {
            return Err(format!("message[{i}].content must not be empty"));
        }
        let expected = if i % 2 == 0 { "user" } else { "assistant" };
        if m.role != expected {
            return Err(format!(
                "messages must alternate starting with user (message[{i}] should be {expected})"
            ));
        }
    }
    if messages.last().map(|m| m.role.as_str()) != Some("user") {
        return Err("the last message must be from the user".into());
    }
    Ok(())
}

/// 文字数で素朴に切り詰める（純粋関数）。char 境界で安全に切る。
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}\n\n[... truncated ...]")
}

/// 単一記事 Ask の system プロンプト（記事本文を埋め込む。純粋関数）。
pub fn build_system_single(ctx: &ArticleContext) -> String {
    let body = truncate_chars(&ctx.body, MAX_CONTEXT_CHARS);
    format!(
        "You are a helpful reading assistant. Answer the user's questions about \
the following article. Base your answers on the article content; if the article \
does not contain the answer, say so. Reply in the same language as the user's question.\n\n\
=== ARTICLE ===\nTitle: {}\n\n{}\n=== END ARTICLE ===",
        ctx.title, body
    )
}

/// 横断 Ask の system プロンプト（複数記事を番号付きで埋め込む。純粋関数）。
pub fn build_system_multi(ctxs: &[ArticleContext]) -> String {
    let per = if ctxs.is_empty() {
        MAX_CONTEXT_CHARS_MULTI
    } else {
        MAX_CONTEXT_CHARS_MULTI / ctxs.len()
    };
    let mut buf = String::from(
        "You are a helpful reading assistant. Answer the user's questions about \
the following articles. You may compare and contrast them. Base your answers on \
the article contents. Reply in the same language as the user's question.\n",
    );
    for (i, c) in ctxs.iter().enumerate() {
        let body = truncate_chars(&c.body, per);
        buf.push_str(&format!(
            "\n=== ARTICLE {} ===\nTitle: {}\n\n{}\n=== END ARTICLE {} ===\n",
            i + 1,
            c.title,
            body,
            i + 1
        ));
    }
    buf
}
```

### 5.2 `ask/repository.rs`（`&PgPool` を取る free async fn、ランタイムクエリのみ）

```rust
use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{ArticleContext, AskMessage};
use crate::shared::error::{AppError, AppResult};

#[derive(Debug, Clone, sqlx::FromRow)]
struct ArticleContextRow {
    title: String,
    content: String,
}

/// 記事本文を読む（読み取り専用射影）。素の Uuid を bind するので articles の
/// domain 型には依存しない。NotFound は呼び出し側で判定したいので Option を返す。
///
/// full_content が将来追加されたら次の1行へ差し替えるだけで優先利用に切り替わる:
///   "SELECT title, COALESCE(NULLIF(full_content, ''), content) AS content FROM articles WHERE id = $1"
pub async fn get_article_context(pool: &PgPool, id: Uuid) -> AppResult<Option<ArticleContext>> {
    let row = sqlx::query_as::<_, ArticleContextRow>(
        "SELECT title, content FROM articles WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ArticleContext { title: r.title, body: r.content }))
}

/// 複数記事をまとめて読む（横断 Ask 用）。渡した順を保つため id 配列で並べ替える。
/// 存在しない id は黙って除外（呼び出し側で空チェック → NotFound 判定）。
pub async fn get_article_contexts(pool: &PgPool, ids: &[Uuid]) -> AppResult<Vec<ArticleContext>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(c) = get_article_context(pool, *id).await? {
            out.push(c);
        }
    }
    Ok(out)
}

/// Q&A を時系列で追記保存（save=true 時のみ呼ばれる）。
/// 直近の user 質問と assistant 回答の 2 行を入れる想定だが、任意件数を受ける。
pub async fn save_notes(pool: &PgPool, article_id: Uuid, rows: &[AskMessage]) -> AppResult<()> {
    for m in rows {
        sqlx::query(
            "INSERT INTO article_notes (id, article_id, role, content) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(article_id)
        .bind(&m.role)
        .bind(&m.content)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// 保存済み Q&A を時系列で取得（GET /notes 用）。
pub async fn list_notes(pool: &PgPool, article_id: Uuid) -> AppResult<Vec<AskMessage>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT role, content FROM article_notes WHERE article_id = $1 ORDER BY created_at ASC",
    )
    .bind(article_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(role, content)| AskMessage { role, content })
        .collect())
}

// 未使用警告回避用に AppError を使うのは get_article_context 内の `?`（sqlx::Error → AppError）経由。
// 明示 import が dead になる場合は上の use から AppError を外す。
let _ = std::convert::identity::<fn() -> AppError>; // ※ 実コードでは不要。import 調整の注記のみ。
```

> **`articles` を読むことの正当化**: instapaper スライスが `SELECT url, title FROM articles WHERE id=$1` を読み取り専用で持つ前例、`feed_overview` が feeds+articles を JOIN 読みする前例どおり、**読み取りのクロステーブル参照はこのコードベースで許容**。articles の**書き込み所有は移していない**ので「越境共通レイヤー」には当たらない。`query!` コンパイル時マクロは使わない（すべて `query`/`query_as`）。
>
> 上記末尾の `let _ = ...` 行は **注記**であり実コードには書かない（import の dead 警告に注意する旨のメモ）。`-D warnings` を通すには `use` する型だけを残すこと。

### 5.3 `ask/service.rs`（`&AppState` を取り repository + llm を統合）

```rust
use uuid::Uuid;

use super::domain::{
    build_system_multi, build_system_single, validate_conversation, AskMessage,
};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{ChatMessage, ChatRequest, LlmClient};
use crate::shared::state::AppState;

/// articles/service.rs と同型の NotEnabled ゲート（articles を import せず自スライス内に複製）。
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

fn to_chat_messages(messages: &[AskMessage]) -> Vec<ChatMessage> {
    messages
        .iter()
        .map(|m| ChatMessage { role: m.role.clone(), content: m.content.clone() })
        .collect()
}

/// 単一記事への Ask。順序: (1) LLM ゲート → (2) 会話検証 → (3) 記事存在 → (4) Claude 呼び出し →
/// (5) save=true なら直近 user 質問 + assistant 回答を保存。
pub async fn ask_article(
    state: &AppState,
    article_id: Uuid,
    messages: Vec<AskMessage>,
    save: bool,
) -> AppResult<String> {
    let client = llm_client(state)?;                       // 未設定なら 503 を先に返す
    validate_conversation(&messages).map_err(AppError::Validation)?;

    let ctx = repository::get_article_context(&state.db, article_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let system = build_system_single(&ctx);
    let answer = client
        .chat(ChatRequest { system, messages: to_chat_messages(&messages), max_tokens: None })
        .await?;

    if save {
        let last_user = messages.last().cloned();          // validate 済みなので必ず user
        let mut to_save = Vec::new();
        if let Some(u) = last_user {
            to_save.push(u);
        }
        to_save.push(AskMessage { role: "assistant".into(), content: answer.clone() });
        repository::save_notes(&state.db, article_id, &to_save).await?;
    }

    Ok(answer)
}

/// 複数記事の横断 Ask。記事が 1 件も存在しなければ NotFound。
pub async fn ask_articles(
    state: &AppState,
    ids: Vec<Uuid>,
    messages: Vec<AskMessage>,
) -> AppResult<String> {
    let client = llm_client(state)?;
    if ids.is_empty() {
        return Err(AppError::Validation("ids must not be empty".into()));
    }
    validate_conversation(&messages).map_err(AppError::Validation)?;

    let ctxs = repository::get_article_contexts(&state.db, &ids).await?;
    if ctxs.is_empty() {
        return Err(AppError::NotFound);
    }

    let system = build_system_multi(&ctxs);
    client
        .chat(ChatRequest { system, messages: to_chat_messages(&messages), max_tokens: None })
        .await
}

pub async fn get_notes(state: &AppState, article_id: Uuid) -> AppResult<Vec<AskMessage>> {
    repository::list_notes(&state.db, article_id).await
}
```

> HTTP/LLM 呼び出しを `service.rs` 内に閉じ、本スライスに新しい trait/dyn を作らない方針（抽象境界は `shared/llm` のみ）に沿う。横断 Ask は `save` 非対応（記事が複数なので 1 記事への notes に紐づかない。必要なら将来別テーブル）。

### 5.4 `ask/handler.rs`（axum ハンドラ）

```rust
use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::domain::AskMessage;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct AskBody {
    pub messages: Vec<AskMessage>,
    #[serde(default)]
    pub save: bool,
}

#[derive(Debug, Serialize)]
pub struct AskResponse {
    pub answer: String,
}

pub async fn ask_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AskBody>,
) -> AppResult<Json<AskResponse>> {
    let answer = service::ask_article(&state, id, body.messages, body.save).await?;
    Ok(Json(AskResponse { answer }))
}

#[derive(Debug, Deserialize)]
pub struct AskMultiBody {
    pub ids: Vec<Uuid>,
    pub messages: Vec<AskMessage>,
}

pub async fn ask_many(
    State(state): State<AppState>,
    Json(body): Json<AskMultiBody>,
) -> AppResult<Json<AskResponse>> {
    let answer = service::ask_articles(&state, body.ids, body.messages).await?;
    Ok(Json(AskResponse { answer }))
}

#[derive(Debug, Serialize)]
pub struct NotesResponse {
    pub messages: Vec<AskMessage>,
}

pub async fn get_notes(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<NotesResponse>> {
    let messages = service::get_notes(&state, id).await?;
    Ok(Json(NotesResponse { messages }))
}
```

### 5.5 `ask/mod.rs`（routes）

```rust
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // 拡張: 複数記事の横断 Ask。静的セグメント "ask" は {id} より優先される（§11 で確認）。
        .route("/api/articles/ask", post(handler::ask_many))
        // 旗艦: 単一記事への対話 Q&A。
        .route("/api/articles/{id}/ask", post(handler::ask_one))
        // 任意保存した Q&A の読み返し。
        .route("/api/articles/{id}/notes", get(handler::get_notes))
}
```

### 5.6 `features/mod.rs` への追加（2行のみ）

```rust
pub mod ask;   // 既存 pub mod 群（articles/feeds/...）に追加
// router() 内の .merge チェーンに追加:
        .merge(ask::routes())
```

既存スライス（articles/feeds/folders/feed_overview/instapaper/search/stats/health）は一切触らない。

### 5.7 AppError の使い分け（`error.rs` は不編集）

| 状況 | バリアント | HTTP | レスポンス `error` 文字列（Display） |
|---|---|---|---|
| `ANTHROPIC_API_KEY` 未設定 | `NotEnabled` | 503 | `feature not yet enabled: ANTHROPIC_API_KEY is not set` |
| `messages` が空 / role 不正 / 交互でない / 末尾が user でない | `Validation` | 400 | `invalid input: messages must alternate starting with user ...` 等 |
| 横断 Ask で `ids` が空 | `Validation` | 400 | `invalid input: ids must not be empty` |
| `{id}` に該当記事なし / 横断で記事が1件も無い | `NotFound` | 404 | `resource not found` |
| Anthropic が 4xx/5xx・応答形不正・ネットワーク障害 | `Upstream` | 502 | `upstream request failed: anthropic 529: ...` |
| DB エラー | `Database`（`?` で自動 `From`） | 500 | `internal error` |

> **チェック順序**: `ask_article` は (1) LLM ゲート → (2) 会話検証 → (3) 記事存在 → (4) 呼び出し。`ANTHROPIC_API_KEY` 未設定時は記事の有無に関わらず 503（機能ゲートを先に判定。articles の `llm_client()` を先に呼ぶ既存パターンと同型）。**新バリアントは追加しない。**

---

## 6. フロントエンド設計

### 6.1 `lib/api.ts` への追加（型3 + メソッド3）

型を追加（backend JSON をミラー）:

```ts
export interface AskMessage {
  role: "user" | "assistant";
  content: string;
}
export interface AskResponse {
  answer: string;
}
export interface NotesResponse {
  messages: AskMessage[];
}
```

`api` オブジェクトにメソッド追加（既存 `http<T>()` を再利用。命名は既存の `動詞+リソース` に揃える）:

```ts
  // 旗艦: 単一記事への対話 Q&A。messages は user で始まり user で終わる。
  askArticle: (id: string, messages: AskMessage[], save = false) =>
    http<AskResponse>(`/api/articles/${id}/ask`, {
      method: "POST",
      body: JSON.stringify({ messages, save }),
    }),
  // 拡張: 複数記事の横断 Q&A。
  askArticles: (ids: string[], messages: AskMessage[]) =>
    http<AskResponse>("/api/articles/ask", {
      method: "POST",
      body: JSON.stringify({ ids, messages }),
    }),
  // 保存済み Q&A の読み返し。
  getArticleNotes: (id: string) =>
    http<NotesResponse>(`/api/articles/${id}/notes`),
```

### 6.2 新規コンポーネント `components/article/ArticleAsk.tsx`

記事ビュー内のチャット UI。状態は **ローカル**（`createSignal` で会話列と入力。グローバルストアは不要）。

骨子:
- props: `{ articleId: string }`。
- `const [messages, setMessages] = createSignal<AskMessage[]>([]);`（user/assistant の積み上げ）
- `const [draft, setDraft] = createSignal("");`、`const [busy, setBusy] = createSignal(false);`、`const [error, setError] = createSignal<string | null>(null);`
- 送信ハンドラ:
  1. `draft()` が空なら無視。
  2. user メッセージを `messages()` に push（楽観表示）。`draft("")`。
  3. `busy(true)`; `try { const res = await api.askArticle(articleId, next, false); setMessages([...next, { role: "assistant", content: res.answer }]); }`
  4. `catch (e)`: `errorStatus(e)` で 503→「要約/翻訳と同じく ANTHROPIC_API_KEY 未設定です」、502→「Claude への接続に失敗しました」、それ以外は素のメッセージを `error()` へ。**失敗時は末尾の user メッセージを差し戻す**（再送可能に）。
  5. `finally { setBusy(false); }`
- 表示:
  - 会話バブルを `<For each={messages()}>` で。user は右寄せ `bg-primary text-primary-foreground`、assistant は左寄せ `bg-muted`。回答は `prose prose-sm dark:prose-invert max-w-none whitespace-pre-wrap`（要約/翻訳と同じタイポ。`ArticleDetail` の既存クラスに合わせる）。
  - 入力欄: `<Input value={draft()} onInput={...} onKeyDown={Enter で送信} placeholder="この記事について質問…" />` + `<Button onClick={send} disabled={busy()}>{busy() ? "問い合わせ中…" : "質問"}</Button>`。
  - 「保存」トグル（任意）: チェック時は `askArticle(id, next, true)` を呼ぶ（`save=true`）。
  - エラーは `<Show when={error()}>` で `text-destructive text-sm`。
- 全体を `Card`（`components/ui/card.tsx`）1枚で囲み、見出し `text-sm font-medium`「Claude に質問」。

> Ark UI は不要（Input/Button/Card の自前 Tailwind で足りる）。複雑なヘッドレス部品（dialog 等）は使わない。

### 6.3 `ArticleDetail.tsx` への差し込み（1行）

`frontend/src/components/article/ArticleDetail.tsx` の本文ブロック末尾（既存の翻訳 `<Show>` の後ろ）に追加:

```tsx
import ArticleAsk from "@/components/article/ArticleAsk";
// ...本文/要約/翻訳の描画の後ろ、Show の閉じる手前あたりで:
<ArticleAsk articleId={a().id /* ArticleId は { 0: Uuid } 透過。実体は文字列化される */} />
```

> **`a().id` の形に注意**: backend の `ArticleId` は `#[sqlx(transparent)] pub ArticleId(pub Uuid)` で `Serialize` され、JSON では **素の文字列**として出る（`getArticle` の型 `Article.id: string`）。よって `a().id` をそのまま `articleId` に渡してよい。型が合わない場合は `String(a().id)` で文字列化する。

### 6.4 ストア / ルーティング

- グローバルストア（`store.tsx`）の変更は**不要**。会話はコンポーネントローカルで完結する（記事を切り替えたら新しい `ArticleAsk` がマウントされ会話はリセット。ステートレス契約と一致）。
- 新規ルートも不要（既存の記事ビュー内に同居）。横断 Ask の専用 UI は MVP では作らず、`askArticles` メソッドだけ提供しておく（検索結果の複数選択 UI 等は将来機能が配線）。

---

## 7. API 契約

> すべて `/api` プレフィックス。`messages[]` は **user で始まり user で終わる**交互列（assistant を挟んでマルチターン）。

### 7.1 `POST /api/articles/{id}/ask` — 単一記事への対話 Q&A（旗艦）

リクエスト:
```json
{
  "messages": [
    { "role": "user", "content": "この記事の主な結論は？" }
  ],
  "save": false
}
```
追問の例（assistant を挟んで再送。クライアントが全履歴を送る）:
```json
{
  "messages": [
    { "role": "user", "content": "この記事の主な結論は？" },
    { "role": "assistant", "content": "結論は……" },
    { "role": "user", "content": "その根拠を3点で。" }
  ],
  "save": true
}
```
レスポンス（200）:
```json
{ "answer": "根拠は次の3点です。1) …" }
```
エラー:
- 503 `{ "error": "feature not yet enabled: ANTHROPIC_API_KEY is not set" }`
- 400 `{ "error": "invalid input: the last message must be from the user" }`
- 404 `{ "error": "resource not found" }`（記事なし）
- 502 `{ "error": "upstream request failed: anthropic 529: ..." }`

### 7.2 `POST /api/articles/ask` — 複数記事の横断 Q&A（拡張）

リクエスト:
```json
{
  "ids": ["1f1c0e8a-...", "2a2d1f9b-..."],
  "messages": [{ "role": "user", "content": "2つの記事の論点の違いは？" }]
}
```
レスポンス（200）: `{ "answer": "記事1は… 記事2は… 違いは…" }`
エラー:
- 503 / 400（`ids` 空・会話不正）/ 404（記事が1件も存在しない）/ 502

### 7.3 `GET /api/articles/{id}/notes` — 保存済み Q&A の取得

レスポンス（200）:
```json
{
  "messages": [
    { "role": "user", "content": "この記事の主な結論は？" },
    { "role": "assistant", "content": "結論は……" }
  ]
}
```
保存が無ければ `{ "messages": [] }`（200）。

---

## 8. 依存関係

- **本機能が依存する機能**: なし（`ask` スライスは自己完結）。
  - `articles` テーブルは**読み取りのみ**参照（既存・存在前提）。articles スライスのコードは変更しない。
  - `shared/llm` の `chat` 追加に依存（本機能内で同時に実装する。唯一の抽象境界の拡張）。
  - 要約/翻訳機能（articles）と同じ `ANTHROPIC_API_KEY` を共有。片方が有効なら Ask も有効。
- **ブロックする機能（本機能に依存しうる将来機能）**:
  - 横断 Ask の UI 導線（検索結果やリストで複数記事を選んで質問）。本機能は `askArticles` メソッドのみ提供し、選択 UI は将来機能（例: 検索結果の複数選択）が配線する。
- 既存スライスへの接触点は **`features/mod.rs` の2行追加** と **`shared/llm/{mod.rs, anthropic.rs}` への加筆**のみ。articles/feeds 等の機能スライスは不変。
- フロントは `ArticleDetail.tsx` に **import 1行 + JSX 1行** を足すのみ（既存 UI を壊さない）。

---

## 9. テスト計画（TDD）

> **テスト配置の方針（既存プロジェクト前例に合わせる）**: `backend/tests/` は存在せず、本クレートは binary crate（`lib.rs` 無し）。よって (1) 純粋ロジックは各 `.rs` の `#[cfg(test)] mod tests`、(2) DB 往復は `repository.rs` 内の `#[cfg(test)] mod tests` に `#[ignore]` 付き実 DB テスト、(3) HTTP 表面は shell スモークの三段。外部 Anthropic を叩く経路は純粋関数へ切り出してカバーする。

### 9.1 単体テスト（`#[cfg(test)] mod tests` in `domain.rs`、外部 API も DB も不要）

`backend/src/features/ask/domain.rs` 末尾に追加。Red を先に書く。

| テスト | 意図 |
|---|---|
| `validate_rejects_empty` | 空 `messages` を `Err` |
| `validate_rejects_unknown_role` | role が `user`/`assistant` 以外を拒否 |
| `validate_rejects_empty_content` | 空 content を拒否 |
| `validate_rejects_not_starting_with_user` | 先頭 assistant を拒否 |
| `validate_rejects_non_alternating` | user, user の連続を拒否 |
| `validate_rejects_ending_with_assistant` | 末尾 assistant を拒否（応答を得られない列） |
| `validate_accepts_single_user` | `[user]` を受理 |
| `validate_accepts_multiturn` | `[user, assistant, user]` を受理 |
| `truncate_keeps_short` | 上限以下はそのまま返す |
| `truncate_cuts_long_on_char_boundary` | 上限超過で切り、`[... truncated ...]` を付す。**マルチバイト境界で panic しない**（日本語で検証） |
| `build_system_single_embeds_title_and_body` | system に title と本文が入る |
| `build_system_multi_numbers_articles` | 各記事に `ARTICLE 1/2…` の見出しが付く |
| `build_system_multi_handles_empty` | 記事0件でも panic しない（per の 0 除算回避を確認） |

### 9.2 リポジトリ往復テスト（`#[cfg(test)] mod tests` in `repository.rs`、実 DB / `#[ignore]`）

`DATABASE_URL`（`just dev-db` の DB、マイグレーション適用済み）で接続。`#[tokio::test]` + `#[ignore]`。**Anthropic 不要**。前提として feeds/articles に1行ずつ用意してから notes を検証する（FK 制約のため）。

雛形:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL for repo tests");
        PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap()
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres (DATABASE_URL)"]
    async fn notes_save_and_list_in_order() {
        let pool = pool().await;
        // feed + article を用意（id はアプリ生成）
        let feed_id = Uuid::new_v4();
        let article_id = Uuid::new_v4();
        sqlx::query("INSERT INTO feeds (id, url) VALUES ($1, $2)")
            .bind(feed_id).bind(format!("https://example.com/{feed_id}"))
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO articles (id, feed_id, url, title) VALUES ($1, $2, $3, $4)")
            .bind(article_id).bind(feed_id)
            .bind(format!("https://example.com/a/{article_id}")).bind("t")
            .execute(&pool).await.unwrap();

        let rows = vec![
            AskMessage { role: "user".into(), content: "q".into() },
            AskMessage { role: "assistant".into(), content: "a".into() },
        ];
        save_notes(&pool, article_id, &rows).await.unwrap();
        let got = list_notes(&pool, article_id).await.unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].role, "user");
        assert_eq!(got[1].role, "assistant");

        // get_article_context が本文を返す
        let ctx = get_article_context(&pool, article_id).await.unwrap().unwrap();
        assert_eq!(ctx.title, "t");

        // ON DELETE CASCADE で記事削除時に notes も消える
        sqlx::query("DELETE FROM articles WHERE id = $1").bind(article_id)
            .execute(&pool).await.unwrap();
        assert!(list_notes(&pool, article_id).await.unwrap().is_empty());

        sqlx::query("DELETE FROM feeds WHERE id = $1").bind(feed_id)
            .execute(&pool).await.unwrap();
    }
}
```

| テスト | 意図 |
|---|---|
| `notes_save_and_list_in_order` | save→list（順序・role）、`get_article_context` の本文取得、`ON DELETE CASCADE` を network 抜きで自動カバー |

### 9.3 HTTP スモークテスト（稼働スタックへの shell = 既存前例）

`scripts/test/api-ask.sh` を新設（`scripts/test/api-*.sh` と同型。nginx 経由）。**Anthropic 本体は叩かない**範囲で配線を検証。

| 手順 / アサーション | 意図 |
|---|---|
| `POST /api/articles/00000000-0000-0000-0000-000000000000/ask` body `{"messages":[]}` → **400**（会話空。※ ただし API キー設定環境では LLM ゲートを通過し validate で 400。**未設定環境では 503 が先**になる点を注記） | 会話検証 / ゲート順序の確認。CI では `ANTHROPIC_API_KEY` の有無で期待コードを分岐 |
| `POST /api/articles/{存在する記事 id}/ask` body `{"messages":[{"role":"assistant","content":"x"}]}` → 400（先頭が user でない） | スライス合成 + validate 配線（記事存在前で検証が走る順序のため、API キー未設定だと 503 が先。下の注記参照） |
| `GET /api/articles/{任意 id}/notes` → 200 かつ JSON に `messages` 配列 | notes 取得配線（外部呼び出し無し。保存無しでも空配列 200） |

> **CI 注意**: `ask_article` は **LLM ゲートを最初**に判定するため、`ANTHROPIC_API_KEY` 未設定の CI では会話/記事の検証前に 503 が返る。スモークは `GET /notes`（外部非依存）を主軸にし、`POST /ask` は「キー設定時 = validate 400 / 未設定時 = 503」を環境で分岐 assert する。成功パス（実 Claude 応答）は手動（§10 step 11）。

### 9.4 フロント（手動 + 型）

- `tsc`（`just lint`）で `api.ts` / `ArticleAsk.tsx` / `ArticleDetail.tsx` の型整合。
- 手動: 記事ビューで質問→回答表示、追問でマルチターン、`ANTHROPIC_API_KEY` 未設定時に 503 のエラーメッセージ表示、`save` ON で `GET /notes` に残ることを確認。

---

## 10. 実装手順（順序付きチェックリスト）

1. **マイグレーション番号を採番**: `ls backend/migrations/` で最大番号を確認し +1（執筆時点では `0006_article_notes.sql`）。既存ファイルは触らない。
2. **マイグレーション作成**: §4.2 の SQL で新規作成。
3. **`shared/llm` 拡張（Red 可能箇所から）**: `mod.rs` に `ChatMessage`/`ChatRequest` と `LlmClient::chat` を追加、`anthropic.rs` に `complete_messages` + `chat` 実装（§5.0）。`cargo build` が通ることを確認。
4. **ドメイン（Red 先行）**: `backend/src/features/ask/domain.rs` を §5.1 で作り、§9.1 の `#[cfg(test)] mod tests` を先に書いて落ちることを確認 → 実装で Green。`cargo test`（DB 無し）で純粋関数を回す。
5. **repository**: `repository.rs` を §5.2 で作成（`query`/`query_as` のみ）。§9.2 の `#[ignore]` テストも書く。**import の dead 警告に注意**（`-D warnings`）。
6. **service**: `service.rs` を §5.3 で作成。`llm_client` ゲート → validate → 記事存在 → `chat` → 任意 save の順を厳守。
7. **handler + mod + 合成**: `handler.rs`（§5.4）、`mod.rs`（§5.5）を作成。`features/mod.rs` に `pub mod ask;` と `.merge(ask::routes())` を追加（§5.6）。
8. **ビルド & lint**: `just lint`（clippy `-D warnings` / tsc）を通す。`/api/articles/ask`（静的）と `/api/articles/{id}`（articles の動的）が **merge で衝突しないこと**を起動して確認（§11）。
9. **DB 起動 & マイグレーション**: `just dev-db` →（バックエンド起動で自動 migrate、または `just migrate`）。
10. **テスト実行**: `cargo test`（純粋）、`DATABASE_URL=... cargo test -- --ignored`（§9.2）、`scripts/test/api-ask.sh`（§9.3、`chmod +x`）。
11. **手動 E2E（実 Claude）**: `ANTHROPIC_API_KEY` を設定し起動 → 記事ビューで質問→回答、追問、`save` ON → `GET /api/articles/{id}/notes` に残ることを確認。未設定時に 503 を確認。
12. **フロント**: `lib/api.ts`（型3 + メソッド3、§6.1）、`components/article/ArticleAsk.tsx`（§6.2）を作成し、`ArticleDetail.tsx` に import + JSX を1行ずつ差し込む（§6.3）。`just lint` の tsc を通す。
13. **コミット**: マイグレーション・スライス・`shared/llm` 加筆・スクリプト・フロントをまとめて。秘密情報/`.env` はコミットしない。

---

## 11. リスク・未決事項・代替案

- **【要確認】axum 0.8 の静的 vs 動的セグメント共存**: `articles` スライスが `/api/articles/{id}`（GET）を、`ask` スライスが `/api/articles/ask`（POST）を別 Router で定義し `merge` する。matchit 0.8 は同位置の静的セグメント（`ask`・`read-all`）と動的セグメント（`{id}`）の共存を許し、**静的が優先**されるため `POST /api/articles/ask` は `ask_many` に解決される見込み。**起動時にルータ構築が panic しないこと・`/api/articles/ask` が 404/405 にならないことを step 8 で必ず確認**。万一衝突するなら拡張エンドポイントを `/api/ask`（articles 配下を避ける）に変更する（フロントの `askArticles` の URL を1行直すだけ）。
- **トークン上限と文字数切り（厳密でない）**: context は文字数（`MAX_CONTEXT_CHARS=12_000`）で素朴に切る。トークン数ではないため、英語/日本語でトークン効率が異なり超過する可能性がある。家庭内・単一ユーザ前提で許容。超過時は Anthropic が 4xx を返し `Upstream`/502 になる。緩和: 上限を下げる、または将来トークン計測を入れる。
- **会話履歴がリクエストごとに肥大**: ステートレス契約のため、マルチターンが長くなると毎回全 `messages[]` + 記事本文を送信しトークンを食う。MVP は許容。緩和: クライアント側で古いターンを要約・間引く、または `max_tokens`/履歴長の上限をフロントで設ける。
- **`shared/llm` への加筆の妥当性**: 抽象境界は `shared/llm` のみという方針の**内側**での拡張（新 trait を作らず既存 trait に1メソッド追加）。要約/翻訳とコードパスを共有し、`complete` と `complete_messages` の重複は後者へ委譲する小リファクタで解消可能（挙動不変）。articles スライスは触らないため Vertical Slice の独立性は保たれる。
- **`full_content` 未実装**: 現状 `articles` に `full_content` 列は無い（§4.3）。context は `content` を使う。抽出強化機能が列を追加したら repository の SELECT を `COALESCE(NULLIF(full_content,''), content)` に1行差し替えるだけ。**本機能では列を作らない**（責務分離）。
- **応答が非ストリーミング**: 長い回答は完了まで待つ（既存 `complete` と同じ）。UX 上は `busy()` スピナで吸収。将来 SSE 化する場合は `anthropic.rs` に streaming 経路を足し、`handler` を `axum::response::Sse` に変える（別機能スコープ）。
- **横断 Ask の保存非対応**: `POST /api/articles/ask` は `save` を持たない（複数記事に対し単一 `article_id` の notes に紐づけられないため）。必要になれば「conversation」テーブルを別途設計する（本機能スコープ外）。
- **role 検証の厳密さ**: Anthropic Messages API は厳密に user/assistant 交互・末尾 user を要求する。`validate_conversation` でこれを保証してから送るが、Anthropic 側の仕様変更（例: 連続 user の許容）があれば検証を緩める。境界は instapaper の `classify_*` と同じく純粋関数なので調整が容易。
- **`save=true` の二重保存**: マルチターンで毎回 `save=true` を送ると、同じ過去ターンは送らず**直近の user 質問 + assistant 回答の2行のみ**を追記する設計（§5.3）。重複保存はしない。ただしクライアントが同一ターンを2回送れば2回保存されうる点は単一ユーザ前提で許容。
- **マイグレーション番号の順序ハザード**: §4.1。`set_ignore_missing` 未使用のため、先に高い番号を適用済みの永続 DB へ小さい番号を足すと起動が壊れる。着手直前に最小空き整数を採ること。
</content>
</invoke>
