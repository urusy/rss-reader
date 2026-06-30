# 28 カスタムルールエンジン (If/Then 自動化)

> 読み手前提: リポジトリは手元にあるが、この設計会話の文脈は知らない別セッションの実装者。本書1枚で着手・完了できるよう、再利用資産・完全な SQL・Rust 型と関数シグネチャ・サービス手順・API 契約（JSON 例）・フロント変更・TDD ケース・番号付き実装手順・リスクまで具体化する。
> 確認済み実ファイル: `backend/src/features/mod.rs`, `backend/src/features/feeds/service.rs`（クロール `fetch_and_store`）, `backend/src/features/articles/repository.rs`（`upsert` / `set_read` / `list`）, `backend/src/shared/{error,state,scheduler}.rs`, `backend/migrations/`（最新 `0005_search.sql`）, `backend/Cargo.toml`（`serde_json` 有り・**sqlx に `json` feature は無い**）, `frontend/src/lib/api.ts`, `docs/design/{03,05,19,24,32}-*.md`。

---

## 1. 概要

ユーザー定義の **If/Then ルール**で記事を自動処理する。各ルールは「**条件（IF）**」と「**アクション（THEN）**」の組で、

- **IF**: `keyword`（タイトル/本文の語）・`author`（著者）・`feed`（特定フィード集合）・`tag`（付与済みタグ）・`date`（公開日の新旧）を、**AND（すべて一致）/ OR（いずれか一致）**で組み合わせる。
- **THEN**: `mark_read`（既読化）・`tag`（タグ付与）・`star`（スター）・`save`（後で読む=Instapaper 送信）・`score`（スコア加減）。

ルールは **クロールの upsert 経路**（`feeds::service::fetch_and_store`）で新規取り込み記事に対して実行される。これにより「特定キーワードを含む記事を自動で既読にする」「あるフィードの記事に自動でタグを付ける」等が無人で行える。

実装は新スライス **`backend/src/features/automation_rules/`** 1枚 ＋ `features/mod.rs` に `.merge()` 1行（Vertical Slice 厳守）。ルール定義は `automation_rules` テーブルに置き、**条件/アクションは JSON 文字列（TEXT 列）**で保持する（sqlx に `json` feature を足さないため。§4 設計判断）。条件評価は**純粋関数**に切り出して TDD で網羅する。フロントは新ルート `/rules` に**ルールビルダー UI**を置く。

**既存機能との関係（重要）**: ミュート（機能19・`hide`/`mark_read`）と自動タグ付け（機能24・タグ付与）は、本ルールエンジンの **部分集合**（特定の条件×アクションの特殊形）である。本書では19/24 を置き換えず**共存**させ、将来それらを本エンジンの設定へ統合する（§8・§11）。AI を使う条件/アクションは本 v1 では導入しない（決定論的なルールのみ）。将来 AI スコアリング等を足す場合は `shared/llm` を再利用し DB キャッシュ + `ANTHROPIC_API_KEY` 未設定で `AppError::NotEnabled` とする（§11）。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）
- マイグレーション **`0006_automation_rules.sql`**（暫定番号。**着手前に `ls backend/migrations/` で最新番号を確認**。本書執筆時点の最新は `0005_search.sql` なので +1 = `0006`。並行作業が先に 0006 を取ったら 0007 以降へ繰り上げ）:
  - `automation_rules` テーブル（`conditions` / `actions` を JSON 文字列の TEXT 列で保持）。
  - `article_scores` テーブル（`score` アクションの書き込み先。**本スライス所有**、`articles` に列を漏らさない）。
  - `articles.author`（著者照合用・追記カラム）と `articles.rules_applied_at`（未処理記事の再処理防止・追記カラム）。
- 新スライス `backend/src/features/automation_rules/`（`domain` / `repository` / `service` / `handler` / `mod`）。
- 条件評価の**純粋関数**（`rule_matches` / `match_condition`）と検証関数（`validate_conditions` / `validate_actions`）＋単体テスト。
- ルール CRUD API（`GET/POST /api/rules`, `GET/PUT/DELETE /api/rules/{id}`）、ドライラン `POST /api/rules/{id}/test`、全件再適用 `POST /api/rules/apply`。
- クロール経路への実行フック（`feeds::service::fetch_and_store` の末尾に**1行**追加）と、著者保存のための `articles::repository::upsert` への**追記的**引数追加（同一アグリゲートへの additive 変更。§5.7）。
- フロント: `lib/api.ts` に型 + 7 メソッド、`routes/Rules.tsx`（一覧 + ビルダー）、`components/rules/RuleBuilder.tsx`、`/rules` ルート。
- `scripts/test/api-rules.sh`（HTTP スモーク）。

### 非スコープ（本機能では実装しない）
- 既存ミュート（19）/自動タグ（24）の本エンジンへの**統合・移行**（共存に留める。統合は将来チケット。§11）。
- AI を使う条件/アクション（感情分析・LLM スコアリング等）。v1 は決定論ルールのみ。
- ルールの実行履歴・監査ログ・アクションの取り消し（undo）。
- 複雑なネスト条件（条件グループの入れ子）。v1 は単一階層の AND/OR のみ。
- 正規表現マッチ。v1 は大小無視の部分一致（`contains`）のみ（`match_type` は前方互換で列に持たず、将来拡張時に追加）。
- リアルタイム再適用（ルール編集の瞬間に既存全記事へ即時反映）。再適用は明示的 `POST /api/rules/apply` で行う。

---

## 3. 既存実装の調査と再利用

**車輪の再発明をしない。** 実ファイルを確認済み。

| 再利用資産 | 実体（確認済み） | 本機能での使い方 |
|---|---|---|
| スライス構成 + `routes()` | `feeds/`・`articles/`・`folders/`（`domain/repository/service/handler/mod`、`fn routes() -> Router<AppState>`） | 同じ5ファイルで `automation_rules` を作る |
| `features/mod.rs` 合成 | `pub mod ...;` + `.merge(...::routes())`（現在8スライス） | `pub mod automation_rules;` と `.merge(automation_rules::routes())` を1行ずつ追加。既存は触らない |
| 主キー newtype | `feeds/domain.rs::FeedId`、`articles/domain.rs::ArticleId`（`#[derive(.. sqlx::Type)] #[sqlx(transparent)]`） | `RuleId(Uuid)` を同型で新設 |
| 値オブジェクト `parse()->Result<_,String>` | `feeds/domain.rs::FeedUrl::parse`（`#[cfg(test)]` 付き） | `RuleName::parse` を同型で新設。`Err(String)` は `map_err(AppError::Validation)` |
| `AppError` 6 バリアント | `shared/error.rs`（`NotFound`/404, `Validation`/400, `NotEnabled`/503, `Upstream`/502, `Database`/500, `Other`/500、`IntoResponse` で `Json({"error": <Display>})`） | 新バリアントを足さず既存で表現（§5.8）。`error.rs` は編集しない |
| 任意機能 = `NotEnabled` | `articles/service.rs` の LLM ゲート、`instapaper/service.rs::add_to_read_later`（資格情報無しで `NotEnabled`） | `save` アクションは `instapaper::service::add_to_read_later` を呼び、未設定時の `NotEnabled` を**握って warn し継続**（クロールを止めない。§5.5） |
| クロール upsert 経路 | `feeds/service.rs::fetch_and_store`（`parser::parse` → `articles::repository::upsert` ループ → `touch_fetched`） | 末尾に `automation_rules::service::apply_for_feed(state, feed.id.0)` を1行追加（実行フック）。著者を `entry.authors.first()` から取り `upsert` に渡す（§5.7） |
| クロステーブル read を自スライス内 SQL で | `instapaper/repository.rs::get_article_ref`（`SELECT ... FROM articles`）、`feed_overview`（feeds+articles JOIN） | `automation_rules` から `articles` を**読み取り専用 SQL**で引く（未処理記事の取得）。タグ照合は `article_tags`/`tags`（機能24）を LEFT JOIN（ソフト依存・§5.4） |
| 既読化の既存経路 | `articles/repository.rs::set_read` / `mark_all_read`（`UPDATE articles SET is_read=...`） | `mark_read` アクションは `articles::repository::set_read(pool, ArticleId, true)` を再利用（既存関数。新規 UPDATE を増やさない） |
| upsert の ON CONFLICT パターン | `articles/repository.rs::upsert`（`INSERT ... ON CONFLICT (url) DO UPDATE`）、`instapaper`（`ON CONFLICT (id)`） | `article_scores` の加点は `INSERT ... ON CONFLICT (article_id) DO UPDATE SET score = article_scores.score + EXCLUDED.score` |
| JSON シリアライズ | `Cargo.toml` に `serde_json = "1"`（`shared/llm/anthropic.rs` が `serde_json::json!` 使用） | 条件/アクションを `serde_json::{to_string,from_str}` で TEXT 列と相互変換（**sqlx の `json` feature 不要**。§4） |
| フロント API クライアント | `frontend/src/lib/api.ts`（`http<T>()` は 204→undefined 畳み込み、`api` に `動詞+リソース` で集約） | 既存 `http<T>()` をそのまま使い型 + 7 メソッド追加 |
| 自前 UI 部品 + Ark UI | `components/ui/{button,card,input,dialog,switch,badge}.tsx`（自前 Tailwind / Ark UI ラップ）、oklch トークン（`bg-background` 等） | ルールビルダーは `Input`/`Button`/`Card`/`switch` と Ark UI の Select を使う（§6） |
| HTTP スモークの慣習 | `scripts/test/api-*.sh`（稼働スタック nginx `:8081` に curl、HTTP コード + JSON キーを assert） | `scripts/test/api-rules.sh` を同型で新設（§9.3） |

