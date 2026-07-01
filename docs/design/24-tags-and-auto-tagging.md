# 24 タグ基盤 + AI 自動タグ付け

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッションの実装者。本書 1 枚だけで着手・完了できるよう、再利用資産・SQL（完全文）・関数シグネチャ・ルート文字列・フロント差分・TDD ケース・番号付き手順まで具体化する。
> **重要な但し書き（AI 部分）**: 本機能は Claude（`shared/llm`）にタグ分類をさせる。**LLM の出力は非決定的**なので、(a) プロンプトに既存タグ語彙を渡して「再利用優先」を強制し、(b) 出力を厳密に JSON パースして失敗時は安全側に倒し、(c) 結果を DB にキャッシュしてトークンを使い回す。プロンプト本文やモデルの返却形は実装時に微調整が要る前提で、**JSON パーサ（`parse_tag_suggestions`）を純粋関数として単体テスト対象に切り出す**（§5.1）。

---

## 1. 概要

記事に**タグ**を付けて整理できるようにする。タグは二系統で付く: (a) ユーザーが手で付ける、(b) **Claude が記事を読み、ユーザー個人の一貫した語彙に分類して提案する**（`POST /api/articles/{id}/suggest-tags`）。AI 提案はあくまで提案であり、ユーザーが承認・編集して初めて記事に紐づく。提案結果は DB にキャッシュし、同一記事への再提案要求はトークンを消費しない（要約/翻訳のキャッシュ方針と同型。`articles/service.rs` 参照）。

本機能はバックエンドに新スライス **`tags`** を 1 枚追加する。責務は (1) タグの CRUD（`tags` テーブル）、(2) 記事⇄タグの関連付け（`article_tags` 結合テーブル）、(3) AI タグ提案とそのキャッシュ（`article_tag_suggestions` テーブル）。AI 呼び出しは唯一の抽象境界 `shared/llm` を再利用し、`LlmClient` trait に `suggest_tags` を 1 つ足す（要約/翻訳と同じ場所・同じ流儀）。`ANTHROPIC_API_KEY` 未設定時は `AppError::NotEnabled`（503）を返す「任意機能」パターンに従う。

このタグ基盤は、後続の **ダイジェスト**（タグ別まとめ）・**スマートビュー**（タグ条件で記事を絞る保存済みビュー）・**自動ルール**（フィード/キーワード→タグ付与）の共通土台になる。本書ではそれら派生機能は**非スコープ**とし、土台として必要な「タグの語彙・関連・AI 提案・承認」までを実装する。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）
- マイグレーション **`0006_tags.sql`**（番号は着手前に要確認。§4.1）。3 テーブル: `tags`（語彙）、`article_tags`（記事⇄タグ結合）、`article_tag_suggestions`（AI 提案キャッシュ）。
- 新スライス `backend/src/features/tags/`（`domain` / `repository` / `service` / `handler` / `mod`）。
- タグ CRUD: `GET /api/tags`（一覧 + 各タグの記事数）、`POST /api/tags`（作成）、`PATCH /api/tags/{id}`（改名・色変更）、`DELETE /api/tags/{id}`（削除）。
- 記事⇄タグ関連付け: `GET /api/articles/{id}/tags`（その記事のタグ一覧）、`PUT /api/articles/{id}/tags`（その記事のタグ集合を一括設定）、`DELETE /api/articles/{id}/tags/{tag_id}`（1 件外す）。
- AI 提案: `POST /api/articles/{id}/suggest-tags`。既存タグ語彙を渡し Claude が分類。結果を `article_tag_suggestions` にキャッシュ。`?refresh=true` で再生成。資格未設定（`ANTHROPIC_API_KEY` 無し）は 503。
- `shared/llm` 拡張: `LlmClient` trait に `suggest_tags(&self, SuggestTagsRequest) -> AppResult<String>`、`AnthropicClient` に実装を追加（要約/翻訳と同列。新スライスではなく既存の抽象境界への追記）。
- 提案 JSON のパース・正規化を純粋関数に切り出し単体テスト（`parse_tag_suggestions`、`TagName::parse`、`normalize_name`）。
- フロント: `lib/api.ts` に型 4・メソッド 8。`components/ui/tag-badge.tsx`（自前 Tailwind）と `components/TagEditor.tsx`（記事のタグ編集 + AI 提案ボタン）。`ArticleView` にタグ編集セクションを追加（ArticleView は既存。最小差分で組み込む）。`store.tsx` に `tags` リソース 1 本（任意・グローバル参照用）。
- バックエンド自動テスト: ドメイン純粋関数（§9.1）+ リポジトリ往復（§9.2、実 DB `#[ignore]`）+ HTTP スモーク（§9.3、shell）。

### 非スコープ（本機能では実装しない）
- **タグで記事一覧を絞る**（`GET /api/articles?tag=...`）。articles スライスの `list` を変更することになるため本書では行わない。**スマートビュー機能**として別スライスで後続実装（本タグ基盤が前提）。
- **ダイジェスト**（タグ別の定期まとめ生成）。本タグ基盤 + スケジューラ（`shared/scheduler.rs`）の上に別機能で載せる。
- **自動ルール**（フィード/キーワード→自動タグ付与）。`article_tags.source='rule'` への拡張余地は残すが本書では `user`/`ai` のみ。
- **タグ階層・タググループ**。フラットな語彙のみ（folders と同じく入れ子なし）。
- **複数ユーザー / 共有語彙**。単一ユーザー前提（タグはグローバルに 1 セット）。
- AI 提案の**自動承認**（提案を勝手に紐づけない）。承認は常にユーザー操作。

---

## 3. 既存実装の調査と再利用

実ファイルを確認済み。以下を **再利用し、車輪の再発明をしない**。

