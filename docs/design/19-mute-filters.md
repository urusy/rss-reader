# 19 ミュート（NGワード）フィルタ

> 読み手向けメモ: このドキュメントは「リポジトリは手元にあるが、この会話の文脈を知らない別セッションの実装者」が、これ1枚だけ読めば着手・完了できる粒度で書いている。裏取りした実ファイル（このセッションで実際に開いて確認した）: `backend/src/features/articles/{domain,repository,service,handler,mod}.rs`, `backend/src/features/feeds/{service,repository,domain,mod}.rs`, `backend/src/features/folders/{handler,mod}.rs`, `backend/src/features/search/mod.rs`, `backend/src/features/mod.rs`, `backend/src/shared/{state,error}.rs`, `backend/migrations/0001_init.sql`〜`0005_search.sql`, `frontend/src/lib/api.ts`, `frontend/src/lib/store.tsx`, `frontend/src/routes/Settings.tsx`。
>
> **マイグレーション番号の注意（最優先）**: 本書は新規マイグレーションを **`0006_mute_rules.sql`** として暫定採番している。リポジトリ確認時点の最新は **`0005_search.sql`**。ただし apalis 移行や他の並行チケットが先に 0006 を取得している可能性がある。**着手前に必ず `ls backend/migrations/` で最新番号を確認し、空き番号へ採番し直すこと**（マイグレーションは追記のみ・既存ファイルは不編集が鉄則）。

## 1. 概要

特定の語を含む記事を一覧から自動的に **非表示（hide）** または **自動既読（mark_read）** にする「ミュート（NGワード）」機能。広告・スポンサー記事・関心のないトピック・特定ドメインの記事を、ユーザーが定義したルールで継続的に弾く。

- ルールは `mute_rules` テーブルに永続化し、`{ field, pattern, match_type, action, enabled }` を持つ。
- `field` は **タイトル / 本文 / URL（ドメイン相当）** のいずれか。`pattern` は大文字小文字を無視した **部分一致（contains）**。
- `action` は `hide`（一覧から除外）か `mark_read`（既読化して未読数から外す）。
- 適用は **読み取り時のフィルタ + 適用バッチ（apply）の併用**: `hide` は `articles.muted_at` を立てて一覧クエリで除外、`mark_read` は既存 `articles.is_read` を立てる。ルール追加・編集・削除のたびに `POST /api/mute-rules/apply` で全記事を再評価でき、クロール後にもスケジューラから自動適用される。
- 実装は新スライス `backend/src/features/mute_rules/` 1枚に閉じる（`domain/repository/service/handler/mod`）。`features/mod.rs` に `.merge()` 1行を足す。

これは将来の汎用 **ルールエンジン（#28）** の部分集合であり、フィールド・パターン・アクションという最小語彙を先行実装する。ルールエンジンが入ったら、`mute_rules` を「アクション = hide/mark_read に限定したルール」として吸収・移行できるよう、列構成を前方互換に保つ。

**AI は使わない**: ミュートは決定論的なパターン一致であり LLM を呼ばない（`shared/llm` 非依存）。NGワードの AI 提案・意味ベースのフィルタ（例: 「政治系を弱める」）は #28 の領域で、その段で `shared/llm` 再利用 + DB キャッシュ + `ANTHROPIC_API_KEY` 未設定時 `AppError::NotEnabled` を踏襲する。本書 §5.8 にその拡張の置き場所だけ記す（実装は本チケット範囲外）。

## 2. スコープ / 非スコープ

**含む（このチケットでやる）**

- 新スライス `backend/src/features/mute_rules/`（`domain.rs` / `repository.rs` / `service.rs` / `handler.rs` / `mod.rs`）。
- 新規マイグレーション `0006_mute_rules.sql`（暫定番号）: `mute_rules` テーブル新設 ＋ `articles` への **`muted_at TIMESTAMPTZ` 追記カラム** ＋ 部分インデックス。
- ルール CRUD: `GET/POST /api/mute-rules`, `PATCH/DELETE /api/mute-rules/{id}`。
- 再評価バッチ: `POST /api/mute-rules/apply`（既存記事も含め全件にルールを当て直す）。
- 読み取り時の hide 反映: `articles` 一覧クエリへ `muted_at IS NULL` ガードを1条件追加（`include_muted` で解除可能）。**これが既存スライスへ触れる唯一の実コード変更**（§5.6 に根拠と最小性を明記）。
- クロール後の自動適用: `shared/scheduler.rs` の定期取得ループ末尾に `apply_all` 呼び出しを1行追加（§5.7）。
- パターン安全化の純関数（LIKE ワイルドカードのエスケープ）と、フィールド→カラム名の whitelist マッピング純関数、およびその単体テスト。
- 結合テスト `scripts/test/api-mute-rules.sh`（psql で決定論シード → ルール作成 → apply → 一覧の実挙動を assert）。
- フロント: `lib/api.ts` に `MuteRule` 型と CRUD/apply メソッド追加。`components/mute/MuteRulesManager.tsx`（自己完結の管理 UI）を新設し、`routes/Settings.tsx` にセクションとしてマウント。

**含まない（別チケット / 別機能）**

- **正規表現マッチ（`match_type='regex'`）**。列・enum は前方互換に作るが、v1 は `contains` のみ。regex は `regex` クレート追加と ReDoS 対策が要るため #28（ルールエンジン）に回す（§11）。
- **著者（author）フィールド**。`articles` に著者カラムが無く、追加するとクロール（`feeds/service::fetch_and_store`）と `articles::repository::upsert` のシグネチャ変更という越境改修になるため本チケットでは扱わない。代替として **`url`（ドメイン部分一致）** で「特定サイトをミュート」を満たす。author の将来追加手順は §11 に記す。
- **AI によるルール提案 / 意味ベースのミュート**（#28、`shared/llm` 利用）。本書 §5.8 に拡張点のみ記述。
- **フォルダ単位・フィード単位のミュートスコープ**（全フィード横断のグローバルルールのみ。per-feed スコープは #28 で `feed_id` 列を足して拡張）。
- **`mark_read` の自動取り消し（un-read）**。`mark_read` は適用時に `is_read=true` を立てるのみで、ルール削除しても既読は戻さない（手動既読と区別できないため）。`hide` は再評価で取り消せる（§5.5）。

## 3. 既存実装の調査と再利用

**車輪の再発明をしないため、以下を再利用する。** いずれもこのセッションで実ファイルを開いて確認済み。