> **sqlx に `json` feature が無い件（確認済み・設計に反映）**: `Cargo.toml` の sqlx features は `runtime-tokio, tls-rustls, postgres, uuid, chrono, macros, migrate` で `json` は**無い**。よって `serde_json::Value` / `sqlx::types::Json<T>` を JSONB 列に直接バインドできない。**条件/アクションは TEXT 列に JSON 文字列として保存**し、アプリ層で `serde_json` を使って相互変換する（Cargo.toml 変更を避ける）。jsonb 化は将来 `json` feature を足す別チケットで（§11）。

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方（必読）

`main.rs` の `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を呼ばないため、**適用済み最大バージョンより小さい未適用マイグレーションを後から追加すると起動時に `VersionMissing`（out-of-order）でエラー**になる。

**ルール**: 着手前に `ls backend/migrations/` で最新番号を確認し **最大番号 +1** を採る。本書執筆時点の最新は `0005_search.sql` なので**暫定的に `0006_automation_rules.sql`**。並行作業（apalis 移行・機能19/24 等が先に 0006 を取った場合）は `0007` 以降へ繰り上げる。**既存マイグレーションは編集しない（追記のみ）。**

### 4.2 スキーマ

新規ファイル **`backend/migrations/0006_automation_rules.sql`**（番号は §4.1 で確認）。完全文:

```sql
-- 0006_automation_rules.sql
-- カスタムルールエンジン (If/Then 自動化)。
--
-- automation_rules: ユーザー定義のルール。conditions/actions は JSON 文字列(TEXT)で保持する。
--   sqlx に json feature を足さずに済ませるため列型は jsonb ではなく TEXT とし、アプリ層
--   (serde_json) でシリアライズ/デシリアライズする(§4 設計判断)。
-- article_scores: score アクションの書き込み先(本スライス所有・articles に列を漏らさない)。
-- articles.author / articles.rules_applied_at: クロールで著者を保存し、未処理記事を
--   再処理しないための追記カラム(ADD COLUMN IF NOT EXISTS で再実行に冪等)。