| 再利用資産 | 実体（確認済みファイル） | 本機能での使い方 |
|---|---|---|
| LLM 抽象境界 + DB キャッシュ | `shared/llm/mod.rs`（`LlmClient` trait: `summarize`/`translate`）、`shared/llm/anthropic.rs`（`AnthropicClient::complete(system,user)` で Messages API 直叩き、最初の text ブロック抽出）、`articles/service.rs`（キャッシュヒット判定 → 無ければ呼んで保存） | `LlmClient` に `suggest_tags` を追加（**唯一許された抽象境界への追記**）。`complete()` を再利用。提案キャッシュは `article_tag_suggestions.suggested_at` の有無で判定（summary キャッシュと同型） |
| 任意機能 = `NotEnabled` | `articles/service.rs::llm_client()`（`anthropic_api_key` 無しで `NotEnabled("ANTHROPIC_API_KEY is not set")`） | 同じ `llm_client` 構築ロジックを `tags/service.rs` に**コピーして**持つ（スライス自己完結。articles を import して結合しない） |
| 値オブジェクト `parse() -> Result<_,String>` | `feeds/domain.rs::FeedUrl::parse`（trim + 検査、`#[cfg(test)]`） | `TagName::parse`（trim・空/長さ検査・正規化）を同型でスライス内に新設。`Err(String)` は `map_err(AppError::Validation)` |
| 主キー newtype | `feeds/domain.rs::FeedId`、`articles/domain.rs::ArticleId`（`#[derive(...,sqlx::Type)] #[sqlx(transparent)]`、`pub struct X(pub Uuid)`） | `TagId(pub Uuid)` を同型で新設。article 参照は `articles/domain.rs::ArticleId` を import（クロススライス domain 参照は既存前例: `articles` が `FeedId` を import） |
| クロステーブル read を自スライス SQL で完結 | `instapaper/repository.rs::get_article_ref`（`SELECT url,title FROM articles WHERE id=$1`、書き込み所有は移さない） | `tags` から `articles` を**読み取り専用 SQL** で引く（提案用に title/content を取得、存在確認）。articles の書き込みは触らない |
| スライス構成 + `routes()` | `articles/mod.rs`・`feeds/`・`folders/`・`instapaper/`（5 ファイル、`fn routes() -> Router<AppState>`、`.route("/path", get(...).post(...))`、パスパラメータは `{id}`） | 同じ 5 ファイル構成で `tags` を作る。パスは `/api/tags`・`/api/articles/{id}/tags` |
| `features/mod.rs` の合成 | `pub mod ...;` + `.merge(...::routes())` | `pub mod tags;` と `.merge(tags::routes())` を 1 行ずつ追加。既存スライスは触らない |
| sqlx ランタイムクエリ + upsert | `instapaper/repository.rs`（`ON CONFLICT (id) DO UPDATE`）、`articles/repository.rs`（`fetch_optional().ok_or(AppError::NotFound)`、`rows_affected()`） | タグ作成は `ON CONFLICT (lower(name)) DO ...`、関連付けは `ON CONFLICT (article_id,tag_id) DO ...`、削除は `rows_affected()`。すべて `query`/`query_as`（`query!` 禁止） |
| `AppError` 6 バリアント | `shared/error.rs`（`NotFound`/404, `Validation`/400, `NotEnabled`/503, `Upstream`/502, `Database`/500, `Other`/500、`IntoResponse` で `Json({"error":<Display>})`） | 新バリアントを足さず既存で表現（§5.7）。`error.rs` は編集しない |
| `AppState{db,config,http}` | `shared/state.rs`（`#[derive(Clone)]`） | `state.http`/`state.config`/`state.db` をそのまま使う |
| フロント API クライアント | `frontend/src/lib/api.ts`（`http<T>()` は 204→`undefined` 畳み込み、`動詞+リソース` 命名） | `http<T>()` を再利用し型 4・メソッド 8 を追加 |
| 自前 UI 部品 + グローバル状態 | `components/ui/button.tsx`/`badge.tsx`/`dialog.tsx`/`input.tsx`、`lib/store.tsx`（`createContext` + `createResource` で `feeds`/`folders`） | `tag-badge.tsx` を `badge.tsx` 同型で新設。`store.tsx` に `tags` リソースを追加（`feeds`/`folders` と同じ書き方） |
| HTTP スモークの慣習 | `scripts/test/api-stats.sh`（稼働スタックに curl、HTTP コード + JSON キーを assert） | `scripts/test/api-tags.sh` を同型で新設（§9.3） |
| 自動マイグレーション実行 | `main.rs` → `db::run_migrations` → `sqlx::migrate!("./migrations")` | ファイルを置くだけで起動時適用。**番号順序に注意**（§4.1） |

> **依存追加は不要**: `uuid`/`serde`/`serde_json`/`sqlx`（`json`/`Json` 型）/`chrono`/`async_trait`/`reqwest` はすべて既存依存。`article_tag_suggestions.suggestions` は `JSONB` 列で、sqlx 既定の `serde_json::Value` 束縛で読み書きできる（Cargo.toml 変更不要）。

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方（必読）

`main.rs` の `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を呼ばないため、**適用済み最大バージョンより小さい未適用マイグレーションを後から追加すると起動時に `VersionMissing`（out-of-order）でエラー**になる。

**ルール**: 着手前に `ls backend/migrations/` で最新番号を確認し、**最大番号 +1** を採る。本書執筆時点の最新は `0005_search.sql` なので、**暫定的に `0006_tags.sql`** と採番する。並行作業（apalis 移行等）が先に `0006` を取った場合は `0007` 以降へ繰り上げること。既存マイグレーションは**編集しない**（追記のみ）。

### 4.2 スキーマ

新規ファイル **`backend/migrations/0006_tags.sql`**（番号は §4.1 で確認）:

```sql
-- 0006_tags.sql
-- Tags: a flat, user-owned vocabulary for classifying articles. Single-user app,
-- so tags are global (no owner column). Two attach sources: 'user' (hand-applied)
-- and 'ai' (approved from a Claude suggestion). Foundation for digests / smart
-- views / rules (future, out of scope here).