- **`folders` スライスが CRUD の前例**（`backend/src/features/folders/{handler,service,repository,mod}.rs`）。`POST` は `201 + Json(entity)`、`PATCH` は更新後エンティティ返却、`DELETE` は `204`。`mute_rules` の CRUD ハンドラ・ルート定義はこの形をそっくり踏襲する。
- **`articles::repository::list` の「オプション条件をプレースホルダで足す」パターン**（`backend/src/features/articles/repository.rs`）。既に `($1::uuid IS NULL OR feed_id=$1)` や `($4 = false OR feed_id IN (SELECT id FROM feeds WHERE folder_id IS NULL))` のように、**他スライスのテーブルを subquery しながらオプション条件を AND で重ねている**。本機能の `muted_at IS NULL` ガードはこの確立済みパターンに1条件足すだけで、新しい越境共通レイヤーではない。
- **`articles.is_read`（`NOT NULL DEFAULT false`）と部分インデックス `idx_articles_is_read WHERE is_read=false`**（`0001_init.sql`）。`action='mark_read'` は新カラムを足さず既存 `is_read` を立てるだけで、未読数（`feed_overview.unread_count` / `stats.unread`）から自動的に外れる。
- **`feeds/service::fetch_and_store` のクロール → `articles::repository::upsert` 呼び出し**（`backend/src/features/feeds/service.rs`）。新着記事はここで入る。自動適用はクロールを束ねる `shared/scheduler.rs`（`feeds::service::refresh_all_feeds` を呼ぶ層）の末尾に1行足す形にし、`feeds` スライス本体は触らない（§5.7）。
- **`shared/error.rs` の `AppError` + `AppResult`**。`sqlx::Error` は `#[from]` で `Database`(500)、不正入力は `Validation`(400)。新バリアント不要。
- **`gen_random_uuid()`**（PostgreSQL 17 コア組込）。`scripts/test/api-feed-overview.sh` のシードや既存マイグレーションで使用実績あり。`mute_rules.id` の DEFAULT に使う（Rust 側 `Uuid::new_v4()` でも可だが DB DEFAULT に寄せる）。
- **フロント `lib/api.ts` の `http<T>()` ヘルパ**（204→undefined 畳み込み、非2xx は throw、先頭3桁を `errorStatus()` で抽出）。`folders` の `list/create/update/delete` と同型メソッドを足すだけ。
- **`routes/Settings.tsx`**（Instapaper 資格情報・テーマ等の設定ホスト）。ミュート管理 UI はここにセクション追加で同居させ、新ルートを増やさない。
- **`components/ui/`（`button`/`input`/`card`/`switch`/`badge`）と oklch トークン**（`bg-background`/`border-border`/`text-muted-foreground`）。管理 UI はこれらだけで組み、Ark UI も新色も不要。

## 4. データモデルとマイグレーション

新規マイグレーション **`backend/migrations/0006_mute_rules.sql`**（暫定番号 — §冒頭の注意参照）。**既存ファイルは編集しない。** 完全文:

```sql
-- 19 ミュート（NGワード）フィルタ。
--
-- mute_rules: ユーザー定義のミュートルール。field（一致対象カラム）× pattern（部分一致語）
--   × action（hide=一覧から除外 / mark_read=既読化）。match_type は前方互換のため列だけ
--   用意し、v1 は 'contains'（大小無視の部分一致）のみ許可する。
-- articles.muted_at: hide ルールに合致した記事へ立てる「非表示スタンプ」。NULL = 表示。
--   apply バッチで再計算する（hide ルールの追加/編集/削除に追従するため、再計算前に NULL へ
--   リセットしてから当て直す）。mark_read は既存 articles.is_read を使うので新カラム不要。

CREATE TABLE IF NOT EXISTS mute_rules (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    field       TEXT        NOT NULL CHECK (field IN ('title', 'content', 'url')),
    pattern     TEXT        NOT NULL CHECK (length(btrim(pattern)) > 0),
    match_type  TEXT        NOT NULL DEFAULT 'contains' CHECK (match_type IN ('contains')),
    action      TEXT        NOT NULL DEFAULT 'hide'     CHECK (action IN ('hide', 'mark_read')),
    enabled     BOOLEAN     NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 有効ルールだけを走査する apply バッチ用。
CREATE INDEX IF NOT EXISTS idx_mute_rules_enabled
    ON mute_rules (enabled) WHERE enabled = true;

-- hide スタンプ。NULL（=表示）の記事を一覧クエリで素早く絞るための部分インデックス。
ALTER TABLE articles
    ADD COLUMN IF NOT EXISTS muted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_articles_muted_at
    ON articles (muted_at) WHERE muted_at IS NULL;
```

設計ノート:

- `field` / `match_type` / `action` は `CHECK` 制約で許可値を固定。`field` の許可値が、§5 で組み立てる動的 SQL のカラム名 whitelist と**二重に**一致を保証する（DB 側でも不正値を弾く）。
- `pattern` は `btrim` 後の長さ>0 を要求し、空白だけのルール（全件一致してしまう）を DB レベルで禁止。サービス層（§5.4）でも検証する。
- `muted_at` は **追記カラム**（CLAUDE.md「新カラムを足したら `migrations/` に新ファイルを追加」に従う）。`ADD COLUMN IF NOT EXISTS` で再実行に冪等。
- 既存 `articles` の他カラム・他インデックスは不変。`SELECT *`（`articles::repository` が使用）は新カラムを自動で拾うため、`Article` 構造体に `muted_at` を1フィールド足すだけで `FromRow` が通る（§5.6）。

## 5. バックエンド設計

新スライス `backend/src/features/mute_rules/`。ルールの CRUD と適用バッチを持つ。`mute_rules` への読み書きは自スライスで完結し、`articles` への書き込み（`muted_at` / `is_read` の一括更新）は適用バッチが行う。これは禁止される「越境共通レイヤー」ではなく、`feeds/service` がクロール時に `articles::repository::upsert` で `articles` を書くのと同じ「機能起点の集約跨ぎ書き込み」である。新 trait / dyn は追加しない。

### 5.1 ルート設計と衝突回避

`/api/mute-rules`（GET/POST）、`/api/mute-rules/{id}`（PATCH/DELETE）、`/api/mute-rules/apply`（POST）。`apply` は静的セグメントなので axum 0.8（matchit）では動的 `{id}` より優先され、`/api/mute-rules/apply` が `{id}="apply"` に化けることはない。既存ルート（`/api/feeds*`, `/api/articles*`, `/api/folders*`, `/api/search`, `/api/instapaper*`, `/api/read-later*`, `/api/stats`, `/api/feeds/overview`）と prefix が重ならないため `.merge()` で衝突しない。

### 5.2 `domain.rs`

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct MuteRuleId(pub Uuid);

/// 永続化されたミュートルール。field/match_type/action は CHECK 制約付き TEXT を
/// そのまま String で受ける（DB が許可値を保証する）。フロントとの契約は文字列。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MuteRule {
    pub id: MuteRuleId,
    pub field: String,      // "title" | "content" | "url"
    pub pattern: String,
    pub match_type: String, // "contains"（v1 はこれのみ）
    pub action: String,     // "hide" | "mark_read"
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// field 文字列 → articles の実カラム名（whitelist）。
/// 動的 SQL にカラム名を埋め込むのは「列名はパラメータ化できない」ため不可避だが、
/// ここで enum 的に固定値だけを返すので SQL インジェクションは起きない
/// （ユーザー文字列をそのまま連結しない）。未知の値は Validation(400)。
pub fn field_column(field: &str) -> AppResult<&'static str> {
    match field {
        "title" => Ok("title"),
        "content" => Ok("content"),
        "url" => Ok("url"),
        other => Err(AppError::Validation(format!("unknown mute field: {other}"))),
    }
}