CREATE TABLE IF NOT EXISTS automation_rules (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT        NOT NULL CHECK (length(btrim(name)) > 0),
    enabled     BOOLEAN     NOT NULL DEFAULT true,
    position    INTEGER     NOT NULL DEFAULT 0,   -- 実行順(昇順、小さいほど先)
    conditions  TEXT        NOT NULL,             -- JSON: {"combinator":"all","items":[...]}
    actions     TEXT        NOT NULL,             -- JSON: [{"kind":"mark_read"}, ...]
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 有効ルールだけを実行順に走査する apply 用。
CREATE INDEX IF NOT EXISTS idx_automation_rules_enabled
    ON automation_rules (enabled, position) WHERE enabled = true;

-- score アクションの蓄積先。記事1件につき1行(冪等加点)。
CREATE TABLE IF NOT EXISTS article_scores (
    article_id UUID        PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    score      INTEGER     NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 著者照合用(クロールで feed-rs の entry.authors から保存)。NULL = 著者不明。
ALTER TABLE articles ADD COLUMN IF NOT EXISTS author TEXT;

-- ルール適用済みスタンプ。NULL = 未適用(次回クロール後 apply の対象)。
ALTER TABLE articles ADD COLUMN IF NOT EXISTS rules_applied_at TIMESTAMPTZ;

-- 未適用記事をフィード単位で素早く拾う部分インデックス。
CREATE INDEX IF NOT EXISTS idx_articles_rules_pending
    ON articles (feed_id) WHERE rules_applied_at IS NULL;
```

設計判断:
- **`conditions`/`actions` を TEXT(JSON) にする理由**: 条件/アクションは可変構造で正規化に向かず、JSON が自然。sqlx に `json` feature が無いため `jsonb` 直バインドは不可（§3 末尾）。TEXT + `serde_json` でアプリ層変換すれば Cargo.toml を変えずに済む。検索・集計の必要が出たら将来 `jsonb` へ移行する（§11）。
- **`article_scores` を別テーブルにする理由**: スコアは本エンジン固有の関心。`articles` に `score` 列を足すと articles アグリゲートに他スライスの関心が漏れる（instapaper/tags が `articles` に列を足さず別テーブルにしたのと同じ判断）。`ON DELETE CASCADE` で記事削除時に孤立行を残さない。
- **`articles.author` は追記カラム（クロールで populate）**: 著者照合は元データ（フィード）からの保存が必須。`feeds::service::fetch_and_store` が `entry.authors.first()` を `upsert` 経由で保存する additive 変更（§5.7）。`SELECT *`（`articles::repository::list` が使用）は新列を自動で拾い、`Article` 構造体に列追加しなくても **未知カラムは sqlx FromRow が無視**するため既存は壊れない（本スライスは著者を自前 SELECT で読む。§5.4）。
- **`articles.rules_applied_at` で再処理を防ぐ**: クロールのたびに全記事を再評価しないよう、適用済み記事に now() を立てる。`apply_for_feed` は `rules_applied_at IS NULL` の記事だけを処理。全件再適用は `POST /api/rules/apply`（§5.3）が `rules_applied_at` を無視して全記事に当てる。
- **`gen_random_uuid()` 既定**: 機能19/24 の前例どおり pgcrypto 由来の関数を既定値に使う（DB 生成）。アプリ側 newtype は読み取り時に `Uuid` を受けるだけ。

`feeds` への列追加は無い。

---

## 5. バックエンド設計

新スライス **`backend/src/features/automation_rules/`**。5ファイル構成。

### 5.1 `domain.rs`（型 + 条件評価/検証の純粋関数 = 単体テスト対象）

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ルール主キー newtype（FeedId と同型）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct RuleId(pub Uuid);

/// 検証済みルール名。
#[derive(Debug, Clone)]
pub struct RuleName(String);

impl RuleName {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into().trim().to_string();
        if s.is_empty() {
            return Err("rule name must not be empty".into());
        }
        if s.chars().count() > 100 {
            return Err("rule name too long (max 100)".into());
        }
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 条件の結合子（AND / OR）。JSON では "all" / "any"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Combinator {
    All, // AND: すべての条件が一致
    Any, // OR : いずれかの条件が一致
}

/// keyword 条件の照合対象。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeywordTarget {
    Title,
    Content,
    Any, // title または content
}

/// date 条件の比較演算子。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOp {
    OlderThanDays, // published_at が now - days より古い
    NewerThanDays, // published_at が now - days より新しい
}

/// 1個の条件。JSON は `{"field":"keyword", ...}` のように `field` で tag 付け。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum Condition {
    Keyword {
        target: KeywordTarget,
        value: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    Author {
        value: String,
    },
    Feed {
        feed_ids: Vec<Uuid>,
    },
    Tag {
        tag: String, // タグ名（大小無視で照合。機能24 のタグ。ソフト依存）
    },
    Date {
        op: DateOp,
        days: i64,
    },
}

/// 条件の集合 + 結合子。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conditions {
    pub combinator: Combinator,
    pub items: Vec<Condition>,
}

/// アクション。JSON は `{"kind":"mark_read"}` / `{"kind":"score","delta":5}` 等。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    MarkRead,
    Star,                  // 機能32（スター）ソフト依存
    Tag { name: String },  // 機能24（タグ）ソフト依存
    Save,                  // 後で読む = Instapaper（機能05/06）ソフト依存
    Score { delta: i32 },  // article_scores に加減算
}

/// 1記事の評価コンテキスト（DB から組み立てる。now を注入してテスト可能に）。
pub struct ArticleCtx<'a> {
    pub title: &'a str,
    pub content: &'a str,
    pub url: &'a str,
    pub author: Option<&'a str>,
    pub feed_id: Uuid,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub tags: &'a [String], // 小文字化済みタグ名。機能24 未マージなら空スライス
    pub now: chrono::DateTime<chrono::Utc>,
}

fn contains(haystack: &str, needle: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        haystack.contains(needle)
    } else {
        haystack.to_lowercase().contains(&needle.to_lowercase())
    }
}

/// 1条件の判定（純粋関数）。
pub fn match_condition(c: &Condition, ctx: &ArticleCtx) -> bool {
    match c {
        Condition::Keyword { target, value, case_sensitive } => match target {
            KeywordTarget::Title => contains(ctx.title, value, *case_sensitive),
            KeywordTarget::Content => contains(ctx.content, value, *case_sensitive),
            KeywordTarget::Any => {
                contains(ctx.title, value, *case_sensitive)
                    || contains(ctx.content, value, *case_sensitive)
            }
        },
        Condition::Author { value } => ctx
            .author
            .map(|a| contains(a, value, false))
            .unwrap_or(false),
        Condition::Feed { feed_ids } => feed_ids.contains(&ctx.feed_id),
        Condition::Tag { tag } => {
            let want = tag.to_lowercase();
            ctx.tags.iter().any(|t| *t == want)
        }
        Condition::Date { op, days } => match ctx.published_at {
            None => false, // 公開日不明は date 条件に一致しない
            Some(p) => {
                let age_days = (ctx.now - p).num_days();
                match op {
                    DateOp::OlderThanDays => age_days > *days,
                    DateOp::NewerThanDays => age_days <= *days,
                }
            }
        },
    }
}

/// ルール全体の一致（結合子で畳む。純粋関数）。
/// items は検証で非空が保証される（空は validate で弾く）。
pub fn rule_matches(conds: &Conditions, ctx: &ArticleCtx) -> bool {
    match conds.combinator {
        Combinator::All => conds.items.iter().all(|c| match_condition(c, ctx)),
        Combinator::Any => conds.items.iter().any(|c| match_condition(c, ctx)),
    }
}

/// 条件の妥当性検査（保存前。純粋関数 = テスト対象）。
pub fn validate_conditions(conds: &Conditions) -> Result<(), String> {
    if conds.items.is_empty() {
        return Err("at least one condition is required".into());
    }
    for c in &conds.items {
        match c {
            Condition::Keyword { value, .. } | Condition::Author { value } => {
                if value.trim().is_empty() {
                    return Err("condition value must not be empty".into());
                }
            }
            Condition::Tag { tag } => {
                if tag.trim().is_empty() {
                    return Err("tag must not be empty".into());
                }
            }
            Condition::Feed { feed_ids } => {
                if feed_ids.is_empty() {
                    return Err("feed condition needs at least one feed".into());
                }
            }
            Condition::Date { days, .. } => {
                if *days < 0 {
                    return Err("days must be non-negative".into());
                }
            }
        }
    }
    Ok(())
}

/// アクションの妥当性検査（保存前。純粋関数 = テスト対象）。
pub fn validate_actions(actions: &[Action]) -> Result<(), String> {
    if actions.is_empty() {
        return Err("at least one action is required".into());
    }
    for a in actions {
        match a {
            Action::Tag { name } => {
                if name.trim().is_empty() {
                    return Err("tag action needs a non-empty name".into());
                }
            }
            Action::Score { delta } => {
                if *delta == 0 {
                    return Err("score delta must not be zero".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
```

> ステータス分類や条件評価を**純粋関数に切り出す**のは、DB も外部 API も叩かずに TDD（Red→Green）を回すため（MEMORY「書いたら必ず実行」「バグ修正もテスト先行」）。`now` を `ArticleCtx` に注入することで date 条件を固定時刻でテストできる。

### 5.2 `repository.rs`（`&PgPool` を取る free async fn、ランタイムクエリのみ）

```rust
use sqlx::PgPool;
use uuid::Uuid;

use crate::features::articles::domain::ArticleId;
use crate::shared::error::AppResult;

/// DB 行（conditions/actions は JSON 文字列のまま受ける。パースは service/domain で）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuleRow {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub position: i32,
    pub conditions: String, // JSON
    pub actions: String,    // JSON
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// 未適用記事の読み取り射影（本スライス内に閉じた read-only projection）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingArticle {
    pub id: Uuid,
    pub feed_id: Uuid,
    pub title: String,
    pub content: String,
    pub url: String,
    pub author: Option<String>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn list_all(pool: &PgPool) -> AppResult<Vec<RuleRow>> {
    let rows = sqlx::query_as::<_, RuleRow>(
        "SELECT * FROM automation_rules ORDER BY position ASC, created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_enabled(pool: &PgPool) -> AppResult<Vec<RuleRow>> {
    let rows = sqlx::query_as::<_, RuleRow>(
        "SELECT * FROM automation_rules WHERE enabled = true ORDER BY position ASC, created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get(pool: &PgPool, id: Uuid) -> AppResult<Option<RuleRow>> {
    let row = sqlx::query_as::<_, RuleRow>("SELECT * FROM automation_rules WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn insert(
    pool: &PgPool,
    name: &str,
    enabled: bool,
    position: i32,
    conditions_json: &str,
    actions_json: &str,
) -> AppResult<RuleRow> {
    let row = sqlx::query_as::<_, RuleRow>(
        r#"INSERT INTO automation_rules (name, enabled, position, conditions, actions)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(name)
    .bind(enabled)
    .bind(position)
    .bind(conditions_json)
    .bind(actions_json)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    enabled: bool,
    position: i32,
    conditions_json: &str,
    actions_json: &str,
) -> AppResult<Option<RuleRow>> {
    let row = sqlx::query_as::<_, RuleRow>(
        r#"UPDATE automation_rules
           SET name = $2, enabled = $3, position = $4,
               conditions = $5, actions = $6, updated_at = now()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(name)
    .bind(enabled)
    .bind(position)
    .bind(conditions_json)
    .bind(actions_json)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM automation_rules WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// 未適用記事を1フィード分取得（apply_for_feed 用）。
pub async fn fetch_pending(
    pool: &PgPool,
    feed_id: Uuid,
    limit: i64,
) -> AppResult<Vec<PendingArticle>> {
    let rows = sqlx::query_as::<_, PendingArticle>(
        r#"SELECT id, feed_id, title, content, url, author, published_at
           FROM articles
           WHERE feed_id = $1 AND rules_applied_at IS NULL
           ORDER BY created_at ASC
           LIMIT $2"#,
    )
    .bind(feed_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 全記事を取得（POST /api/rules/apply の再適用 + テストの母集合に使う）。
pub async fn fetch_all_articles(pool: &PgPool, limit: i64) -> AppResult<Vec<PendingArticle>> {
    let rows = sqlx::query_as::<_, PendingArticle>(
        r#"SELECT id, feed_id, title, content, url, author, published_at
           FROM articles
           ORDER BY created_at DESC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 処理済みスタンプを立てる（再処理防止）。
pub async fn mark_applied(pool: &PgPool, ids: &[Uuid]) -> AppResult<()> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE articles SET rules_applied_at = now() WHERE id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await?;
    Ok(())
}

/// 記事の小文字化済みタグ名を取得（機能24 のテーブルが無ければ空 Vec を返す）。
/// to_regclass で存在チェックしてから引くため、24 未マージでもエラーにしない。
pub async fn tags_for(pool: &PgPool, article_id: Uuid) -> AppResult<Vec<String>> {
    let exists: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public.article_tags')::text")
            .fetch_one(pool)
            .await?;
    if exists.is_none() {
        return Ok(Vec::new());
    }
    let tags: Vec<String> = sqlx::query_scalar(
        r#"SELECT lower(t.name)
           FROM article_tags at JOIN tags t ON t.id = at.tag_id
           WHERE at.article_id = $1"#,
    )
    .bind(article_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default(); // 列構成差にも頑健に（best-effort）
    Ok(tags)
}

/// score アクション: 冪等加点（行が無ければ作る）。
pub async fn bump_score(pool: &PgPool, article_id: Uuid, delta: i32) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO article_scores (article_id, score, updated_at)
           VALUES ($1, $2, now())
           ON CONFLICT (article_id) DO UPDATE
             SET score = article_scores.score + EXCLUDED.score,
                 updated_at = now()"#,
    )
    .bind(article_id)
    .bind(delta)
    .execute(pool)
    .await?;
    Ok(())
}

/// 参考: mark_read は既存 articles::repository::set_read を再利用する（新規 UPDATE を増やさない）。
/// 呼び出し側は ArticleId::from(uuid) を渡す。
pub fn article_id(uuid: Uuid) -> ArticleId {
    ArticleId(uuid)
}
```

> `query!` コンパイル時マクロは使わない（ビルドに DB 接続が要るため禁止）。すべて `query`/`query_as`/`query_scalar` のランタイムクエリ。`articles` を読むのは **読み取り専用射影**で、`instapaper::get_article_ref` や `feed_overview` の前例どおり「越境共通レイヤー」には当たらない（articles の書き込み所有は移さない。`mark_read` は articles スライスの既存公開関数 `set_read` を呼ぶ）。

### 5.3 `service.rs`（`&AppState` を取り repository + アクションを統合）

```rust
use uuid::Uuid;

use super::domain::{
    self, Action, ArticleCtx, Conditions, RuleName,
};
use super::repository::{self, PendingArticle, RuleRow};
use crate::features::articles;
use crate::features::instapaper;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;
use serde::Serialize;

/// API 公開形（conditions/actions をパース済みで返す）。
#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub position: i32,
    pub conditions: Conditions,
    pub actions: Vec<Action>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// DB 行 → API 形（JSON 文字列をパース。壊れた JSON は 500 ではなく明示エラー）。
fn parse_row(row: RuleRow) -> AppResult<Rule> {
    let conditions: Conditions = serde_json::from_str(&row.conditions)
        .map_err(|e| AppError::Other(anyhow::anyhow!("corrupt conditions json: {e}")))?;
    let actions: Vec<Action> = serde_json::from_str(&row.actions)
        .map_err(|e| AppError::Other(anyhow::anyhow!("corrupt actions json: {e}")))?;
    Ok(Rule {
        id: row.id,
        name: row.name,
        enabled: row.enabled,
        position: row.position,
        conditions,
        actions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub async fn list_rules(state: &AppState) -> AppResult<Vec<Rule>> {
    repository::list_all(&state.db)
        .await?
        .into_iter()
        .map(parse_row)
        .collect()
}

pub async fn get_rule(state: &AppState, id: Uuid) -> AppResult<Rule> {
    let row = repository::get(&state.db, id).await?.ok_or(AppError::NotFound)?;
    parse_row(row)
}

/// 検証 → JSON 直列化 → 保存。conditions/actions は handler が受けた型をそのまま渡す。
pub async fn create_rule(
    state: &AppState,
    name: String,
    enabled: bool,
    position: i32,
    conditions: Conditions,
    actions: Vec<Action>,
) -> AppResult<Rule> {
    let name = RuleName::parse(name).map_err(AppError::Validation)?;
    domain::validate_conditions(&conditions).map_err(AppError::Validation)?;
    domain::validate_actions(&actions).map_err(AppError::Validation)?;
    let cj = serde_json::to_string(&conditions).map_err(|e| AppError::Other(e.into()))?;
    let aj = serde_json::to_string(&actions).map_err(|e| AppError::Other(e.into()))?;
    let row = repository::insert(&state.db, name.as_str(), enabled, position, &cj, &aj).await?;
    parse_row(row)
}

pub async fn update_rule(
    state: &AppState,
    id: Uuid,
    name: String,
    enabled: bool,
    position: i32,
    conditions: Conditions,
    actions: Vec<Action>,
) -> AppResult<Rule> {
    let name = RuleName::parse(name).map_err(AppError::Validation)?;
    domain::validate_conditions(&conditions).map_err(AppError::Validation)?;
    domain::validate_actions(&actions).map_err(AppError::Validation)?;
    let cj = serde_json::to_string(&conditions).map_err(|e| AppError::Other(e.into()))?;
    let aj = serde_json::to_string(&actions).map_err(|e| AppError::Other(e.into()))?;
    let row = repository::update(&state.db, id, name.as_str(), enabled, position, &cj, &aj)
        .await?
        .ok_or(AppError::NotFound)?;
    parse_row(row)
}

pub async fn delete_rule(state: &AppState, id: Uuid) -> AppResult<()> {
    if repository::delete(&state.db, id).await? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// クロール経路から呼ばれる本体: 1フィードの未適用記事へ有効ルールを当てる。
pub async fn apply_for_feed(state: &AppState, feed_id: Uuid) -> AppResult<()> {
    let rules = load_enabled(state).await?;
    let pending = repository::fetch_pending(&state.db, feed_id, 500).await?;
    apply_rules_to(state, &rules, &pending).await?;
    let ids: Vec<Uuid> = pending.iter().map(|a| a.id).collect();
    repository::mark_applied(&state.db, &ids).await?; // ルール0件でも適用済みにし再走査を避ける
    Ok(())
}

/// 全記事へ再適用（ルール編集後の手動バックフィル）。rules_applied_at は無視する。
pub async fn apply_all(state: &AppState) -> AppResult<usize> {
    let rules = load_enabled(state).await?;
    let articles = repository::fetch_all_articles(&state.db, 5000).await?;
    apply_rules_to(state, &rules, &articles).await?;
    let ids: Vec<Uuid> = articles.iter().map(|a| a.id).collect();
    repository::mark_applied(&state.db, &ids).await?;
    Ok(articles.len())
}

/// ドライラン: ルール1件を直近記事に当て、一致した記事 id を返す（DB 変更なし）。
pub async fn test_rule(state: &AppState, id: Uuid, sample: i64) -> AppResult<Vec<Uuid>> {
    let rule = get_rule(state, id).await?;
    let articles = repository::fetch_all_articles(&state.db, sample).await?;
    let now = chrono::Utc::now();
    let mut matched = Vec::new();
    for a in &articles {
        let tags = repository::tags_for(&state.db, a.id).await.unwrap_or_default();
        let ctx = build_ctx(a, &tags, now);
        if domain::rule_matches(&rule.conditions, &ctx) {
            matched.push(a.id);
        }
    }
    Ok(matched)
}

async fn load_enabled(state: &AppState) -> AppResult<Vec<Rule>> {
    repository::list_enabled(&state.db)
        .await?
        .into_iter()
        .filter_map(|r| parse_row(r).ok()) // 壊れた行はスキップ（クロールを止めない）
        .collect::<Vec<_>>()
        .pipe(Ok) // ※ pipe は説明用。実装は普通に let v = ...; Ok(v)
}

fn build_ctx<'a>(
    a: &'a PendingArticle,
    tags: &'a [String],
    now: chrono::DateTime<chrono::Utc>,
) -> ArticleCtx<'a> {
    ArticleCtx {
        title: &a.title,
        content: &a.content,
        url: &a.url,
        author: a.author.as_deref(),
        feed_id: a.feed_id,
        published_at: a.published_at,
        tags,
        now,
    }
}

async fn apply_rules_to(
    state: &AppState,
    rules: &[Rule],
    articles: &[PendingArticle],
) -> AppResult<()> {
    if rules.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now();
    for a in articles {
        let tags = repository::tags_for(&state.db, a.id).await.unwrap_or_default();
        let ctx = build_ctx(a, &tags, now);
        for rule in rules {
            if domain::rule_matches(&rule.conditions, &ctx) {
                for action in &rule.actions {
                    // 1アクションの失敗でクロール全体を止めない（best-effort・warn して継続）。
                    if let Err(e) = apply_action(state, a.id, action).await {
                        tracing::warn!(
                            error = %e, rule = %rule.name, article = %a.id,
                            "rule action failed; continuing"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

/// 1アクションの実行。依存機能が無い場合は Err になるが、呼び出し側が warn して継続する。
async fn apply_action(state: &AppState, article_id: Uuid, action: &Action) -> AppResult<()> {
    match action {
        Action::MarkRead => {
            // 既存の articles スライス公開関数を再利用。
            articles::repository::set_read(
                &state.db,
                articles::domain::ArticleId(article_id),
                true,
            )
            .await
        }
        Action::Score { delta } => repository::bump_score(&state.db, article_id, *delta).await,
        Action::Save => {
            // Instapaper（機能05/06）。資格情報未設定なら NotEnabled → 呼び出し側が warn。
            instapaper::service::add_to_read_later(state, article_id).await
        }
        Action::Tag { name } => {
            // 機能24（タグ）。テーブルが無ければ tag_article は Err → warn して継続。
            tag_article(state, article_id, name).await
        }
        Action::Star => {
            // 機能32（スター）。未マージなら Err → warn して継続。
            star_article(state, article_id).await
        }
    }
}

/// タグ付与（機能24 のテーブルへ書く・存在しなければ Err）。
async fn tag_article(state: &AppState, article_id: Uuid, name: &str) -> AppResult<()> {
    // tags へ upsert（lower(name) ユニーク）→ id 取得 → article_tags へ idempotent insert。
    let tag_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO tags (id, name, source) VALUES (gen_random_uuid(), $1, 'user')
           ON CONFLICT (lower(name)) DO UPDATE SET name = tags.name
           RETURNING id"#,
    )
    .bind(name.trim())
    .fetch_one(&state.db)
    .await?;
    sqlx::query(
        r#"INSERT INTO article_tags (article_id, tag_id, source)
           VALUES ($1, $2, 'user') ON CONFLICT (article_id, tag_id) DO NOTHING"#,
    )
    .bind(article_id)
    .bind(tag_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// スター付与（機能32 のテーブルへ書く・存在しなければ Err）。
/// 32 のスキーマ未確定のため、実装時に 32 の最終テーブル名/列に合わせること（§11）。
async fn star_article(state: &AppState, article_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO article_stars (article_id) VALUES ($1)
           ON CONFLICT (article_id) DO NOTHING"#,
    )
    .bind(article_id)
    .execute(&state.db)
    .await?;
    Ok(())
}
```

> **注**: `load_enabled` の `.pipe(Ok)` は読みやすさのための擬似コード。実装は `let v: Vec<Rule> = ...collect(); Ok(v)` とする（`tap`/`pipe` は依存に無い）。
>
> HTTP/DB アクセスを `service.rs` 内に閉じるのは、本スライスに trait/dyn の抽象境界を作らない方針（抽象境界は `shared/llm` のみ）に沿うため。`save` だけは既存 `instapaper::service::add_to_read_later` を**再利用**する（05/06 が所有する経路を二重実装しない）。`mark_read` も `articles::repository::set_read` を再利用する。

### 5.4 アクション/条件のソフト依存と劣化動作

| 種別 | 依存機能 | 未マージ/未設定時の動作 |
|---|---|---|
| 条件 `tag` | 24（タグ） | `repository::tags_for` が `to_regclass` でテーブル不在を検知し**空 Vec** を返す → tag 条件は常に不一致（クラッシュしない） |
| アクション `tag` | 24（タグ） | `tag_article` の SQL が relation 不在で Err → 呼び出し側が **warn して継続** |
| アクション `star` | 32（スター） | `star_article` の SQL が Err → **warn して継続**。32 マージ後にテーブル名/列を確認 |
| アクション `save` | 05/06（Instapaper） | 資格情報未設定で `NotEnabled` → **warn して継続**（クロールは止まらない） |
| 条件 `author` | （本機能で追加する `articles.author`） | クロールで保存。過去記事は NULL → author 条件不一致（`apply_all` 後も NULL のまま。再クロールで埋まる） |

**コア（依存なしで完全動作）**: 条件 `keyword`/`feed`/`date`、アクション `mark_read`/`score`。v1 はこの範囲だけでも単独で価値を出せる。

### 5.5 `handler.rs`（axum ハンドラ）

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{Action, Conditions};
use super::service::{self, Rule};
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RuleBody {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub position: i32,
    pub conditions: Conditions,
    pub actions: Vec<Action>,
}
fn default_true() -> bool { true }

pub async fn list(State(s): State<AppState>) -> AppResult<Json<Vec<Rule>>> {
    Ok(Json(service::list_rules(&s).await?))
}

pub async fn get_one(State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Json<Rule>> {
    Ok(Json(service::get_rule(&s, id).await?))
}

pub async fn create(
    State(s): State<AppState>,
    Json(b): Json<RuleBody>,
) -> AppResult<(StatusCode, Json<Rule>)> {
    let rule = service::create_rule(&s, b.name, b.enabled, b.position, b.conditions, b.actions).await?;
    Ok((StatusCode::CREATED, Json(rule)))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(b): Json<RuleBody>,
) -> AppResult<Json<Rule>> {
    Ok(Json(
        service::update_rule(&s, id, b.name, b.enabled, b.position, b.conditions, b.actions).await?,
    ))
}

pub async fn delete(State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    service::delete_rule(&s, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Serialize)]
pub struct TestResult {
    pub matched_count: usize,
    pub matched_ids: Vec<Uuid>,
}

pub async fn test(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<TestResult>> {
    let ids = service::test_rule(&s, id, 200).await?;
    Ok(Json(TestResult { matched_count: ids.len(), matched_ids: ids }))
}

#[derive(Debug, serde::Serialize)]
pub struct ApplyResult {
    pub processed: usize,
}

pub async fn apply(State(s): State<AppState>) -> AppResult<Json<ApplyResult>> {
    let n = service::apply_all(&s).await?;
    Ok(Json(ApplyResult { processed: n }))
}
```

### 5.6 `mod.rs`（routes）

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
        .route("/api/rules", get(handler::list).post(handler::create))
        .route("/api/rules/apply", post(handler::apply))
        .route(
            "/api/rules/{id}",
            get(handler::get_one).put(handler::update).delete(handler::delete),
        )
        .route("/api/rules/{id}/test", post(handler::test))
}
```

> ルート衝突回避: 静的 `/api/rules/apply` は動的 `/api/rules/{id}` より優先される（axum 0.8 / matchit は静的セグメント優先）。`feed_overview` が `/api/feeds/overview` と `/api/feeds/{id}` を衝突なく同居させている前例と同型。

### 5.7 既存スライスへの最小追記（同一アグリゲートへの additive 変更）

**(A) `features/mod.rs`（2行）**:
```rust
pub mod automation_rules; // 既存 pub mod 群に追加
// router() の .merge チェーンに追加:
        .merge(automation_rules::routes())
```

**(B) クロール実行フック — `feeds/src/service.rs::fetch_and_store` 末尾（`touch_fetched` の後）に1行**:
```rust
    repository::touch_fetched(&state.db, feed.id, feed_title.as_deref()).await?;
    // 取り込み直後にルールを適用（失敗してもクロールは止めない）。
    if let Err(e) = crate::features::automation_rules::service::apply_for_feed(state, feed.id.0).await {
        tracing::error!(error = %e, feed = %feed.url, "rule application failed");
    }
    Ok(())
```
これは既に `fetch_and_store` が `articles::repository::upsert` を呼んでいる**クロススライス呼び出しの前例**と同型（feeds → 他スライスの呼び出しは既存）。

**(C) 著者保存 — `articles::repository::upsert` に `author: Option<&str>` を追記**（additive）し、`fetch_and_store` のループで `entry.authors.first().map(|p| p.name.as_str())` を渡す:
```rust
// articles/repository.rs::upsert の INSERT を author 込みに（追記）
//   INSERT INTO articles (id, feed_id, url, title, content, published_at, author)
//   VALUES ($1,$2,$3,$4,$5,$6,$7)
//   ON CONFLICT (url) DO UPDATE SET title=EXCLUDED.title, content=EXCLUDED.content,
//                                   author=EXCLUDED.author
// 呼び出し側 fetch_and_store:
//   let author = entry.authors.first().map(|p| p.name.clone());
//   articles::repository::upsert(&state.db, FeedId(feed.id.0), &url, &title, &content,
//                                published, author.as_deref()).await?;
```
> これは articles アグリゲートへの **additive 変更**（列1つ + 引数1つ）であり、Vertical Slice の「同一アグリゲートへの書き込みは正当」に該当（README 前提）。`author` を使わない呼び出し元が他に無い（`upsert` の呼び出しは `fetch_and_store` のみ）ため影響は局所。`Article` 構造体（`SELECT *`）には未知カラムを足しても sqlx FromRow が無視するので、著者を UI に出さないなら構造体変更は不要（本機能は著者を自前射影 `PendingArticle` で読む）。

### 5.8 AppError の使い分け（`error.rs` は不編集）

| 状況 | バリアント | HTTP |
|---|---|---|
| ルール名が空 / 条件0件 / アクション0件 / 値空 / days 負 | `Validation` | 400 |
| `GET/PUT/DELETE /api/rules/{id}` で対象が無い | `NotFound` | 404 |
| DB エラー（`?` で `From<sqlx::Error>`） | `Database` | 500 |
| 保存済み JSON が壊れている（通常起きない） | `Other`（anyhow） | 500 |
| `save` アクションで Instapaper 未設定 | （クロール時は warn で握る）`NotEnabled` | — |

> クロール実行中のアクション失敗は **HTTP には出さず** `tracing::warn!` で記録し継続（バッチ処理なので個別失敗で全体を落とさない）。新バリアントは追加しない。

---

## 6. フロントエンド設計

### 6.1 `lib/api.ts`（型 + 7 メソッド追加）

backend JSON をミラーする型（serde の `#[serde(tag=...)]` に合わせた判別共用体）:

```ts
export type Combinator = "all" | "any";
export type KeywordTarget = "title" | "content" | "any";
export type DateOp = "older_than_days" | "newer_than_days";

export type Condition =
  | { field: "keyword"; target: KeywordTarget; value: string; case_sensitive?: boolean }
  | { field: "author"; value: string }
  | { field: "feed"; feed_ids: string[] }
  | { field: "tag"; tag: string }
  | { field: "date"; op: DateOp; days: number };

export type Action =
  | { kind: "mark_read" }
  | { kind: "star" }
  | { kind: "tag"; name: string }
  | { kind: "save" }
  | { kind: "score"; delta: number };

export interface Conditions {
  combinator: Combinator;
  items: Condition[];
}

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  position: number;
  conditions: Conditions;
  actions: Action[];
  created_at: string;
  updated_at: string;
}

export interface RuleInput {
  name: string;
  enabled?: boolean;
  position?: number;
  conditions: Conditions;
  actions: Action[];
}

export interface RuleTestResult {
  matched_count: number;
  matched_ids: string[];
}
```

`api` オブジェクトに追加（既存 `http<T>()` を再利用・命名は既存 `動詞+リソース`）:

```ts
  listRules: () => http<Rule[]>("/api/rules"),
  getRule: (id: string) => http<Rule>(`/api/rules/${id}`),
  createRule: (input: RuleInput) =>
    http<Rule>("/api/rules", { method: "POST", body: JSON.stringify(input) }),
  updateRule: (id: string, input: RuleInput) =>
    http<Rule>(`/api/rules/${id}`, { method: "PUT", body: JSON.stringify(input) }),
  deleteRule: (id: string) => http<void>(`/api/rules/${id}`, { method: "DELETE" }),
  testRule: (id: string) => http<RuleTestResult>(`/api/rules/${id}/test`, { method: "POST" }),
  applyRules: () => http<{ processed: number }>("/api/rules/apply", { method: "POST" }),
```

### 6.2 ルート `routes/Rules.tsx`（一覧 + 新規/編集）

- `const [rules, { refetch }] = createResource(() => api.listRules());`
- 一覧: 各ルールを `Card` で表示（名前・有効スイッチ・条件/アクションの要約バッジ・編集/削除/テストボタン）。
- 有効トグルは `switch.tsx`（Ark UI）→ `api.updateRule(id, { ...rule, enabled })` → `refetch()`。
- 「新規ルール」ボタンで `RuleBuilder` を `dialog.tsx`（Ark UI）に開く。
- 「すべて再適用」ボタン → `api.applyRules()` → 件数をトースト/テキスト表示。
- 「テスト」→ `api.testRule(id)` → `matched_count` を表示（ドライラン）。
- 状態は**ローカル**（`createResource` + `createSignal`）。グローバルストア不要。`store.tsx` は変更しない。

### 6.3 コンポーネント `components/rules/RuleBuilder.tsx`（新規）

ルールビルダー本体。**条件行リスト**と**アクション行リスト**を動的に追加/削除する自前フォーム。

骨子:
- props: `initial?: Rule`（編集時）, `onSaved: () => void`。
- `createSignal` で `name`, `enabled`, `combinator: "all"|"any"`, `conditions: Condition[]`, `actions: Action[]`。
- **結合子**: `all`/`any` を Ark UI Select か 2択トグル（`button.tsx`）で選ぶ。
- **条件行**: 「フィールド種別」を Ark UI Select（keyword/author/feed/tag/date）で選び、種別に応じて入力を出し分け:
  - keyword: target(Select: title/content/any) + value(Input) + case_sensitive(switch)
  - author: value(Input)
  - feed: フィード複数選択（`api.listFeeds()` をチェックボックスリストで。`feed_ids: string[]`）
  - tag: tag(Input)
  - date: op(Select: older_than_days/newer_than_days) + days(Input number)
- **アクション行**: 「種別」Select（mark_read/star/tag/save/score）+ 種別依存入力（tag→name, score→delta）。
- 「条件を追加」「アクションを追加」ボタンで行追加、各行に削除ボタン。
- 保存: `RuleInput` を組み立て `api.createRule` / `api.updateRule` → `onSaved()`。バリデーションエラー（400）は `catch` でフォーム上部に表示。
- 装飾は oklch トークン（`bg-background`/`border-input`/`text-muted-foreground`）と既存 `Input`/`Button`/`Card`/`switch`。
- 依存機能が未マージのアクション（tag/star/save）には「機能24/32/05 が必要」の注記を `text-xs text-muted-foreground` で添える（任意）。

> Ark UI の Select / Switch の part 名・props はバージョンで変わりうる。実装時に [ark-ui.com](https://ark-ui.com)（Solid）で最新構造を確認すること（CLAUDE.md UI 方針）。

### 6.4 ルーティング `index.tsx`

```tsx
import Rules from "./routes/Rules";
// <Router> 内に追加:
<Route path="/rules" component={Rules} />
```
導線（Sidebar/設定からのリンク）は二ペイン（機能10）/設定（機能05 の `/settings`）に合わせて足す。最低限 `/rules` を直接開けば使える状態にする。

### 6.5 Ark UI / 状態管理

- 必要部品: `Input`/`Button`/`Card`（自前・既存）、`switch`/`dialog`/Select（Ark UI・switch/dialog は既存、Select は新規ラップが要れば `components/ui/select.tsx` を Ark UI で薄くラップ）。
- 新しいグローバル状態は不要（ルール編集はローカルに閉じる）。`store.tsx` は触らない。

---

## 7. API 契約

すべて `/api` プレフィックス。

### 7.1 `GET /api/rules` — ルール一覧
レスポンス `200`: `Rule[]`（`position ASC, created_at ASC`）。
```json
[
  {
    "id": "9f1c0e8a-1111-2222-3333-444455556666",
    "name": "Rust 記事を自動既読",
    "enabled": true,
    "position": 0,
    "conditions": {
      "combinator": "any",
      "items": [
        { "field": "keyword", "target": "title", "value": "rust", "case_sensitive": false },
        { "field": "tag", "tag": "rust" }
      ]
    },
    "actions": [{ "kind": "mark_read" }, { "kind": "score", "delta": 5 }],
    "created_at": "2026-06-30T10:00:00Z",
    "updated_at": "2026-06-30T10:00:00Z"
  }
]
```

### 7.2 `POST /api/rules` — 作成
リクエスト（`RuleInput`）:
```json
{
  "name": "古い広告記事を既読",
  "enabled": true,
  "position": 1,
  "conditions": {
    "combinator": "all",
    "items": [
      { "field": "keyword", "target": "any", "value": "PR" },
      { "field": "date", "op": "older_than_days", "days": 30 }
    ]
  },
  "actions": [{ "kind": "mark_read" }]
}
```
レスポンス `201`: 作成された `Rule`。
エラー:
- 400 `{ "error": "invalid input: at least one condition is required" }`
- 400 `{ "error": "invalid input: score delta must not be zero" }`
- 400 `{ "error": "invalid input: rule name must not be empty" }`

### 7.3 `GET /api/rules/{id}` — 取得 / `PUT /api/rules/{id}` — 更新 / `DELETE /api/rules/{id}` — 削除
- GET → `200 Rule` / `404`
- PUT（body は `RuleInput`）→ `200 Rule`（更新後） / `404` / `400`
- DELETE → `204 No Content` / `404`

### 7.4 `POST /api/rules/{id}/test` — ドライラン（DB 変更なし）
レスポンス `200`:
```json
{ "matched_count": 12, "matched_ids": ["...", "..."] }
```
直近 200 記事に対する一致結果のみ返す（アクションは実行しない）。`404` ならルール不在。

### 7.5 `POST /api/rules/apply` — 全件再適用（バックフィル）
レスポンス `200`:
```json
{ "processed": 1500 }
```
直近 5000 記事へ有効ルールを当て、アクションを実行する。`processed` は走査記事数。

---

## 8. 依存関係

- **本機能が依存する機能（ハード依存）**: なし。コア（keyword/feed/date 条件 × mark_read/score アクション）は既存 `articles`/`feeds` テーブルのみで完結し、単独でマージ・動作できる。
- **ソフト依存（無くても動くが、その種別だけ no-op/skip になる）**:
  - 機能24（タグ）: `tag` 条件・`tag` アクション。未マージ時は §5.4 のとおり劣化（条件は不一致・アクションは warn skip）。
  - 機能32（スター）: `star` アクション。32 のテーブル名/列確定後に `star_article` を合わせる。
  - 機能05/06（Instapaper / 後で読む）: `save` アクション。`instapaper::service::add_to_read_later` を再利用。未設定なら warn skip。
- **本機能がブロックする/土台になるもの**:
  - 機能19（ミュート）・機能24（自動タグ）の**将来統合**。両者は本エンジンの部分集合（19 = keyword 条件 × hide/mark_read、24 = 条件 × tag アクション）。統合時は19/24 の専用テーブル/UI を本エンジンの設定へ移行する（別チケット。§11）。本書では**共存**させ既存挙動を壊さない。
- 既存スライスへの接触点は **3 箇所のみ**: `features/mod.rs`（2行）、`feeds/service.rs`（実行フック1行 + 著者抽出）、`articles/repository.rs::upsert`（`author` 引数追記）。

---

## 9. テスト計画（TDD）

**Red → 理解 → Green。書いたら必ず実行する。** テスト配置は実プロジェクト慣習（純粋ロジック = `#[cfg(test)]`、結合 = `scripts/test/*.sh`、DB 往復 = `repository.rs` 内 `#[ignore]`）に従う。`backend/tests/` は存在せず本 crate は binary crate（lib なし）なので内部関数は同一モジュールの `#[cfg(test)]` から呼ぶ（03/05 設計書と同じ判断）。

### 9.1 単体テスト（`#[cfg(test)] mod tests` in `domain.rs`、DB/外部 API 不要）

| テスト | 意図 |
|---|---|
| `keyword_title_case_insensitive_matches` | target=title・大小無視で部分一致 |
| `keyword_case_sensitive_respects_case` | case_sensitive=true で大小区別 |
| `keyword_any_matches_title_or_content` | target=any は title か content のいずれか一致 |
| `keyword_no_match_returns_false` | 含まれない語は不一致 |
| `author_matches_substring_insensitive` | 著者部分一致（大小無視） |
| `author_none_is_false` | 著者 None は author 条件不一致 |
| `feed_matches_when_id_in_set` | feed_ids に含まれれば一致 |
| `feed_no_match_when_absent` | 集合外は不一致 |
| `tag_matches_lowercased` | タグは小文字化して一致（"Rust"=="rust"） |
| `tag_empty_ctx_is_false` | ctx.tags 空（機能24 未マージ相当）で不一致 |
| `date_older_than_days_true_for_old` | published が days より古ければ older_than 一致 |
| `date_newer_than_days_true_for_recent` | 直近 days 以内なら newer_than 一致 |
| `date_none_published_is_false` | published_at None は date 条件不一致 |
| `rule_matches_all_requires_every_condition` | combinator=all は全条件一致で true、1つ外れると false |
| `rule_matches_any_requires_one_condition` | combinator=any は1つ一致で true、全外れで false |
| `validate_conditions_rejects_empty_items` | 条件0件を Err |
| `validate_conditions_rejects_empty_value` | keyword/author の空 value を Err |
| `validate_conditions_rejects_empty_feed_set` | feed_ids 空を Err |
| `validate_actions_rejects_empty` | アクション0件を Err |
| `validate_actions_rejects_zero_score` | score delta=0 を Err |
| `serde_roundtrip_condition_and_action` | Condition/Action の JSON タグ（field/kind）で round-trip（`from_str(to_string(x))==x`） |

> `serde_roundtrip_*` は TEXT(JSON) 保存方式が壊れていないことを保証する重要テスト（DB 不要で JSON 契約を固定）。

### 9.2 リポジトリ往復テスト（`#[cfg(test)] mod tests` in `repository.rs`、実 DB / `#[ignore]`）

`DATABASE_URL`（`just dev-db`）に接続し `#[tokio::test]` + `#[ignore]`。マイグレーション適用済み前提（`cargo test -- --ignored`）。

| テスト | 意図 |
|---|---|
| `rule_crud_roundtrip` | insert → get（一致）→ update（変更反映）→ delete（rows=1）→ get（None） |
| `bump_score_is_idempotent_add` | bump_score(+5) → bump_score(+3) で score=8（ON CONFLICT 加算） |
| `fetch_pending_excludes_applied` | rules_applied_at を立てた記事が fetch_pending に出ない |
| `mark_applied_sets_timestamp` | mark_applied 後に fetch_pending から消える |
| `tags_for_returns_empty_when_table_absent` | （24 未マージ環境）`to_regclass` 経路で空 Vec を返しエラーにしない |

### 9.3 HTTP スモークテスト（`scripts/test/api-rules.sh`、稼働スタックへ curl）

`scripts/test/api-stats.sh` を雛形に nginx `:8081` 経由。

| 手順 / アサーション | 意図 |
|---|---|
| `POST /api/rules`（keyword=any "qa-rule-xyz" × mark_read）→ 201 かつ `id` 返る | 作成 + スライス合成 + JSON 直列化 |
| `GET /api/rules` → 200 配列に作成した name を含む | 一覧 |
| `POST /api/rules`（conditions.items=[]）→ 400 | バリデーション（条件0件） |
| `POST /api/rules`（actions=[{kind:score,delta:0}]）→ 400 | バリデーション（score 0） |
| `POST /api/rules/{id}/test` → 200 かつ `matched_count` が数値 | ドライラン配線 |
| `POST /api/rules/apply` → 200 かつ `processed` が数値 | 全件再適用配線 |
| `GET /api/rules/{bad-uuid-but-valid}` → 404 | NotFound 経路 |
| `DELETE /api/rules/{id}` → 204、再 `GET` → 404 | 削除 + 冪等確認 |

> 作成したルールは末尾で必ず `DELETE` してクリーンにする（決定論）。`apply` は実アクションを起こすため、テストルールは「実在しないキーワード」で一致0件になるよう組み、副作用を避ける。

### 9.4 フロント（手動 + 型）
- `tsc`（`just lint`）で `api.ts` / `Rules.tsx` / `RuleBuilder.tsx` の型整合を確認。
- 手動: `/rules` でルール作成（keyword×mark_read）→ テストで一致件数表示 → 該当フィードを再取得（または `apply`）→ 記事が既読になることを確認 → 削除。

---

## 10. 実装手順（順序付きチェックリスト）

1. **マイグレーション番号採番**: `ls backend/migrations/` で最新確認（現状 `0005_search.sql` → `0006_automation_rules.sql`）。既に高い番号が永続 DB に適用済みなら、より小さい番号を新規追加しない（§4.1）。
2. **マイグレーション作成**: §4.2 の SQL で `0006_automation_rules.sql` を新規作成（既存は触らない）。
3. **ドメイン（Red 先行）**: `backend/src/features/automation_rules/domain.rs` に §5.1 の型と純粋関数 + §9.1 の `#[cfg(test)] mod tests` を書く。まず Red→実装で Green。`cd backend && cargo test automation_rules`。
4. **repository**: `repository.rs` を §5.2 で作成（`query`/`query_as`/`query_scalar` のみ）。§9.2 の `#[cfg(test)]`（`#[ignore]`）も書く。
5. **service**: `service.rs` を §5.3 で作成（`load_enabled` は擬似 `.pipe` を `let v=...; Ok(v)` に直す）。
6. **handler + mod**: §5.5/§5.6 で作成。
7. **既存への最小追記**: §5.7 の (A) `features/mod.rs` 2行、(B) `feeds/service.rs` 実行フック1行、(C) `articles/repository.rs::upsert` の `author` 引数 + `fetch_and_store` の著者抽出。
8. **ビルド & lint**: `cargo fmt` → `just lint`（clippy `-D warnings` / tsc）。`serde_json` は既存依存・sqlx の `json` feature は不要（TEXT 保存）。
9. **DB 起動 & マイグレーション**: `just dev-db` →（バックエンド起動で自動 migrate、または `just migrate`）。
10. **リポジトリ往復テスト**: `DATABASE_URL=... cargo test -- --ignored` で §9.2 を Green に。
11. **HTTP スモーク**: `scripts/test/api-rules.sh` を §9.3 で作成・`chmod +x`・実行。
12. **フロント**: `lib/api.ts`（型 + 7 メソッド）、`routes/Rules.tsx`、`components/rules/RuleBuilder.tsx`、`index.tsx` に `/rules`。必要なら Ark UI `select` を `components/ui/select.tsx` に薄くラップ。`just lint` の tsc を通す。
13. **手動 E2E**: `/rules` で作成→テスト→クロール（フィード再取得）/`apply`→既読化を確認→削除。
14. **コミット**: マイグレーション・スライス・既存追記・スクリプト・フロントをまとめて。`.env`/秘密はコミットしない。コミットメッセージ末尾に `Co-Authored-By` 行。

---

## 11. リスク・未決事項・代替案

- **JSON を TEXT で保持（sqlx に `json` feature 無し）**: 条件/アクションは TEXT 列に JSON 文字列で保存し `serde_json` で相互変換（Cargo.toml 変更回避）。欠点は DB 側で JSON 検索/部分更新ができないこと。将来ルール条件で DB 検索が要るなら、`Cargo.toml` の sqlx に `json` を足して `jsonb` へ移行（新マイグレーション `ALTER ... TYPE jsonb USING conditions::jsonb`）。v1 はアプリ層パースで十分。
- **著者保存が articles アグリゲートに触れる**: `articles.author` 列追加 + `upsert` 引数追加 + `fetch_and_store` の著者抽出は additive だが既存スライスへの変更。author 条件を諦めれば回避できるが、要求にある以上 v1 で入れる。過去記事は `author=NULL`（再クロールで埋まる）。
- **ソフト依存アクション（tag/star/save）の劣化**: 24/32/05 未マージ/未設定時は warn して skip（§5.4）。`star_article` の SQL は機能32 の確定テーブル（本書は `article_stars(article_id)` を仮定）に合わせて実装時に修正すること（32 はドラフトでスキーマ未確定）。
- **`rules_applied_at` の再処理セマンティクス**: クロール時は未処理記事のみ処理（性能のため）。ルールを後から作成/編集しても**既存記事には自動で再適用されない** → 明示的 `POST /api/rules/apply`（全件・最大5000件）で当て直す。UI に「再適用」ボタンを置く（§6.2）。バックフィル件数上限（5000）は家庭内規模の安全弁。超大量時はページング/バッチ化を将来検討。
- **アクションの競合/順序**: 複数ルールが同記事に当たると、有効ルールを `position ASC` で順次実行（後勝ち。score は加算）。相反するアクション（既読化↔別ルールでスター）は両方実行される。v1 は「全マッチルールの全アクションを実行」する単純規則。`stop`（以降のルールを止める）アクションは将来拡張余地。
- **クロール内同期実行の遅延**: `apply_for_feed` は `fetch_and_store` 内で同期実行。フィードあたり未処理500件上限 + ルール件数×記事件数の評価（純粋関数なので軽い）+ アクションの DB 往復。家庭内・単一ユーザ規模で問題なし。重くなれば apalis ジョブ化（ロードマップの apalis 移行に乗せる）。
- **機能19/24 との二重処理（共存期）**: 統合前は、例えば「あるキーワードを mute(19) で hide しつつ、ルールエンジンでも mark_read」のような重複設定が可能。挙動は両方が独立に働く（破壊的ではない）。統合チケットで19/24 の設定を本エンジンの条件/アクションへ移送し、専用テーブル/UI を廃止する。
- **`gen_random_uuid()` 依存**: 既定値に pgcrypto 由来関数を使う（機能19/24 前例）。万一 extension 未導入環境なら `CREATE EXTENSION IF NOT EXISTS pgcrypto;` を migration 先頭に足すか、アプリ側 `Uuid::new_v4()` で id を bind する方式へ切替（`insert` の SQL を `id` 明示に変更）。
- **AI ルール（将来）**: v1 は決定論のみ。将来「LLM で関連度スコア」等を足す場合は `shared/llm::LlmClient` を再利用し、結果を `article_scores` 等にキャッシュ、`ANTHROPIC_API_KEY` 未設定で `AppError::NotEnabled`（要約/翻訳と同型）。本書のアクション enum に `AiScore` 等を追記する形で拡張できる。
- **テスト/apply の `processed` と実副作用**: `POST /api/rules/apply` は実アクションを起こす。スモークテストでは一致0件になるルールのみで検証し、本番データへの意図しない一括既読等を避ける。UI でも apply 前に件数（test）を見せてから実行させる導線が望ましい。