CREATE TABLE IF NOT EXISTS tags (
    id         UUID PRIMARY KEY,
    name       TEXT NOT NULL,                 -- display form (as the user typed it)
    color      TEXT,                          -- optional UI hint: oklch token name or hex; NULL = default
    source     TEXT NOT NULL DEFAULT 'user'   -- provenance of the tag itself
                 CHECK (source IN ('user', 'ai')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Case-insensitive uniqueness: "Rust" and "rust" are the same tag. This is the
-- real guard that keeps the vocabulary consistent (the AI prompt also nudges
-- reuse, but the DB is the source of truth).
CREATE UNIQUE INDEX IF NOT EXISTS idx_tags_name_lower ON tags (lower(name));

-- Article <-> tag association. Composite PK makes (re)attaching idempotent.
CREATE TABLE IF NOT EXISTS article_tags (
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    tag_id     UUID NOT NULL REFERENCES tags(id)     ON DELETE CASCADE,
    source     TEXT NOT NULL DEFAULT 'user'          -- who attached THIS edge
                 CHECK (source IN ('user', 'ai')),
    confidence REAL,                                  -- AI confidence 0..1; NULL for user edges
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (article_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_article_tags_tag_id ON article_tags(tag_id);

-- AI suggestion cache (the LLM cache, same spirit as articles.summary).
-- One row per article. Presence of a row = cache hit; re-suggest only when the
-- caller passes ?refresh=true. Stores the raw, NOT-yet-approved suggestions.
CREATE TABLE IF NOT EXISTS article_tag_suggestions (
    article_id   UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    suggestions  JSONB NOT NULL,        -- [{"name":"rust","confidence":0.9}, ...]
    model        TEXT NOT NULL,         -- model id used, for auditing
    suggested_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

設計判断:
- **`tags` を別テーブルにする理由**: 語彙を一元管理し、`article_tags` が外部キーで参照することで「同じタグ名の表記ゆれ」を構造的に防ぐ。`lower(name)` のユニークインデックスが一貫性の中核。
- **`article_tags.source` / `confidence`**: 提案由来か手動かを区別し、将来 UI で「AI が付けたタグ」を見分けたり、信頼度で並べたりできる。`source` の `CHECK` に `'rule'` を足せば自動ルール機能（非スコープ）へ拡張可能。
- **`article_tag_suggestions` を独立テーブルにする理由**: 提案は「まだ承認されていない候補」であり、確定した `article_tags` とはライフサイクルが違う。`articles` への列追加（要約/翻訳と同型）も選べたが、**articles スライスの所有物に tags の関心を漏らさない**ため独立テーブルにする（instapaper が `articles` に列を足さず別テーブルにしたのと同じ判断）。`suggested_at` の有無がキャッシュヒット判定。
- **`ON DELETE CASCADE`**: 記事/タグが消えたら関連・提案も消える（孤立行を残さない）。
- **`color` を任意列に**: ミニマルデザイン（機能 07）の oklch トークンで装飾するためのヒント。NULL なら UI 既定色。

`feeds`/`articles` への列追加は**無い**。

---

## 5. バックエンド設計

新スライス **`backend/src/features/tags/`**。5 ファイル構成。加えて `shared/llm` に `suggest_tags` を 1 メソッド追記（§5.8）。

### 5.1 `domain.rs`（newtype + 値オブジェクト + 純粋ロジック）

```rust
use serde::Serialize;
use uuid::Uuid;

use crate::features::articles::domain::ArticleId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct TagId(pub Uuid);

/// 永続化されたタグ。API レスポンスにそのまま使う。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Tag {
    pub id: TagId,
    pub name: String,
    pub color: Option<String>,
    pub source: String, // "user" | "ai"
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 一覧用: タグ + そのタグが付いた記事数（GET /api/tags）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TagWithCount {
    pub id: TagId,
    pub name: String,
    pub color: Option<String>,
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub article_count: i64,
}

/// 記事に付いたタグ（GET /api/articles/{id}/tags の 1 要素）。
/// source/confidence は article_tags 由来（AI が付けたか手動か）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ArticleTag {
    pub id: TagId,
    pub name: String,
    pub color: Option<String>,
    pub attached_source: String,   // article_tags.source
    pub confidence: Option<f32>,   // article_tags.confidence
}

/// 検証済みタグ名。空・長すぎを構築時に弾く（不正状態を表現不能にする）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagName(String);

const MAX_TAG_LEN: usize = 50;

/// 表記ゆれを吸収する正規化（内部空白を 1 個に畳み、前後を trim）。
/// 大文字小文字は DB の lower(name) ユニーク制約で吸収するためここでは変えない。
pub fn normalize_name(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl TagName {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let n = normalize_name(&raw.into());
        if n.is_empty() {
            return Err("tag name must not be empty".into());
        }
        if n.chars().count() > MAX_TAG_LEN {
            return Err(format!("tag name must be at most {MAX_TAG_LEN} characters"));
        }
        Ok(Self(n))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Claude が返す 1 件の生提案（承認前）。article_tag_suggestions.suggestions に
/// この配列を JSON で保存する。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RawSuggestion {
    pub name: String,
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// Claude の生出力（JSON 文字列）を厳密にパース・正規化する純粋関数。
/// LLM はときに前後に説明文や ```json フェンスを付けるので、最初の '[' から
/// 最後の ']' までを切り出してから serde_json でパースする。失敗は Err（安全側）。
/// 空名は捨て、正規化後に重複（大文字小文字無視）を除去し、max_tags 件に切り詰める。
pub fn parse_tag_suggestions(raw: &str, max_tags: usize) -> Result<Vec<RawSuggestion>, String> {
    let start = raw.find('[').ok_or("no JSON array found in LLM output")?;
    let end = raw.rfind(']').ok_or("no JSON array found in LLM output")?;
    if end < start {
        return Err("malformed JSON array in LLM output".into());
    }
    let slice = &raw[start..=end];
    let parsed: Vec<RawSuggestion> =
        serde_json::from_str(slice).map_err(|e| format!("invalid suggestion JSON: {e}"))?;

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for s in parsed {
        let name = normalize_name(&s.name);
        if name.is_empty() {
            continue;
        }
        let key = name.to_lowercase();
        if !seen.insert(key) {
            continue; // 重複除去（大文字小文字無視）
        }
        let confidence = s.confidence.map(|c| c.clamp(0.0, 1.0));
        out.push(RawSuggestion { name, confidence });
        if out.len() >= max_tags {
            break;
        }
    }
    Ok(out)
}
```

> 提案の JSON パースを純粋関数に切り出すのは、Claude を叩かずに TDD で Red→Green を回すため（MEMORY「書いたら必ず実行」「バグ修正もテスト先行」）。フェンス除去・重複除去・件数制限・clamp の境界はここで完全にテストする（§9.1）。

### 5.2 `repository.rs`（`&PgPool` を取る free async fn、ランタイムクエリのみ）

```rust
use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{ArticleTag, RawSuggestion, Tag, TagId, TagWithCount};
use crate::features::articles::domain::ArticleId;
use crate::shared::error::{AppError, AppResult};

/// 提案用に記事本文を引く読み取り射影（articles 書き込み所有は移さない）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArticleText {
    pub title: String,
    pub content: String,
}

// ---- tags CRUD ----

pub async fn list_tags(pool: &PgPool) -> AppResult<Vec<TagWithCount>> {
    let rows = sqlx::query_as::<_, TagWithCount>(
        r#"SELECT t.id, t.name, t.color, t.source, t.created_at,
                  COUNT(at.article_id) AS article_count
           FROM tags t
           LEFT JOIN article_tags at ON at.tag_id = t.id
           GROUP BY t.id
           ORDER BY t.name ASC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 既存なら（大文字小文字無視で）それを返す upsert。AI 提案承認とユーザー作成の両方で使う。
pub async fn upsert_tag(
    pool: &PgPool,
    name: &str,
    color: Option<&str>,
    source: &str,
) -> AppResult<Tag> {
    let tag = sqlx::query_as::<_, Tag>(
        r#"INSERT INTO tags (id, name, color, source)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (lower(name)) DO UPDATE
             SET color = COALESCE(EXCLUDED.color, tags.color)
           RETURNING id, name, color, source, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .bind(color)
    .bind(source)
    .fetch_one(pool)
    .await?;
    Ok(tag)
}

pub async fn update_tag(
    pool: &PgPool,
    id: TagId,
    name: &str,
    color: Option<&str>,
) -> AppResult<Tag> {
    let tag = sqlx::query_as::<_, Tag>(
        r#"UPDATE tags SET name = $2, color = $3
           WHERE id = $1
           RETURNING id, name, color, source, created_at"#,
    )
    .bind(id.0)
    .bind(name)
    .bind(color)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(tag)
}

pub async fn delete_tag(pool: &PgPool, id: TagId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM tags WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

// ---- article <-> tag ----

pub async fn list_article_tags(pool: &PgPool, article_id: ArticleId) -> AppResult<Vec<ArticleTag>> {
    let rows = sqlx::query_as::<_, ArticleTag>(
        r#"SELECT t.id, t.name, t.color,
                  at.source AS attached_source, at.confidence
           FROM article_tags at
           JOIN tags t ON t.id = at.tag_id
           WHERE at.article_id = $1
           ORDER BY t.name ASC"#,
    )
    .bind(article_id.0)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn attach_tag(
    pool: &PgPool,
    article_id: ArticleId,
    tag_id: TagId,
    source: &str,
    confidence: Option<f32>,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO article_tags (article_id, tag_id, source, confidence)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (article_id, tag_id) DO UPDATE
             SET source = EXCLUDED.source, confidence = EXCLUDED.confidence"#,
    )
    .bind(article_id.0)
    .bind(tag_id.0)
    .bind(source)
    .bind(confidence)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn detach_tag(pool: &PgPool, article_id: ArticleId, tag_id: TagId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM article_tags WHERE article_id = $1 AND tag_id = $2")
        .bind(article_id.0)
        .bind(tag_id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// PUT /api/articles/{id}/tags の「集合一括設定」を 1 トランザクションで。
/// 渡された tag_id 群以外の user エッジを消し、渡されたものを upsert する。
/// （AI エッジは保持: ai エッジを消すかは UI 仕様次第。ここでは全置換せず user 由来のみ置換）
pub async fn set_article_tags(
    pool: &PgPool,
    article_id: ArticleId,
    tag_ids: &[TagId],
) -> AppResult<()> {
    let ids: Vec<Uuid> = tag_ids.iter().map(|t| t.0).collect();
    let mut tx = pool.begin().await?;

    // 今回の集合に無い user エッジを削除（ai エッジは温存）。
    sqlx::query(
        "DELETE FROM article_tags
         WHERE article_id = $1 AND source = 'user' AND NOT (tag_id = ANY($2))",
    )
    .bind(article_id.0)
    .bind(&ids)
    .execute(&mut *tx)
    .await?;

    for id in &ids {
        sqlx::query(
            r#"INSERT INTO article_tags (article_id, tag_id, source, confidence)
               VALUES ($1, $2, 'user', NULL)
               ON CONFLICT (article_id, tag_id) DO UPDATE SET source = 'user'"#,
        )
        .bind(article_id.0)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

// ---- articles read-only (for suggestion) ----

pub async fn get_article_text(pool: &PgPool, article_id: ArticleId) -> AppResult<Option<ArticleText>> {
    let row = sqlx::query_as::<_, ArticleText>("SELECT title, content FROM articles WHERE id = $1")
        .bind(article_id.0)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// 現在の語彙（既存タグ名）。プロンプトに渡して再利用を促す。
pub async fn vocabulary(pool: &PgPool) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM tags ORDER BY name ASC")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

// ---- AI suggestion cache ----

pub async fn get_cached_suggestions(
    pool: &PgPool,
    article_id: ArticleId,
) -> AppResult<Option<Vec<RawSuggestion>>> {
    let row: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT suggestions FROM article_tag_suggestions WHERE article_id = $1")
            .bind(article_id.0)
            .fetch_optional(pool)
            .await?;
    match row {
        Some((json,)) => {
            let v: Vec<RawSuggestion> =
                serde_json::from_value(json).map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;
            Ok(Some(v))
        }
        None => Ok(None),
    }
}

pub async fn save_suggestions(
    pool: &PgPool,
    article_id: ArticleId,
    suggestions: &[RawSuggestion],
    model: &str,
) -> AppResult<()> {
    let json = serde_json::to_value(suggestions).map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;
    sqlx::query(
        r#"INSERT INTO article_tag_suggestions (article_id, suggestions, model, suggested_at)
           VALUES ($1, $2, $3, now())
           ON CONFLICT (article_id) DO UPDATE
             SET suggestions = EXCLUDED.suggestions,
                 model = EXCLUDED.model,
                 suggested_at = now()"#,
    )
    .bind(article_id.0)
    .bind(json)
    .bind(model)
    .execute(pool)
    .await?;
    Ok(())
}
```

> **`articles` を読むことの正当化**: 提案には記事本文が要る（`get_article_text`）。`instapaper/repository.rs::get_article_ref` と同じ「読み取り専用クロステーブル参照」の前例どおり許容。articles の**書き込み所有は移さない**。`query!` は使わず全て `query`/`query_as`。

### 5.3 `service.rs`（`&AppState` を取り repository + LLM を統合）

```rust
use super::domain::{
    parse_tag_suggestions, ArticleTag, RawSuggestion, Tag, TagId, TagName, TagWithCount,
};
use super::repository;
use crate::features::articles::domain::ArticleId;
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{LlmClient, SuggestTagsRequest};
use crate::shared::state::AppState;

const MAX_SUGGESTIONS: usize = 6;

/// articles/service.rs と同型。スライス自己完結のため articles を import せず複製。
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

pub async fn list_tags(state: &AppState) -> AppResult<Vec<TagWithCount>> {
    repository::list_tags(&state.db).await
}

pub async fn create_tag(state: &AppState, name: TagName, color: Option<String>) -> AppResult<Tag> {
    repository::upsert_tag(&state.db, name.as_str(), color.as_deref(), "user").await
}

pub async fn update_tag(
    state: &AppState,
    id: TagId,
    name: TagName,
    color: Option<String>,
) -> AppResult<Tag> {
    repository::update_tag(&state.db, id, name.as_str(), color.as_deref()).await
}

pub async fn delete_tag(state: &AppState, id: TagId) -> AppResult<()> {
    let n = repository::delete_tag(&state.db, id).await?;
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn list_article_tags(state: &AppState, article_id: ArticleId) -> AppResult<Vec<ArticleTag>> {
    repository::list_article_tags(&state.db, article_id).await
}

pub async fn set_article_tags(
    state: &AppState,
    article_id: ArticleId,
    tag_ids: &[TagId],
) -> AppResult<Vec<ArticleTag>> {
    repository::set_article_tags(&state.db, article_id, tag_ids).await?;
    repository::list_article_tags(&state.db, article_id).await
}

pub async fn detach_tag(state: &AppState, article_id: ArticleId, tag_id: TagId) -> AppResult<()> {
    let n = repository::detach_tag(&state.db, article_id, tag_id).await?;
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// AI タグ提案（キャッシュ付き）。
/// 流れ: (1) refresh=false かつキャッシュ有り → そのまま返す（トークン不消費）。
///       (2) 記事存在チェック（無ければ NotFound）。
///       (3) 資格チェック（ANTHROPIC_API_KEY 無しは NotEnabled）。
///       (4) 語彙を渡して Claude 呼び出し → JSON パース → 正規化 → 保存。
/// 戻り値は提案配列（承認は別エンドポイント PUT/POST が行う）。
pub async fn suggest_tags(
    state: &AppState,
    article_id: ArticleId,
    refresh: bool,
) -> AppResult<Vec<RawSuggestion>> {
    if !refresh {
        if let Some(cached) = repository::get_cached_suggestions(&state.db, article_id).await? {
            return Ok(cached);
        }
    }

    let article = repository::get_article_text(&state.db, article_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let client = llm_client(state)?; // ここで NotEnabled 判定
    let vocabulary = repository::vocabulary(&state.db).await?;

    let raw = client
        .suggest_tags(SuggestTagsRequest {
            title: article.title,
            content: article.content,
            vocabulary,
            max_tags: MAX_SUGGESTIONS,
        })
        .await?;

    let suggestions = parse_tag_suggestions(&raw, MAX_SUGGESTIONS)
        .map_err(|e| AppError::Upstream(format!("could not parse LLM tag output: {e}")))?;

    repository::save_suggestions(&state.db, article_id, &suggestions, &state.config.anthropic_model)
        .await?;
    Ok(suggestions)
}
```

> 順序が重要: **キャッシュ → 記事存在 → 資格 → 呼び出し**。キャッシュヒット時は記事存在も資格も問わず即返す（要約のキャッシュ挙動と同型）。LLM 出力が JSON として壊れていた場合は `Upstream`（502）に倒す（クライアントのせいではない上流事象）。

### 5.4 `handler.rs`（axum ハンドラ）

```rust
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use super::domain::{ArticleTag, RawSuggestion, Tag, TagId, TagName, TagWithCount};
use super::service;
use crate::features::articles::domain::ArticleId;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn list_tags(State(state): State<AppState>) -> AppResult<Json<Vec<TagWithCount>>> {
    Ok(Json(service::list_tags(&state).await?))
}

#[derive(Debug, Deserialize)]
pub struct CreateTagBody {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn create_tag(
    State(state): State<AppState>,
    Json(body): Json<CreateTagBody>,
) -> AppResult<(StatusCode, Json<Tag>)> {
    let name = TagName::parse(body.name).map_err(AppError::Validation)?;
    let tag = service::create_tag(&state, name, body.color).await?;
    Ok((StatusCode::CREATED, Json(tag)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateTagBody {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn update_tag(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Json(body): Json<UpdateTagBody>,
) -> AppResult<Json<Tag>> {
    let name = TagName::parse(body.name).map_err(AppError::Validation)?;
    Ok(Json(service::update_tag(&state, TagId(id), name, body.color).await?))
}

pub async fn delete_tag(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<StatusCode> {
    service::delete_tag(&state, TagId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_article_tags(
    State(state): State<AppState>,
    Path(article_id): Path<uuid::Uuid>,
) -> AppResult<Json<Vec<ArticleTag>>> {
    Ok(Json(service::list_article_tags(&state, ArticleId(article_id)).await?))
}

#[derive(Debug, Deserialize)]
pub struct SetTagsBody {
    pub tag_ids: Vec<uuid::Uuid>,
}

pub async fn set_article_tags(
    State(state): State<AppState>,
    Path(article_id): Path<uuid::Uuid>,
    Json(body): Json<SetTagsBody>,
) -> AppResult<Json<Vec<ArticleTag>>> {
    let ids: Vec<TagId> = body.tag_ids.into_iter().map(TagId).collect();
    Ok(Json(service::set_article_tags(&state, ArticleId(article_id), &ids).await?))
}

pub async fn detach_tag(
    State(state): State<AppState>,
    Path((article_id, tag_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> AppResult<StatusCode> {
    service::detach_tag(&state, ArticleId(article_id), TagId(tag_id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SuggestQuery {
    #[serde(default)]
    pub refresh: bool,
}

pub async fn suggest_tags(
    State(state): State<AppState>,
    Path(article_id): Path<uuid::Uuid>,
    Query(q): Query<SuggestQuery>,
) -> AppResult<Json<Vec<RawSuggestion>>> {
    Ok(Json(service::suggest_tags(&state, ArticleId(article_id), q.refresh).await?))
}
```

### 5.5 `mod.rs`（routes）

```rust
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/tags", get(handler::list_tags).post(handler::create_tag))
        .route(
            "/api/tags/{id}",
            axum::routing::patch(handler::update_tag).delete(handler::delete_tag),
        )
        .route(
            "/api/articles/{id}/tags",
            get(handler::list_article_tags).put(handler::set_article_tags),
        )
        .route(
            "/api/articles/{id}/tags/{tag_id}",
            delete(handler::detach_tag),
        )
        .route(
            "/api/articles/{id}/suggest-tags",
            post(handler::suggest_tags),
        )
}
```

> ルート文字列のパスパラメータは `{id}`（Axum 0.8 形式）。`articles` スライスの既存ルートと同じ書式（`/api/articles/{id}/read` 等）に合わせる。`/api/articles/{id}/tags` を tags スライスが持つことに注意（articles スライスは触らない。Router マージで共存する）。

### 5.6 `features/mod.rs` への追加（2 行のみ）

```rust
pub mod tags; // 既存 pub mod 群に追加（articles; feeds; folders; ... の並びに）
// router() 内の .merge チェーンに追加:
        .merge(tags::routes())
```

既存スライス（articles/feeds/folders/instapaper/search/health 等）は一切触らない。

### 5.7 AppError の使い分け（`error.rs` は不編集）

| 状況 | バリアント | HTTP | レスポンス `error`（Display） |
|---|---|---|---|
| タグ名が空 / 長すぎ | `Validation` | 400 | `invalid input: tag name must not be empty` |
| `PATCH`/`DELETE` 対象タグが無い | `NotFound` | 404 | `resource not found` |
| `suggest-tags` の article_id に記事が無い | `NotFound` | 404 | `resource not found` |
| `suggest-tags` で `ANTHROPIC_API_KEY` 未設定 | `NotEnabled` | 503 | `feature not yet enabled: ANTHROPIC_API_KEY is not set` |
| `detach` 対象エッジが無い | `NotFound` | 404 | `resource not found` |
| Claude 呼び出し失敗 / JSON パース不能 | `Upstream` | 502 | `upstream request failed: ...` |
| DB エラー | `Database`（`?` で自動 `From`） | 500 | `internal error` |

> 新バリアントは追加しない。`suggest_tags` のチェック順は **キャッシュ→記事存在→資格→呼び出し**（§5.3）。キャッシュヒット時のみ資格未設定でも 200 を返せるが、これは「以前 AI が出した結果の再表示」であり意図どおり。

### 5.8 `shared/llm` の拡張（唯一許された抽象境界への追記）

`shared/llm/mod.rs` に型 + trait メソッドを追加:

```rust
#[derive(Debug, Clone)]
pub struct SuggestTagsRequest {
    pub title: String,
    pub content: String,
    /// 既存タグ語彙。Claude に再利用を促し、語彙の一貫性を保つ。
    pub vocabulary: Vec<String>,
    pub max_tags: usize,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn summarize(&self, req: SummarizeRequest) -> AppResult<String>;
    async fn translate(&self, req: TranslateRequest) -> AppResult<String>;
    // 追加: タグ提案。返り値は JSON 配列文字列（service 側で parse_tag_suggestions に通す）。
    async fn suggest_tags(&self, req: SuggestTagsRequest) -> AppResult<String>;
}
```

`shared/llm/anthropic.rs` に実装を追加（`complete()` を再利用）:

```rust
use super::{LlmClient, SuggestTagsRequest, SummarizeRequest, TranslateRequest};

#[async_trait]
impl LlmClient for AnthropicClient {
    // 既存 summarize / translate はそのまま
    async fn suggest_tags(&self, req: SuggestTagsRequest) -> AppResult<String> {
        let vocab = if req.vocabulary.is_empty() {
            "(none yet)".to_string()
        } else {
            req.vocabulary.join(", ")
        };
        let system = format!(
            "You are a tagging assistant for a personal RSS reader. \
             Classify the article using a CONSISTENT personal vocabulary. \
             PREFER reusing tags from this existing vocabulary: [{vocab}]. \
             Only invent a new tag when none of the existing ones fit. \
             Return AT MOST {max} tags. \
             Respond with ONLY a JSON array, no prose, no code fences, like: \
             [{{\"name\":\"rust\",\"confidence\":0.9}}]. \
             Tag names should be short, lowercase nouns.",
            vocab = vocab,
            max = req.max_tags,
        );
        // content は長すぎると無駄なので適度に切る（要約と同様、ここでは素直に渡す）。
        let user = format!("Title: {}\n\n{}", req.title, req.content);
        self.complete(&system, &user).await
    }
}
```

> trait に 1 メソッド足すと既存実装（`AnthropicClient`）に追記が要るだけ。**新規 dyn / 新 trait は作らない**（`shared/llm` は元々この用途の抽象境界）。要約/翻訳と同列の「LLM への新しい依頼種別」なので、ここに足すのが正しい置き場所。

---

## 6. フロントエンド設計

### 6.1 `lib/api.ts` への追加（型 4 + メソッド 8）

型（backend JSON をミラー）:

```ts
export interface Tag {
  id: string;
  name: string;
  color: string | null;
  source: "user" | "ai";
  created_at: string;
  article_count?: number; // GET /api/tags のみ付く
}

export interface ArticleTag {
  id: string;
  name: string;
  color: string | null;
  attached_source: "user" | "ai";
  confidence: number | null;
}

export interface TagSuggestion {
  name: string;
  confidence: number | null;
}
```

`api` オブジェクトにメソッド追加（既存 `http<T>()` を再利用、`動詞+リソース` 命名）:

```ts
  listTags: () => http<Tag[]>("/api/tags"),
  createTag: (body: { name: string; color?: string }) =>
    http<Tag>("/api/tags", { method: "POST", body: JSON.stringify(body) }),
  updateTag: (id: string, body: { name: string; color?: string }) =>
    http<Tag>(`/api/tags/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteTag: (id: string) => http<void>(`/api/tags/${id}`, { method: "DELETE" }),

  getArticleTags: (articleId: string) =>
    http<ArticleTag[]>(`/api/articles/${articleId}/tags`),
  setArticleTags: (articleId: string, tagIds: string[]) =>
    http<ArticleTag[]>(`/api/articles/${articleId}/tags`, {
      method: "PUT",
      body: JSON.stringify({ tag_ids: tagIds }),
    }),
  detachArticleTag: (articleId: string, tagId: string) =>
    http<void>(`/api/articles/${articleId}/tags/${tagId}`, { method: "DELETE" }),

  // AI 提案。承認はしない（候補を返すだけ）。refresh で再生成。
  suggestTags: (articleId: string, refresh = false) =>
    http<TagSuggestion[]>(
      `/api/articles/${articleId}/suggest-tags${refresh ? "?refresh=true" : ""}`,
      { method: "POST" },
    ),
```

### 6.2 `store.tsx` への追加（任意・グローバル語彙参照）

`feeds`/`folders` と同じ書き方で `tags` リソースを 1 本足す（タグピッカーが既存タグ候補を出すのに使う）:

```tsx
// UiStore に追加
  tags: Resource<Tag[]>;
  refetchTags(): void;
```

`AppProvider` 内:
```tsx
  const [tags, { refetch: refetchTags }] = createResource(() => api.listTags());
```
`useApp()` の戻り値に `tags, refetchTags: () => void refetchTags()` を含める。**未導入でも本機能は成立する**（`TagEditor` 内で `createResource` 直接でもよい）が、語彙をアプリ全体で共有するなら store が素直。

### 6.3 新規 UI 部品 `components/ui/tag-badge.tsx`

`badge.tsx` 同型の自前 Tailwind。oklch トークンで装飾。AI 由来は控えめに区別（破線枠など）。

```tsx
import { splitProps, type ComponentProps, Show } from "solid-js";
import { cn } from "@/lib/utils";

type Props = ComponentProps<"span"> & {
  color?: string | null;
  ai?: boolean;        // AI 由来エッジを薄く区別
  onRemove?: () => void;
};

export function TagBadge(props: Props) {
  const [local, rest] = splitProps(props, ["class", "color", "ai", "onRemove", "children"]);
  return (
    <span
      class={cn(
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs",
        "bg-muted text-foreground border-border",
        local.ai && "border-dashed text-muted-foreground",
        local.class,
      )}
      style={local.color ? { "border-color": local.color } : undefined}
      {...rest}
    >
      {local.children}
      <Show when={local.onRemove}>
        <button type="button" class="opacity-60 hover:opacity-100" onClick={() => local.onRemove?.()}>
          ×
        </button>
      </Show>
    </span>
  );
}
```

### 6.4 新規コンポーネント `components/TagEditor.tsx`

記事 1 件のタグ編集 UI。`ArticleView` から `articleId` を受け取る。

骨子（Solid）:
- `const [tags, { refetch }] = createResource(() => props.articleId, (id) => api.getArticleTags(id));`
- 既存タグ表示: `<For each={tags()}>` で `<TagBadge ai={t.attached_source === "ai"} onRemove={...}>{t.name}</TagBadge>`。`onRemove` は `await api.detachArticleTag(props.articleId, t.id); refetch();`。
- 追加入力: `<Input>` + 候補（`store.tags()` から filter）。Enter または候補選択で「現在の tag id 集合 + 新規」を `api.setArticleTags(articleId, ids)`。新規名は先に `api.createTag({name})` で id を得てから set、または set のバックエンド契約を「tag_ids のみ」にしているので**フロントで先に createTag → その id を含めて setArticleTags** する（2 段）。`store.refetchTags()` も呼ぶ。
- **AI 提案ボタン**: `「AI でタグ提案」` → `const sug = await api.suggestTags(props.articleId)`。`busy`/`error` シグナルでローディングとエラー（503=API キー未設定、502=生成失敗）を表示。提案を `<TagBadge ai>` のチップ列で出し、各チップにクリックで「承認」= `createTag(name)`（既存なら upsert で同一が返る）→ そのタグを記事に attach（`setArticleTags` に加える）。`「再提案」` は `api.suggestTags(id, true)`。
- 承認はユーザー操作のみ（提案を勝手に紐づけない。§2 非スコープ）。

### 6.5 `ArticleView` への組み込み（既存ファイルの最小差分）

記事本文（`prose`）の下に `<TagEditor articleId={article.id} />` を 1 行挿入する。要約/翻訳ボタン群の近く（記事メタ領域）に置くのが自然。既存ロジックは変えない。

### 6.6 タグ管理画面（任意・薄く）

`GET /api/tags`（記事数付き）を使った一覧 + 改名/削除は、`routes/Settings.tsx` か `routes/FeedManage.tsx` に「タグ」セクション（Card 1 枚）として薄く足してよい（任意・本機能の必須ではない）。MVP は `ArticleView` の `TagEditor` だけで「作成・付与・AI 提案・削除（detach）」が回る。タグ自体の物理削除 UI は管理画面側に置く。

### 6.7 Ark UI について

本機能の UI（badge / input / button / チップ列）はすべて自前 Tailwind で賄える。**Ark UI 部品は不要**。候補ドロップダウンを本格化するなら将来 Ark UI の combobox を薄くラップする余地はあるが本書では非スコープ。

---

## 7. API 契約

> すべて `/api` プレフィックス。パスパラメータは UUID。

### 7.1 `GET /api/tags` — タグ一覧（記事数付き）
レスポンス（200）:
```json
[
  { "id": "…", "name": "rust", "color": null, "source": "user",
    "created_at": "2026-06-30T…", "article_count": 12 }
]
```

### 7.2 `POST /api/tags` — タグ作成（既存名は大文字小文字無視で再利用）
リクエスト: `{ "name": "Rust", "color": "#e36" }`（`color` 任意）
レスポンス（201）: `{ "id": "…", "name": "Rust", "color": "#e36", "source": "user", "created_at": "…" }`
エラー: 400 `{ "error": "invalid input: tag name must not be empty" }`

### 7.3 `PATCH /api/tags/{id}` — 改名・色変更
リクエスト: `{ "name": "rustlang", "color": null }`
レスポンス（200）: 更新後 `Tag`。エラー: 404（対象なし）/ 400（名前不正）

### 7.4 `DELETE /api/tags/{id}` — タグ削除（関連も CASCADE 削除）
レスポンス: `204 No Content`。エラー: 404（対象なし）

### 7.5 `GET /api/articles/{id}/tags` — 記事のタグ一覧
レスポンス（200）:
```json
[ { "id": "…", "name": "rust", "color": null, "attached_source": "ai", "confidence": 0.88 } ]
```

### 7.6 `PUT /api/articles/{id}/tags` — 記事のタグ集合を一括設定
リクエスト: `{ "tag_ids": ["…", "…"] }`（user エッジを集合に同期。ai エッジは温存。§5.2）
レスポンス（200）: 設定後の `ArticleTag[]`

### 7.7 `DELETE /api/articles/{id}/tags/{tag_id}` — タグを 1 件外す
レスポンス: `204 No Content`。エラー: 404（エッジなし）

### 7.8 `POST /api/articles/{id}/suggest-tags` — AI タグ提案（キャッシュ）
クエリ: `?refresh=true` で再生成（既定はキャッシュ優先）
レスポンス（200）:
```json
[ { "name": "rust", "confidence": 0.9 }, { "name": "async", "confidence": 0.7 } ]
```
エラー:
- 404 `{ "error": "resource not found" }`（記事なし。キャッシュ無し時のみ判定）
- 503 `{ "error": "feature not yet enabled: ANTHROPIC_API_KEY is not set" }`（キー未設定、キャッシュ無し時）
- 502 `{ "error": "upstream request failed: could not parse LLM tag output: …" }`（生成/パース失敗）

> 提案は**承認しない**。承認はクライアントが `POST /api/tags`（または既存タグ）→ `PUT /api/articles/{id}/tags` で行う。

---

## 8. 依存関係

- **本機能が依存する機能**: 機能上の必須依存は無い（`tags` スライスは自己完結）。
  - `articles` テーブル/`ArticleId`（domain）を**読み取りのみ**参照（既存・存在する）。
  - `shared/llm`（`LlmClient`/`AnthropicClient`/`complete`）を再利用し `suggest_tags` を追記。
  - フロントは `ArticleView`（既存。二ペイン機能 10 で整備済み）に `TagEditor` を 1 行差し込む。
- **本機能がブロックする（土台となる）後続機能**:
  - **スマートビュー**: `GET /api/articles?tag=...` か新スライスでタグ条件の記事絞り込み。`article_tags`（`idx_article_tags_tag_id` 済み）が前提。
  - **ダイジェスト**: タグ別の定期まとめ。`shared/scheduler.rs` + 本タグ基盤。
  - **自動ルール**: フィード/キーワード→自動タグ。`article_tags.source` に `'rule'` を足す拡張（CHECK 制約を新マイグレーションで更新）。
- 既存スライス（feeds/articles/folders/instapaper/search/health）への変更は**無し**。接触点は `features/mod.rs` の 2 行と `shared/llm`（trait 1 メソッド + 実装 1 ブロック）のみ。

---

## 9. テスト計画（TDD）

> 配置方針は instapaper 設計（05）と同じ二段: ドメイン純粋関数は同モジュール `#[cfg(test)]`、リポジトリ往復は `repository.rs` 内 `#[cfg(test)]` + 実 DB `#[ignore]`、HTTP 表面は shell スクリプト。binary crate（`lib.rs` 無し）なので `backend/tests/` 別クレートからは内部関数を呼べない前提。

### 9.1 単体テスト（`#[cfg(test)] mod tests` in `domain.rs`、外部依存なし）

`backend/src/features/tags/domain.rs` 末尾。Red を先に書く。

| テスト | 意図 |
|---|---|
| `tag_name_rejects_empty` | 空・空白のみを `Err` |
| `tag_name_trims_and_collapses_whitespace` | `"  rust   lang "` → `"rust lang"` |
| `tag_name_rejects_too_long` | 51 文字超を `Err` |
| `tag_name_accepts_valid` | 正常系で `as_str()` 取得 |
| `normalize_name_collapses_internal_spaces` | 内部連続空白を 1 個に |
| `parse_suggestions_parses_plain_array` | 素の JSON 配列をパース |
| `parse_suggestions_strips_prose_and_fences` | 前後の説明文/```json フェンス付きでも `[`〜`]` を抽出 |
| `parse_suggestions_dedupes_case_insensitive` | `"Rust"`/`"rust"` を 1 件に |
| `parse_suggestions_drops_empty_names` | 空名要素を捨てる |
| `parse_suggestions_truncates_to_max` | `max_tags` 件に切り詰め |
| `parse_suggestions_clamps_confidence` | `1.5`→`1.0`, `-0.2`→`0.0` |
| `parse_suggestions_errors_on_no_array` | 配列が無い出力は `Err` |
| `parse_suggestions_errors_on_malformed_json` | 壊れた JSON は `Err` |

### 9.2 リポジトリ往復テスト（`#[cfg(test)] mod tests` in `repository.rs`、実 DB / `#[ignore]`）

`DATABASE_URL` で実 DB に接続（`just dev-db`、マイグレーション適用済み前提）。`#[tokio::test]` + `#[ignore]`。雛形は 05 の §9.2 と同型（`PgPoolOptions::new().max_connections(1).connect(...)`）。

| テスト | 意図 |
|---|---|
| `tag_upsert_is_case_insensitive` | `upsert_tag("Rust")` 後 `upsert_tag("rust")` が**同一 id** を返す（`lower(name)` 制約） |
| `list_tags_returns_article_count` | タグ作成 + 記事に attach → `list_tags` の `article_count` が一致 |
| `attach_then_list_article_tags` | attach（source/confidence 指定）→ `list_article_tags` で値が往復 |
| `attach_is_idempotent` | 同一 (article,tag) 2 回 attach で重複行にならず source/confidence が更新 |
| `set_article_tags_replaces_user_keeps_ai` | ai エッジ 1 + user エッジ 2 の状態で `set_article_tags([t3])` → user は t3 のみ、ai は残る |
| `detach_returns_zero_when_absent` | 無いエッジの detach は `rows_affected()==0` |
| `suggestions_cache_roundtrip` | `save_suggestions` → `get_cached_suggestions` で配列が往復、再 save で上書き |
| `delete_tag_cascades_edges` | タグ削除で `article_tags` の該当行も消える（FK CASCADE） |

> テスト後は作成データを削除して決定的に（`DELETE FROM article_tags ...`, `DELETE FROM tags WHERE name ILIKE 'test-%'` 等、テスト用プレフィックスで隔離）。

### 9.3 HTTP スモークテスト（shell、稼働スタックへ curl）

`scripts/test/api-tags.sh` を新設（`scripts/test/api-stats.sh` と同型、nginx 経由）。**Claude を叩かない範囲**を検証:

| 手順 / アサーション | 意図 |
|---|---|
| `POST /api/tags {"name":"test-rust"}` → 201 + `id` あり | 作成 + スライス合成 |
| `POST /api/tags {"name":"TEST-RUST"}` → 201 で**同一 id** | 大文字小文字無視の upsert |
| `POST /api/tags {"name":"  "}` → 400 | 空名バリデーション |
| `GET /api/tags` → 200 で配列、上記タグを含む | 一覧 + `article_count` キー存在 |
| `PATCH /api/tags/{id} {"name":"test-rustlang"}` → 200 | 改名 |
| `DELETE /api/tags/{id}` → 204、再 `DELETE` → 404 | 削除 + 冪等でない 404 |
| `GET /api/articles/{存在しない uuid}/tags` → 200 で `[]` | 記事に紐づくタグが無いケース（空配列） |
| `POST /api/articles/00000000-…/suggest-tags`（キー未設定環境）→ 503 | `NotEnabled` 配線（Claude 到達前に弾く） |

> `suggest-tags` の成功パスは実 Claude が要るため CI 自動化しない。手動手順は §10 step 11。

### 9.4 フロント（型 + 手動）
- `just lint` の `tsc` で `api.ts`/`TagEditor.tsx`/`tag-badge.tsx`/`store.tsx` の型整合を確認。
- 手動: 記事を開く → タグ手入力で付与/削除 → 「AI でタグ提案」→ 候補表示 → 承認で記事に付く → 「再提案」でキャッシュ更新。キー未設定時は 503 がエラー表示される。

---

## 10. 実装手順（順序付きチェックリスト）

1. **マイグレーション番号を採番**: `ls backend/migrations/` で最大番号を確認（執筆時最新 `0005_search.sql` → **`0006_tags.sql`**）。並行作業が先取りしていれば +1 へ繰り上げ。
2. **マイグレーション作成**: `backend/migrations/0006_tags.sql` を §4.2 の SQL で新規作成（既存は触らない）。
3. **`shared/llm` 拡張（Red 先行不可の境界だが小さい）**: `shared/llm/mod.rs` に `SuggestTagsRequest` と trait メソッド `suggest_tags` を追加、`anthropic.rs` に実装（§5.8）。`complete()` を再利用。
4. **ドメイン（Red 先行）**: `backend/src/features/tags/domain.rs` を §5.1 で作成。§9.1 の `#[cfg(test)] mod tests` を先に書き、落ちる→実装で Green。`cargo test`（DB 不要）で実行。
5. **repository**: `repository.rs` を §5.2 で作成（`query`/`query_as` のみ、`query!` 不可）。§9.2 の `#[cfg(test)]`（`#[ignore]`）も書く。
6. **service**: `service.rs` を §5.3 で作成。`llm_client` は articles から複製（import しない）。
7. **handler**: `handler.rs` を §5.4 で作成。`TagName::parse` の `Err(String)` を `map_err(AppError::Validation)`。
8. **mod + 合成**: `mod.rs` を §5.5 で作成。`features/mod.rs` に `pub mod tags;` と `.merge(tags::routes())` を追加（§5.6）。
9. **ビルド & lint**: `just lint`（clippy `-D warnings` / tsc）。trait に新メソッドを足したので `AnthropicClient` 実装漏れがないこと、テストモックがあれば（無ければ不要）更新。
10. **DB 起動 & マイグレーション**: `just dev-db` →（バックエンド起動で自動 migrate、または `just migrate`）。`DATABASE_URL=... cargo test -- --ignored` で §9.2 を Green に。
11. **HTTP スモーク**: `scripts/test/api-tags.sh` を §9.3 で作成・`chmod +x`・実行。
12. **フロント**: `lib/api.ts`（型 4・メソッド 8、§6.1）、`components/ui/tag-badge.tsx`（§6.3）、`components/TagEditor.tsx`（§6.4）を作成。`ArticleView` に `<TagEditor>` を 1 行挿入（§6.5）。必要なら `store.tsx` に `tags` リソース（§6.2）。`just lint` の tsc を通す。
13. **手動 E2E**: `ANTHROPIC_API_KEY` を設定し、記事で手動タグ付与/削除 → AI 提案 → 承認 → 再提案（キャッシュ更新）を確認。キー未設定で 503 表示も確認。
14. **コミット**: マイグレーション・スライス・`shared/llm` 追記・スクリプト・フロントをまとめて。`.env`/キーはコミットしない。

---

## 11. リスク・未決事項・代替案

- **【最重要】LLM 出力の非決定性**: Claude は JSON 以外（説明文・```json フェンス・余分なキー）を返しうる。`parse_tag_suggestions` で `[`〜`]` 抽出・余分キー無視（`serde` の既定）・パース失敗時 502 で安全側に倒す。プロンプト（§5.8）は「ONLY a JSON array」「no code fences」を明示するが、**実装時に数記事で実出力を観察し system プロンプトを微調整**すること。`max_tokens=1024`（`complete` 既定）で足りるが、提案数が多い記事では切れる可能性あり（その場合 §5.8 で `complete` を使わず専用に `max_tokens` を上げる版を slice 内に持つ拡張余地）。
- **語彙の一貫性は DB が最終保証**: プロンプトに既存語彙を渡しても Claude が表記ゆれ（`Rust`/`rust`/`rustlang`）を出すことはある。`lower(name)` ユニークインデックス + `upsert_tag` の `ON CONFLICT` で同一視するのが最終防波堤。語彙が巨大化したらプロンプトに全語彙を載せきれなくなる（トークン上限）。緩和: 件数が増えたら「よく使う上位 N 件 + 記事と類似のタグ」を渡す（将来。本書は全件 ORDER BY name）。
- **マイグレーション番号の順序ハザード**: §4.1。`run_migrations` は `set_ignore_missing` 非設定。着手直前に最新番号を確認し最小空き整数を取る。`0006` が先取りされていたら繰り上げる。
- **`set_article_tags` の ai エッジ温存ポリシー**: 本書は「PUT は user エッジのみ集合同期、ai エッジは温存」とした（AI が付けたタグを PUT のたびに消さない）。もし UI が「表示中の全タグ＝あるべき集合」を送る設計なら、ai/user 区別なく全置換する版に変える（§5.2 のクエリから `source='user'` 条件と挿入時固定 `'user'` を外す）。**UI 実装と齟齬が出ないよう、フロントの送信内容（user タグのみか全タグか）と合わせること**。本書のフロント（§6.4）は detach を個別 API で行い、PUT は user 追加に使う前提。
- **記事本文が大きい場合のトークンコスト**: 提案は記事全文を送る。長文記事ではコスト増。緩和: 要約（`articles.summary`）があればそれを優先して送る案があるが、articles への結合を増やすため本書では素の content を送る（必要なら `get_article_text` で `COALESCE(summary, content)` 的に切替可能。articles 読み取りのみで実現でき結合は増えない）。
- **削除 UX**: `DELETE /api/articles/{id}/tags/{tag_id}` は記事からエッジを外すだけ（タグ語彙は残る）。タグ自体の物理削除（`DELETE /api/tags/{id}`）は全記事から外れる破壊操作なので、管理画面（§6.6）で確認ダイアログ（`dialog.tsx`）を出すこと。
- **`confidence` の信頼度**: Claude の自己申告 confidence は較正されていない。並び順のヒント程度に使い、UI のしきい値（自動承認等）には使わない（本書は自動承認しない）。
- **キャッシュの陳腐化**: 記事本文は基本不変なので提案キャッシュは安全。ただし語彙が増えると「今なら既存タグに寄せた提案」が出るはずの記事も古い提案を返す。`?refresh=true` で明示再生成できる。自動無効化（語彙更新時に全キャッシュ破棄）は過剰なので行わない。