/// LIKE/ILIKE のワイルドカード（% _ \）をエスケープする純関数。
/// ユーザーパターンに含まれる % や _ をリテラルとして扱うため、ESCAPE '\' と併用する。
/// 例: "50%_off\\x" -> "50\\%\\_off\\\\x"。これを '%' || escaped || '%' で囲んで部分一致にする。
pub fn escape_like(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    for ch in pattern.chars() {
        match ch {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// 作成・更新時の入力検証（DB CHECK の手前で 400 に整形する）。
pub fn validate(field: &str, pattern: &str, match_type: &str, action: &str) -> AppResult<()> {
    field_column(field)?; // 未知 field を弾く
    if pattern.trim().is_empty() {
        return Err(AppError::Validation("pattern must not be empty".into()));
    }
    if match_type != "contains" {
        return Err(AppError::Validation(format!(
            "unsupported match_type: {match_type} (only 'contains' in v1)"
        )));
    }
    if action != "hide" && action != "mark_read" {
        return Err(AppError::Validation(format!("unknown action: {action}")));
    }
    Ok(())
}

/// 作成入力（handler の Json ボディ）。match_type/action は省略時デフォルトを持つ。
#[derive(Debug, Deserialize)]
pub struct NewMuteRule {
    pub field: String,
    pub pattern: String,
    #[serde(default = "default_match_type")]
    pub match_type: String,
    #[serde(default = "default_action")]
    pub action: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// 部分更新（PATCH）。None のフィールドは変更しない。
#[derive(Debug, Deserialize)]
pub struct PatchMuteRule {
    pub field: Option<String>,
    pub pattern: Option<String>,
    pub match_type: Option<String>,
    pub action: Option<String>,
    pub enabled: Option<bool>,
}

fn default_match_type() -> String {
    "contains".into()
}
fn default_action() -> String {
    "hide".into()
}
fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_like_passes_plain_text() {
        assert_eq!(escape_like("Sponsored"), "Sponsored");
    }

    #[test]
    fn escape_like_escapes_percent_and_underscore() {
        assert_eq!(escape_like("50%_off"), "50\\%\\_off");
    }

    #[test]
    fn escape_like_escapes_backslash() {
        assert_eq!(escape_like("a\\b"), "a\\\\b");
    }

    #[test]
    fn escape_like_handles_unicode() {
        // 日本語はワイルドカードを含まないのでそのまま。
        assert_eq!(escape_like("広告"), "広告");
    }

    #[test]
    fn field_column_maps_known_fields() {
        assert_eq!(field_column("title").unwrap(), "title");
        assert_eq!(field_column("content").unwrap(), "content");
        assert_eq!(field_column("url").unwrap(), "url");
    }

    #[test]
    fn field_column_rejects_unknown_field() {
        // SQL インジェクション語をそのまま渡しても弾かれる（連結されない）。
        assert!(field_column("title; DROP TABLE articles--").is_err());
    }

    #[test]
    fn validate_rejects_empty_pattern() {
        assert!(validate("title", "   ", "contains", "hide").is_err());
    }

    #[test]
    fn validate_rejects_regex_in_v1() {
        assert!(validate("title", "ad", "regex", "hide").is_err());
    }

    #[test]
    fn validate_rejects_unknown_action() {
        assert!(validate("title", "ad", "contains", "delete").is_err());
    }

    #[test]
    fn validate_accepts_well_formed_rule() {
        assert!(validate("url", "example.com", "contains", "mark_read").is_ok());
    }
}
```

### 5.3 `repository.rs`

```rust
use sqlx::PgPool;

use super::domain::{self, MuteRule, MuteRuleId};
use crate::shared::error::{AppError, AppResult};

pub async fn list_all(pool: &PgPool) -> AppResult<Vec<MuteRule>> {
    let rows = sqlx::query_as::<_, MuteRule>(
        "SELECT * FROM mute_rules ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get(pool: &PgPool, id: MuteRuleId) -> AppResult<MuteRule> {
    sqlx::query_as::<_, MuteRule>("SELECT * FROM mute_rules WHERE id = $1")
        .bind(id.0)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn insert(
    pool: &PgPool,
    field: &str,
    pattern: &str,
    match_type: &str,
    action: &str,
    enabled: bool,
) -> AppResult<MuteRule> {
    let row = sqlx::query_as::<_, MuteRule>(
        r#"INSERT INTO mute_rules (field, pattern, match_type, action, enabled)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(field)
    .bind(pattern.trim())
    .bind(match_type)
    .bind(action)
    .bind(enabled)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 部分更新。COALESCE で「渡された列だけ」更新する（NULL = 変更なし）。
#[allow(clippy::too_many_arguments)]
pub async fn update(
    pool: &PgPool,
    id: MuteRuleId,
    field: Option<&str>,
    pattern: Option<&str>,
    match_type: Option<&str>,
    action: Option<&str>,
    enabled: Option<bool>,
) -> AppResult<MuteRule> {
    sqlx::query_as::<_, MuteRule>(
        r#"UPDATE mute_rules SET
             field      = COALESCE($2, field),
             pattern    = COALESCE($3, pattern),
             match_type = COALESCE($4, match_type),
             action     = COALESCE($5, action),
             enabled    = COALESCE($6, enabled),
             updated_at = now()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id.0)
    .bind(field)
    .bind(pattern.map(str::trim))
    .bind(match_type)
    .bind(action)
    .bind(enabled)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn delete(pool: &PgPool, id: MuteRuleId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM mute_rules WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// hide スタンプを全消去（再評価の前段）。冪等。
pub async fn clear_all_hidden(pool: &PgPool) -> AppResult<u64> {
    let res = sqlx::query("UPDATE articles SET muted_at = now() WHERE false") // placeholder, see note
        .execute(pool)
        .await?;
    // ↑実際は下のクエリ。上の行は使わない（説明用）。
    let _ = res;
    let res = sqlx::query("UPDATE articles SET muted_at = NULL WHERE muted_at IS NOT NULL")
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// 1ルールを既存記事へ適用する。action と field に応じて UPDATE を1本実行し、
/// 影響行数を返す。カラム名は domain::field_column（whitelist）由来なので注入安全。
pub async fn apply_rule(
    pool: &PgPool,
    field: &str,
    pattern: &str,
    action: &str,
) -> AppResult<u64> {
    let col = domain::field_column(field)?; // "title" | "content" | "url"
    let needle = format!("%{}%", domain::escape_like(pattern.trim()));

    // action ごとに対象（hide=muted_at NULL のみ / mark_read=is_read false のみ）を絞る。
    let sql = match action {
        "hide" => format!(
            "UPDATE articles SET muted_at = now() \
             WHERE muted_at IS NULL AND {col} ILIKE $1 ESCAPE '\\'"
        ),
        "mark_read" => format!(
            "UPDATE articles SET is_read = true \
             WHERE is_read = false AND {col} ILIKE $1 ESCAPE '\\'"
        ),
        other => return Err(AppError::Validation(format!("unknown action: {other}"))),
    };

    let res = sqlx::query(&sql).bind(&needle).execute(pool).await?;
    Ok(res.rows_affected())
}
```

> 注: 上記 `clear_all_hidden` の最初の `sqlx::query(... WHERE false)` 行は説明上の冗長記述。実装では2本目の `UPDATE articles SET muted_at = NULL WHERE muted_at IS NOT NULL` のみを残すこと。

設計ノート:

- **`query_as` / `query`（runtime クエリ）のみ。`query!` マクロは使わない**（ビルド時 DB 接続を要求するため禁止、CLAUDE.md）。
- カラム名 `{col}` は `format!` で SQL に埋めるが、値は `domain::field_column` の固定文字列（`"title"`/`"content"`/`"url"`）のみ。ユーザー入力の連結は無く、パターン値は `$1` でパラメータ化＋`escape_like` 済みなので**インジェクションは起きない**。
- `ILIKE ... ESCAPE '\'` で大小無視・ワイルドカードエスケープ。`escape_like` が `%` `_` `\` を `\` で前置するので、パターン中のこれらはリテラル扱いになる。
- `hide` は `muted_at IS NULL` の行だけ更新（既にミュート済みを二度打たない）。`mark_read` は `is_read=false` の行だけ更新（手動既読を上書きしない・行数を正確に数える）。

### 5.4 `service.rs`

```rust
use serde::Serialize;

use super::domain::{self, MuteRule, MuteRuleId, NewMuteRule, PatchMuteRule};
use super::repository;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn list_rules(state: &AppState) -> AppResult<Vec<MuteRule>> {
    repository::list_all(&state.db).await
}

pub async fn create_rule(state: &AppState, input: NewMuteRule) -> AppResult<MuteRule> {
    domain::validate(&input.field, &input.pattern, &input.match_type, &input.action)?;
    let rule = repository::insert(
        &state.db,
        &input.field,
        &input.pattern,
        &input.match_type,
        &input.action,
        input.enabled,
    )
    .await?;
    // 作成直後に当ルールを既存記事へ適用しておく（即時反映。hide はリセット不要＝
    // 新規ルールは既存スタンプを壊さない。mark_read は追加的に既読化）。
    if rule.enabled {
        repository::apply_rule(&state.db, &rule.field, &rule.pattern, &rule.action).await?;
    }
    Ok(rule)
}

pub async fn update_rule(
    state: &AppState,
    id: MuteRuleId,
    patch: PatchMuteRule,
) -> AppResult<MuteRule> {
    // 変更後の最終形を検証するため、現行値とマージしてから validate する。
    let current = repository::get(&state.db, id).await?;
    let field = patch.field.as_deref().unwrap_or(&current.field);
    let pattern = patch.pattern.as_deref().unwrap_or(&current.pattern);
    let match_type = patch.match_type.as_deref().unwrap_or(&current.match_type);
    let action = patch.action.as_deref().unwrap_or(&current.action);
    domain::validate(field, pattern, match_type, action)?;

    let rule = repository::update(
        &state.db,
        id,
        patch.field.as_deref(),
        patch.pattern.as_deref(),
        patch.match_type.as_deref(),
        patch.action.as_deref(),
        patch.enabled,
    )
    .await?;
    // ルール変更は hide 状態を変えうる（語の縮小・無効化で再表示が必要）。全件再評価する。
    apply_all(state).await?;
    Ok(rule)
}

pub async fn delete_rule(state: &AppState, id: MuteRuleId) -> AppResult<()> {
    let affected = repository::delete(&state.db, id).await?;
    if affected == 0 {
        return Err(crate::shared::error::AppError::NotFound);
    }
    // 削除でミュート対象が減ったら再表示するため再評価（hide のみ取り消せる。§11）。
    apply_all(state).await?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ApplyReport {
    pub rules_evaluated: usize,
    pub hidden: u64,
    pub marked_read: u64,
}

/// 全有効ルールを既存記事へ当て直す。hide は「一旦全解除 → 有効 hide ルールで再付与」で
/// ルールの追加/編集/削除に冪等に追従する。mark_read は加算的（取り消さない）。
pub async fn apply_all(state: &AppState) -> AppResult<ApplyReport> {
    let rules = repository::list_all(&state.db).await?;
    let enabled: Vec<&MuteRule> = rules.iter().filter(|r| r.enabled).collect();

    // hide はリセットしてから再計算（取り消しを実現）。
    repository::clear_all_hidden(&state.db).await?;

    let mut hidden = 0u64;
    let mut marked_read = 0u64;
    for r in &enabled {
        let n = repository::apply_rule(&state.db, &r.field, &r.pattern, &r.action).await?;
        match r.action.as_str() {
            "hide" => hidden += n,
            "mark_read" => marked_read += n,
            _ => {}
        }
    }
    Ok(ApplyReport {
        rules_evaluated: enabled.len(),
        hidden,
        marked_read,
    })
}
```

### 5.5 `handler.rs`

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use super::domain::{MuteRule, MuteRuleId, NewMuteRule, PatchMuteRule};
use super::service::{self, ApplyReport};
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<MuteRule>>> {
    Ok(Json(service::list_rules(&state).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewMuteRule>,
) -> AppResult<(StatusCode, Json<MuteRule>)> {
    let rule = service::create_rule(&state, body).await?;
    Ok((StatusCode::CREATED, Json(rule))) // 201（folders::create 前例）
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchMuteRule>,
) -> AppResult<Json<MuteRule>> {
    let rule = service::update_rule(&state, MuteRuleId(id), body).await?;
    Ok(Json(rule))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::delete_rule(&state, MuteRuleId(id)).await?;
    Ok(StatusCode::NO_CONTENT) // 204
}

pub async fn apply(State(state): State<AppState>) -> AppResult<Json<ApplyReport>> {
    Ok(Json(service::apply_all(&state).await?))
}
```

### 5.6 `mod.rs` と `articles` 一覧クエリへの最小追記

```rust
// backend/src/features/mute_rules/mod.rs
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/mute-rules", get(handler::list).post(handler::create))
        .route(
            "/api/mute-rules/{id}",
            axum::routing::patch(handler::update).delete(handler::delete),
        )
        .route("/api/mute-rules/apply", post(handler::apply))
}
```

`features/mod.rs` への合成（**追加2行のみ** — 既存スライスは不変）:

```rust
pub mod mute_rules; // ← 追加（モジュール宣言）

// ...router() 内、任意の .merge() の隣...
        .merge(mute_rules::routes()) // ← 追加（1行）
```

**唯一の既存スライス実コード変更 = `articles` 一覧の hide ガード**。理由と最小性:

`articles::repository::list` は既に「他テーブルを subquery しつつオプション条件を AND で重ねる」設計（folder フィルタが `feeds` を subquery 済み）。hide 反映はその確立済みパターンに**1条件**足すだけで、新しい越境共通レイヤーではない（CLAUDE.md「逸脱時は理由を述べる」に従い明記）。`muted_at` カラムは同じ `articles` 集約上なので、`SELECT *` がそのまま拾う。

1. `articles/domain.rs` の `Article` に1フィールド追加（`SELECT *` と `FromRow` の整合のため必須）:

   ```rust
   pub muted_at: Option<chrono::DateTime<chrono::Utc>>,
   ```

2. `articles/repository.rs::list` の WHERE 末尾に1条件、引数とバインドを1つ追加:

   ```rust
   pub async fn list(
       pool: &PgPool,
       feed_id: Option<FeedId>,
       unread_only: bool,
       folder_id: Option<FolderId>,
       unclassified: bool,
       include_muted: bool, // ← 追加
   ) -> AppResult<Vec<Article>> {
       let rows = sqlx::query_as::<_, Article>(
           r#"SELECT * FROM articles
              WHERE ($1::uuid IS NULL OR feed_id = $1)
                AND ($2 = false OR is_read = false)
                AND ($3::uuid IS NULL
                     OR feed_id IN (SELECT id FROM feeds WHERE folder_id = $3))
                AND ($4 = false
                     OR feed_id IN (SELECT id FROM feeds WHERE folder_id IS NULL))
                AND ($5 = true OR muted_at IS NULL)   -- ← 追加（hide 反映）
              ORDER BY published_at DESC NULLS LAST, created_at DESC
              LIMIT 200"#,
       )
       .bind(feed_id.map(|f| f.0))
       .bind(unread_only)
       .bind(folder_id.map(|f| f.0))
       .bind(unclassified)
       .bind(include_muted) // ← 追加
       .fetch_all(pool)
       .await?;
       Ok(rows)
   }
   ```

3. `articles/service.rs::list_articles` に `include_muted: bool` を通し、`repository::list` へ渡す。
4. `articles/handler.rs::ListQuery` に `#[serde(default)] pub include_muted: bool,` を追加し、`service::list_articles(..., q.include_muted)` を渡す（既定 `false` = ミュート除外。`?include_muted=true` で全件取得＝管理画面のプレビュー用）。

> これらは「同一 `articles` 集約の読み取りクエリへフィルタを足す」だけで、CRUD・要約・既読など他の articles 機能には一切触れない。新規の追加カラム（`muted_at`）も含め、CLAUDE.md の許容範囲（新カラムは新マイグレーションで追記）に収まる。

### 5.7 クロール後の自動適用（`shared/scheduler.rs` に1行）

新着記事へルールを反映するため、定期取得ループが `feeds::service::refresh_all_feeds` を呼んだ直後に `mute_rules::service::apply_all` を呼ぶ。`feeds` スライス本体は触らず、クロールを束ねるスケジューラ層に1行足すだけ:

```rust
// shared/scheduler.rs の tick ループ内、refresh_all_feeds の後
if let Err(e) = crate::features::feeds::service::refresh_all_feeds(&state).await {
    tracing::error!(error = %e, "feed refresh failed");
}
// ↓ 追加（ミュート自動適用。失敗してもクロールは継続）
if let Err(e) = crate::features::mute_rules::service::apply_all(&state).await {
    tracing::error!(error = %e, "mute apply failed");
}
```

`apply_all` は hide を毎回リセット→再計算するので冪等。クロール直後に走らせることで、新着のうち NG 語を含むものがその周期で hide / 既読化される。即時性が要るなら、フロントは記事リスト表示前に明示的に `POST /api/mute-rules/apply` を呼ぶこともできる（§6）。

### 5.8 AI 拡張の置き場所（本チケット範囲外・#28 で実装）

ミュート本体は決定論的で LLM 非依存だが、将来「この記事はミュート対象か」を意味ベースで判定する補助を入れる場合の置き場所だけ明示しておく（実装しない）:

- 新エンドポイント例 `POST /api/mute-rules/suggest`（記事群から NG 語候補を提案）を `mute_rules` スライス内に足し、`shared/llm::LlmClient` を再利用する。
- パターンは要約/翻訳と同じ: `state.config.anthropic_api_key` が `None` なら `AppError::NotEnabled("ANTHROPIC_API_KEY is not set")` を返し、結果は DB（提案キャッシュ用の新テーブル or 既存ルールへの付帯列）にキャッシュしてトークンを節約する。
- これは #28（ルールエンジン）の領域。本チケットでは列・API を増やさない。

### 5.9 AppError の使い分け

- 不正な `field`/`match_type`/`action`/空 `pattern` → `Validation`(400)（DB CHECK の手前で `domain::validate` が整形）。
- 存在しないルール ID への PATCH/DELETE → `NotFound`(404)。
- DB エラー → `sqlx::Error` から `#[from]` で `Database`(500)、`?` 伝播。
- 新バリアント追加なし（`shared/error.rs` 不編集）。

## 6. フロントエンド設計

> 方針: ミュート管理はリスト＋フォーム＋トグルの素直な CRUD UI。Ark UI の a11y 部品は不要で、既存 `components/ui/`（`button`/`input`/`card`/`switch`/`badge`）と oklch トークンだけで組む。新ルートは作らず、既存 `routes/Settings.tsx` にセクション追加で同居させる（設定系の自然な置き場所）。

### 6.1 `lib/api.ts`（型 + メソッド追加）

```ts
export interface MuteRule {
  id: string;
  field: "title" | "content" | "url";
  pattern: string;
  match_type: "contains";
  action: "hide" | "mark_read";
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface MuteApplyReport {
  rules_evaluated: number;
  hidden: number;
  marked_read: number;
}

// api オブジェクト内に追加（folders と同型の CRUD）
listMuteRules: () => http<MuteRule[]>("/api/mute-rules"),
createMuteRule: (input: {
  field: MuteRule["field"];
  pattern: string;
  action?: MuteRule["action"];
  enabled?: boolean;
}) =>
  http<MuteRule>("/api/mute-rules", {
    method: "POST",
    body: JSON.stringify(input),
  }),
updateMuteRule: (id: string, patch: Partial<Pick<MuteRule, "field" | "pattern" | "action" | "enabled">>) =>
  http<MuteRule>(`/api/mute-rules/${id}`, {
    method: "PATCH",
    body: JSON.stringify(patch),
  }),
deleteMuteRule: (id: string) =>
  http<void>(`/api/mute-rules/${id}`, { method: "DELETE" }),
applyMuteRules: () =>
  http<MuteApplyReport>("/api/mute-rules/apply", { method: "POST" }),
```

- 命名は既存規約「動詞 + リソース camelCase」（`listFolders`/`createFolder` 等）に揃える。
- バックエンドが PATCH/DELETE/apply のたびに自動で再評価するので（§5.4）、フロントは追加で apply を呼ばなくても整合する。ただし「今すぐ既存記事へ反映」ボタン用に `applyMuteRules()` を露出する。

### 6.2 管理コンポーネント `components/mute/MuteRulesManager.tsx`（新規・自己完結）

`listFeedOverview` 系の前例（自前 `createResource` で取得して描画する独立部品）に倣い、自分でルール一覧を取得・操作する。`Settings` のどこに置いても動く。

```tsx
import { createResource, createSignal, For, Show } from "solid-js";
import { api, type MuteRule } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";

const FIELD_LABEL: Record<MuteRule["field"], string> = {
  title: "タイトル",
  content: "本文",
  url: "URL（ドメイン）",
};
const ACTION_LABEL: Record<MuteRule["action"], string> = {
  hide: "非表示",
  mark_read: "既読化",
};

export default function MuteRulesManager() {
  const [rules, { refetch }] = createResource(() => api.listMuteRules());
  const [field, setField] = createSignal<MuteRule["field"]>("title");
  const [pattern, setPattern] = createSignal("");
  const [action, setAction] = createSignal<MuteRule["action"]>("hide");
  const [busy, setBusy] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);

  async function add(e: Event) {
    e.preventDefault();
    if (!pattern().trim()) return;
    setBusy(true);
    setErr(null);
    try {
      await api.createMuteRule({ field: field(), pattern: pattern().trim(), action: action() });
      setPattern("");
      await refetch();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function toggle(r: MuteRule) {
    await api.updateMuteRule(r.id, { enabled: !r.enabled });
    await refetch();
  }

  async function remove(r: MuteRule) {
    await api.deleteMuteRule(r.id);
    await refetch();
  }

  return (
    <section class="space-y-3">
      <h2 class="text-sm font-semibold">ミュート（NGワード）</h2>

      <form class="flex flex-wrap items-center gap-2" onSubmit={add}>
        <select
          class="h-9 rounded-md border border-border bg-background px-2 text-sm"
          value={field()}
          onChange={(e) => setField(e.currentTarget.value as MuteRule["field"])}
        >
          <option value="title">タイトル</option>
          <option value="content">本文</option>
          <option value="url">URL（ドメイン）</option>
        </select>
        <Input
          class="min-w-40 flex-1"
          placeholder="NGワード（部分一致）"
          value={pattern()}
          onInput={(e) => setPattern(e.currentTarget.value)}
        />
        <select
          class="h-9 rounded-md border border-border bg-background px-2 text-sm"
          value={action()}
          onChange={(e) => setAction(e.currentTarget.value as MuteRule["action"])}
        >
          <option value="hide">非表示</option>
          <option value="mark_read">既読化</option>
        </select>
        <Button type="submit" disabled={busy() || !pattern().trim()}>
          追加
        </Button>
      </form>
      <Show when={err()}>
        <p class="text-xs text-destructive">{err()}</p>
      </Show>

      <Show
        when={(rules()?.length ?? 0) > 0}
        fallback={<p class="text-sm text-muted-foreground">ルールはありません。</p>}
      >
        <ul class="divide-y divide-border">
          <For each={rules()}>
            {(r) => (
              <li class="flex items-center justify-between gap-3 py-2">
                <div class="flex min-w-0 items-center gap-2">
                  <Badge>{FIELD_LABEL[r.field]}</Badge>
                  <span class="truncate text-sm font-medium">{r.pattern}</span>
                  <Badge variant="secondary">{ACTION_LABEL[r.action]}</Badge>
                </div>
                <div class="flex items-center gap-2">
                  <Switch checked={r.enabled} onChange={() => toggle(r)} />
                  <Button variant="ghost" onClick={() => remove(r)}>
                    削除
                  </Button>
                </div>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}
```

ポイント:

- `import { createResource, createSignal, For, Show } from "solid-js"` を明記（コピペ即実装）。`@/` エイリアスと既存 `api` を使う。
- バックエンドが作成/更新/削除のたびに `apply_all` を走らせる（§5.4）ので、UI は `refetch()` でルール一覧を更新するだけ。記事一覧側は次回 `listArticles()`（既定で hide 除外）で反映される。
- `Switch`/`Badge`/`Button`/`Input` の props 名は実装時に `components/ui/` の実シグネチャへ合わせる（`checked`/`variant` 等は前例に倣う）。プレーンな `<select>` は自前 Tailwind で十分（Ark UI 不要）。

### 6.3 `routes/Settings.tsx` へのマウント（1行差し込み）

`Settings` の JSX に `import MuteRulesManager from "@/components/mute/MuteRulesManager";` と、設定カード群の中へ `<MuteRulesManager />` を1箇所差し込むだけ。既存の Instapaper 設定等のセクションには触れない。

### 6.4 状態管理・トークン

- グローバル状態（`store.tsx`）の変更は不要。ルール一覧はコンポーネントローカルの `createResource` に閉じる。
- 記事一覧の hide 反映は「既定でミュート除外された結果が返る」ことで自動的に成立する。リアルタイムに反映したい場合のみ、ルール変更後に記事一覧を再 `refetch` する（`store` の `refetchFeeds` 相当の記事再取得を呼ぶ箇所があれば併用。無ければ画面遷移で更新される）。
- 装飾は意味トークンのみ（`border-border`/`bg-background`/`text-muted-foreground`/`text-destructive`）。新色・生 hex は持ち込まない。

## 7. API 契約

### 7.1 `GET /api/mute-rules`

- リクエスト: なし。
- レスポンス `200`: `MuteRule[]`（`created_at` 降順）。

```json
[
  {
    "id": "b2f1c0a4-1111-4222-8333-444455556666",
    "field": "title",
    "pattern": "Sponsored",
    "match_type": "contains",
    "action": "hide",
    "enabled": true,
    "created_at": "2026-06-30T09:00:00Z",
    "updated_at": "2026-06-30T09:00:00Z"
  }
]
```

### 7.2 `POST /api/mute-rules`

- リクエスト（`match_type` 省略時 `"contains"`、`action` 省略時 `"hide"`、`enabled` 省略時 `true`）:

```json
{ "field": "url", "pattern": "ad.example.com", "action": "mark_read" }
```

- レスポンス `201`: 作成された `MuteRule`。作成直後に当ルールが既存記事へ適用される（§5.4）。
- エラー: `400`（`field`/`action`/`match_type` 不正、`pattern` が空白のみ）。

### 7.3 `PATCH /api/mute-rules/{id}`

- リクエスト（変更したいフィールドのみ。全て任意）:

```json
{ "pattern": "PR記事", "enabled": false }
```

- レスポンス `200`: 更新後の `MuteRule`。更新後に全件再評価（hide のリセット→再付与）が走る。
- エラー: `404`（ID なし）、`400`（マージ後の値が不正）。

### 7.4 `DELETE /api/mute-rules/{id}`

- レスポンス `204`。削除後に全件再評価が走り、その hide ルールで隠れていた記事は再表示される（`mark_read` は戻らない、§11）。
- エラー: `404`。

### 7.5 `POST /api/mute-rules/apply`

- リクエスト: ボディなし。
- レスポンス `200`: 適用レポート。

```json
{ "rules_evaluated": 3, "hidden": 12, "marked_read": 5 }
```

- 意味: `hidden` = 今回新たに `muted_at` を立てた件数、`marked_read` = 今回新たに既読化した件数（hide はリセット後の再付与総数、mark_read は `is_read=false`→`true` にした件数）。

### 7.6 既存 `GET /api/articles` の拡張（破壊的でない）

- 新クエリ `include_muted`（既定 `false`）。`false`（既定）= `muted_at IS NULL` のみ返す（hide 反映）。`true` = ミュート済みも含む（管理プレビュー用）。
- 既存呼び出し（パラメータ無し）は従来どおり動き、加えて hide 済みが自動的に除外される。例: `GET /api/articles?feed_id=...&unread=true&include_muted=true`。

## 8. 依存関係

- **依存する機能（このチケットが必要とするもの）: なし。`dependsOn` は空。** 既存 `articles`/`feeds` テーブルと `shared/scheduler.rs` のみで完結。フロントも `Settings`（既存）にセクションを足すだけで、二ペイン（#10）等の未着手機能を待たない。
- **このチケットが土台になる機能**:
  - **#28 ルールエンジン**: 本スライスの `field`/`pattern`/`match_type`/`action` 語彙はその部分集合。#28 は `match_type='regex'`・per-feed スコープ・AI 提案（`shared/llm`）を本スライスへ拡張する形で実装できる（列を前方互換に作ってある）。
- **関係するが本機能では触れないもの**:
  - 既読管理（#09）/ 投稿統計（`feed_overview`）: `action='mark_read'` は既存 `is_read` を立てるだけなので、これらの未読数集計に自動で反映される（追加対応不要）。
  - フォルダ（#02）: 本スライスは `folder_id` を参照しない（グローバルルールのみ）。

## 9. テスト計画（TDD）

**Red → 理解 → Green の順。書いたら必ず実行する。** 配置は既存実慣習に従う: 純粋ロジックは `#[cfg(test)] mod tests`（`articles/handler.rs` や `feeds/domain.rs` が前例）、結合は `scripts/test/*.sh`（起動済みスタックへ HTTP ＋ psql シード）。`backend/tests/` は存在せず、本 crate はバイナリ専用で library target が無いため使わない。

### 9.1 単体テスト（`#[cfg(test)] mod tests`、§5.2 に同梱・DB 不要）

`mute_rules/domain.rs` に**先に**書く（Red）。

| テスト | 意図 |
|--------|------|
| `escape_like_passes_plain_text` | ワイルドカード無しの語は素通し |
| `escape_like_escapes_percent_and_underscore` | `%`/`_` をリテラル化（誤マッチ防止） |
| `escape_like_escapes_backslash` | `\` 自体のエスケープ |
| `escape_like_handles_unicode` | 日本語語はそのまま（多バイトでも壊れない） |
| `field_column_maps_known_fields` | title/content/url → 正しいカラム名 |
| `field_column_rejects_unknown_field` | 注入文字列を含む未知 field を 400 で弾く（カラム名連結の安全性） |
| `validate_rejects_empty_pattern` | 空白のみパターンを拒否（全件一致事故の防止） |
| `validate_rejects_regex_in_v1` | `match_type='regex'` を v1 で拒否 |
| `validate_rejects_unknown_action` | 未知 action を拒否 |
| `validate_accepts_well_formed_rule` | 正常系（url+contains+mark_read）を許可 |

実行: `cd backend && cargo test mute_rules`（DB 不要）。`just lint`（clippy `-D warnings` + tsc）も通す。

### 9.2 結合テスト（`scripts/test/api-mute-rules.sh`、新規・**実挙動を assert**）

`api-feed-overview.sh` を雛形に、psql で決定論シード → ルール作成/apply → `GET /api/articles` の**実際の絞り込み結果**を assert する。内部 DB へは `docker compose exec -T db psql` で到達する（compose の DB はホスト非公開のため）。

シード（決定論）:

- **feed M**（`id = 00000000-0000-0000-0000-0000000000cc`）に記事3本:
  - m1: `title='Sponsored: 新商品'`, `url='https://news.test/m1'`, `is_read=false`
  - m2: `title='今日の天気'`, `url='https://ad.example.com/m2'`, `is_read=false`
  - m3: `title='通常記事'`, `url='https://news.test/m3'`, `is_read=false`

検証フロー:

1. ルール作成 `POST /api/mute-rules {field:"title", pattern:"Sponsored", action:"hide"}` → `201`。
2. ルール作成 `POST /api/mute-rules {field:"url", pattern:"ad.example.com", action:"mark_read"}` → `201`。
3. `POST /api/mute-rules/apply` → `200`、`hidden>=1`・`marked_read>=1`。
4. `GET /api/articles?feed_id=<M>` → m1 は**含まれない**（hide）、m2 は**含まれる**が `is_read=true`、m3 は含まれ `is_read=false`。
5. `GET /api/articles?feed_id=<M>&include_muted=true` → m1 も**含まれる**（プレビュー）。
6. `GET /api/articles?feed_id=<M>&unread=true` → m1（hide）と m2（既読化済み）は**除外**、m3 のみ。

```bash
#!/usr/bin/env bash
# Integration test for mute_rules: seeds articles, creates rules, applies, and
# asserts the ACTUAL filtering behavior of GET /api/articles.
# Requires: running stack (nginx :8081), docker compose, jq.
set -uo pipefail

BASE="${BASE:-http://localhost:8081}"
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rssreader}"
PGDB="${POSTGRES_DB:-rssreader}"
M="00000000-0000-0000-0000-0000000000cc"

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() {
  psql -q -c "DELETE FROM feeds WHERE id='$M';" >/dev/null 2>&1 || true
  psql -q -c "DELETE FROM mute_rules WHERE pattern IN ('Sponsored','ad.example.com');" >/dev/null 2>&1 || true
}
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

# --- seed ---
psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id='$M';
INSERT INTO feeds (id, url, title) VALUES ('$M', 'https://news.test/feed.xml', 'feed M');
INSERT INTO articles (id, feed_id, url, title, is_read) VALUES
  (gen_random_uuid(), '$M', 'https://news.test/m1',     'Sponsored: 新商品', false),
  (gen_random_uuid(), '$M', 'https://ad.example.com/m2','今日の天気',        false),
  (gen_random_uuid(), '$M', 'https://news.test/m3',     '通常記事',          false);
SQL

# --- create rules ---
curl -s -m 5 -X POST "$BASE/api/mute-rules" -H 'Content-Type: application/json' \
  -d '{"field":"title","pattern":"Sponsored","action":"hide"}' | jq -e '.id' >/dev/null \
  || fail "create hide rule"
curl -s -m 5 -X POST "$BASE/api/mute-rules" -H 'Content-Type: application/json' \
  -d '{"field":"url","pattern":"ad.example.com","action":"mark_read"}' | jq -e '.id' >/dev/null \
  || fail "create mark_read rule"

# --- apply ---
curl -s -m 5 -X POST "$BASE/api/mute-rules/apply" \
  | jq -e '.hidden >= 1 and .marked_read >= 1' >/dev/null || fail "apply report"

# --- default list: m1 hidden, m2 present+read, m3 present+unread ---
list="$(curl -s -m 5 "$BASE/api/articles?feed_id=$M")"
echo "$list" | jq -e 'map(.title) | (index("Sponsored: 新商品") | not)' >/dev/null \
  || fail "m1 should be hidden"
echo "$list" | jq -e 'any(.[]; .url=="https://ad.example.com/m2" and .is_read==true)' >/dev/null \
  || fail "m2 should be present and read"
echo "$list" | jq -e 'any(.[]; .url=="https://news.test/m3" and .is_read==false)' >/dev/null \
  || fail "m3 should be present and unread"

# --- include_muted=true: m1 reappears ---
curl -s -m 5 "$BASE/api/articles?feed_id=$M&include_muted=true" \
  | jq -e 'any(.[]; .title=="Sponsored: 新商品")' >/dev/null \
  || fail "m1 should reappear with include_muted=true"

# --- unread filter: only m3 ---
curl -s -m 5 "$BASE/api/articles?feed_id=$M&unread=true" \
  | jq -e 'map(.url) == ["https://news.test/m3"]' >/dev/null \
  || fail "unread should be only m3"

echo "PASS: mute_rules hide/mark_read filtering"
```

- **Red**: 実装前は `/api/mute-rules` が 404 → 「create hide rule」で落ちる。実装後 Green。
- 環境変数 `BASE`/`DB_SVC`/`POSTGRES_USER`/`POSTGRES_DB` で上書き可。`jq` 必須。

### 9.3 フロント（手動 / 型）

- `tsc`（`just lint` の `pnpm typecheck`）で `MuteRule`/`MuteApplyReport` 型・api メソッド・`MuteRulesManager.tsx` の整合を確認。
- 手動: `Settings` でルール追加（title contains "広告" / url contains "example.com"）→ 記事一覧から該当が消える / 既読になることを目視。トグル無効化で再表示、削除で再表示を確認。

## 10. 実装手順（順序付きチェックリスト）

1. ブランチを切る（例 `feat/mute-filters`）。`main` 直コミットしない。
2. **`ls backend/migrations/` で最新番号を確認**し、`0006_mute_rules.sql`（空いていなければ次の空き番号）を §4 の完全文で追加。既存ファイルは編集しない。
3. `backend/src/features/mute_rules/` を作成し5ファイルを置く:
   - `domain.rs`（§5.2。**まず `#[cfg(test)] mod tests` を書いて Red**、`escape_like`/`field_column`/`validate`/各 struct）。
   - `repository.rs`（§5.3。`clear_all_hidden` は説明用の冗長行を除き、`apply_rule` のカラム名は whitelist 由来）。
   - `service.rs`（§5.4）。
   - `handler.rs`（§5.5）。
   - `mod.rs`（§5.6 の `routes()`）。
4. `cd backend && cargo test mute_rules` で単体テストを Green。
5. `backend/src/features/mod.rs` に `pub mod mute_rules;` と `.merge(mute_rules::routes())` を1行ずつ追加（§5.6）。
6. `articles` 一覧の hide ガードを最小追記（§5.6 の4点: `Article.muted_at` フィールド、`repository::list` の `$5` 条件＋bind、`service::list_articles` の `include_muted` 引数、`handler::ListQuery` の `include_muted`）。`articles` の他機能は触らない。
7. `shared/scheduler.rs` の refresh 直後に `mute_rules::service::apply_all(&state)` を1行追加（§5.7）。
8. `cargo build` → `just lint`（clippy `-D warnings` + tsc）→ `cargo fmt`。
9. スタック起動（`just up`、または `just dev-db` + `just back`）。`scripts/test/api-mute-rules.sh` を追加（§9.2）、実行 → Green を確認。手で `curl` でも挙動を見る。
10. フロント: `lib/api.ts` に `MuteRule`/`MuteApplyReport` 型と CRUD/apply メソッド追加（§6.1）。`components/mute/MuteRulesManager.tsx` 新設（§6.2）。`routes/Settings.tsx` に import + `<MuteRulesManager />` を1箇所マウント（§6.3）。
11. `just lint`（tsc）を通し、追加→消える→トグルで再表示→削除で再表示を目視確認。
12. ユーザーが望むタイミングでコミット（メッセージ末尾に `Co-Authored-By` 行）。新規マイグレーション番号が衝突していないことを最終確認。

## 11. リスク・未決事項・代替案

| 項目 | 内容 / リスク | 対処 |
|------|----------------|------|
| **マイグレーション番号衝突** | apalis 移行・他チケットが先に 0006 を取得しうる | 着手直前に `ls backend/migrations/` を確認し空き番号へ採番。既存は不編集（鉄則） |
| **既存スライス（articles）への変更** | hide 反映で `articles` の `list`/`Article`/`service`/`handler` に最小追記が必要＝完全な「新スライスのみ」ではない | folder filter が既に `feeds` を subquery する確立済みパターンに1条件足すだけ。`muted_at` は同一 articles 集約の追記カラム。CLAUDE.md「逸脱時は理由明記」に従い §5.6 に根拠を記載 |
| **`mark_read` は取り消せない** | ルール削除/無効化しても既読は戻らない（手動既読と区別不能） | 仕様として明記（§2 非スコープ）。確実に戻したいユーザーは hide を使う。#28 で「ミュート起因の既読」を別管理する案 |
| **動的 SQL のカラム名埋め込み** | `apply_rule` が `format!` でカラム名を連結 | 値は `field_column` の固定 whitelist のみ（ユーザー文字列を連結しない）。DB CHECK でも `field` 許可値を二重保証。`field_column_rejects_unknown_field` でテスト |
| **LIKE ワイルドカード誤マッチ** | パターン中の `%`/`_` が任意一致になり過剰ミュート | `escape_like` + `ESCAPE '\'` でリテラル化。単体テストで担保 |
| **空/広すぎるパターン** | 空白のみや極短語（"a"）で全件ミュート事故 | 空白のみは DB CHECK + `validate` で 400。極短語は許容するが UI で注意喚起（将来: 最小長バリデーション） |
| **apply の性能** | `apply_all` は hide 全リセット→全有効ルールで `articles` 全体を ILIKE スキャン。trgm GIN（0005）は前方一致でなく `ILIKE '%x%'` には部分的にしか効かない | 家庭内・単一ユーザ規模では問題なし。記事数増大時は (a) ルール変更時のみ apply、(b) 新着のみ評価する差分 apply、(c) trgm を活かす式に最適化、を将来検討 |
| **正規表現（regex）未対応** | v1 は contains のみ。`match_type` 列は用意済み | #28 で `regex` クレート追加＋ReDoS 対策（タイムアウト/長さ制限）をして拡張。`validate` が現状 regex を 400 で弾く |
| **著者（author）フィールド未対応** | `articles` に author 列が無い。intent の「著者」は未充足 | 代替: `url`（ドメイン部分一致）で「サイト単位ミュート」を提供。将来 author を足すには 新マイグレーションで `articles.author` 追加＋`feeds/service::fetch_and_store` で `entry.authors` を `upsert` に渡す改修（越境のため別チケット） |
| **url 部分一致＝厳密なドメイン一致ではない** | "example.com" が URL のパス中にも一致しうる | 家庭内規模では許容。厳密化は将来 host 抽出（`split_part`/正規表現）か専用 domain 列で対応 |
| **クロール自動適用の遅延** | 新着は次のクロール周期の `apply_all` まで未ミュート | 即時性が要れば一覧表示前にフロントが `applyMuteRules()` を呼ぶ。または `feeds` の新着 upsert 直後に差分適用する最適化（将来） |
| **同時更新の競合** | 複数タブで apply が並走すると hide リセット中に瞬間的に全表示されうる | 単一ユーザ前提で許容。厳密化はトランザクション化（`clear`+再付与を1 tx に）で対応可 |
| **AI 拡張の境界** | NGワード AI 提案は本チケットに含めない | §5.8 に置き場所のみ記述。#28 で `shared/llm` 再利用 + DB キャッシュ + `ANTHROPIC_API_KEY` 未設定時 `AppError::NotEnabled` を踏襲 |
