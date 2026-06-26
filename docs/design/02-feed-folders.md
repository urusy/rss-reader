# 02 フィードのフォルダ分け

> 読み手: このリポジトリは持っているが本会話の文脈を知らない、別セッションの実装者。曖昧さを残さず、このファイルだけで実装に着手できる粒度で書く。
> 関連土台: `docs/design/00-foundation-backend.md` / `docs/design/00-foundation-frontend.md`。本書はそれらの「0002 マイグレーション割り当て」「新 `folders` スライス」「`feeds` PATCH 拡張」「未分類=`folder_id IS NULL`」の方針に従う。
> 既に最終化済みの隣接設計: `docs/design/10-two-pane-layout.md`（二ペインシェル・`useSelection`/`scopeFromPath`・`/folders/:folderId` ルート枠）、`docs/design/11-unread-filter-toggle.md`（`?unread=` トグル）。本書はこれらと**矛盾しないように**書いてある（§6.3 参照）。

---

## 1. 概要

フィードをユーザー定義の **フォルダ（カテゴリ）** で整理できるようにする。左ペインに「フォルダ → 配下フィード」のツリーを表示し、どのフォルダにも属さないフィードは末尾の **「未分類」** 仮想グループにまとめる。これにより購読数が増えても見通しが保て、フォルダ単位で記事一覧を絞り込める。単一ユーザ・家庭内 LAN 前提なので共有設定は不要。

実現手段は3つ:
1. 新マイグレーション `0002_folders.sql` … `folders` テーブル新設 + `feeds.folder_id`（nullable FK, `ON DELETE SET NULL`）追加。
2. 新スライス `folders` … フォルダの CRUD（5ファイル構成）。
3. 既存スライスの最小拡張 … `feeds` に `PATCH /api/feeds/{id}`（フォルダ割当 + リネーム）、`articles::list` に `folder_id` / `unclassified` 絞り込みを追加。

---

## 2. スコープ / 非スコープ

### 含む（このフィーチャで実装する）
- `folders` テーブル + `feeds.folder_id` のマイグレーション（`0002_folders.sql`）。
- `folders` CRUD スライス（一覧 / 作成 / 改名 / 削除）。
- フィードのフォルダ割当・解除・リネーム API（`PATCH /api/feeds/{id}`、`folder_id` を値/`null`、`title` で設定）。**本書がこのエンドポイント・ハンドラ・`UpdateFeed` 構造体・`repository::update` の唯一の定義所有者**（§5.2 / §8 / §11）。
- 記事一覧のフォルダ絞り込み（`GET /api/articles?folder_id=` と `?unclassified=true`）。
- フロント: 左ペインの「フォルダ→フィード」ツリー、未分類グループ、フォルダ作成/改名/削除 UI、フィードのフォルダ移動 UI（select）。
- 未分類（`folder_id IS NULL`）の定義と全経路での扱い。

### 含まない（他フィーチャ or 将来）
- フィード別/フォルダ別の **未読数バッジ**（feature 03 `feed_overview` / feature 09。ツリーはバッジ差し込み口だけ用意）。
- 二ペインのアプリシェルそのもの（feature 10 `two-pane-layout`。本書はその Sidebar にツリーを載せる前提。10 未着手時は現シェルへ仮置きしても API は不変）。
- フォルダの **並べ替え（ドラッグ&ドロップ / position 編集）**（`position` 列は用意するが編集 UI は将来）。
- フォルダの入れ子（ネスト）。本書はフラット1階層のみ。
- フィードのリネーム **UI** 自体（feature 01 `feed-management`）。ただしリネームの **API（`PATCH /api/feeds/{id}` の `title`）は本書が定義し、01 は再利用するだけ**（再定義しない）。

---

## 3. 既存実装の調査と再利用

実ファイルを読んで確認した、再利用すべき資産（車輪の再発明を避ける根拠）:

| 資産 | 場所（確認済み） | どう使うか |
|------|------|-----------|
| スライス雛形（domain/repository/service/handler/mod）| `backend/src/features/feeds/*`, `.../stats/*` | `folders` スライスはこの5ファイル構成をそのまま踏襲 |
| newtype + 値オブジェクト | `feeds/domain.rs`（`FeedId` = `#[sqlx(transparent)]`、`FeedUrl::parse -> Result<_, String>`）| `FolderId` / `FolderName::parse` を同型で作る |
| クロススライスの ID import | `articles/domain.rs` / `articles/repository.rs` が `use crate::features::feeds::domain::FeedId;` | 逆向きに `feeds` と `articles` が `crate::features::folders::domain::FolderId` を import（祝福済みパターン。§5.1 末尾の循環依存メモ参照） |
| `AppError`（6バリアント、`shared/error.rs`）| `NotFound`(404) / `Validation(String)`(400) を使用。**新バリアントは足さない** | 行なし→`NotFound`、名前不正/割当先不在→`Validation`。`Validation` の応答ボディは `{"error":"invalid input: {msg}"}`（`error.rs` の `#[error("invalid input: {0}")]` 実測） |
| ランタイムクエリ規約 | `feeds/repository.rs` の `sqlx::query_as::<_, T>(SQL).bind(..)` | `query!` マクロは使わない。`fetch_optional` → `None` → `AppError::NotFound` |
| マイグレーション実行 | `shared/db.rs` の `run_migrations` = `sqlx::migrate!("./migrations")`（起動時）。最新は `0001_init.sql` | 次番号 `0002_*.sql` を **追記**（既存は編集しない）。`just migrate`（`sqlx migrate run`）でも適用 |
| `articles::repository::list` の既存フィルタ | `feed_id: Option<FeedId>` + `unread_only: bool` を1クエリで `($1::uuid IS NULL OR ...)` 形式（実測） | この形式に `folder_id` / `unclassified` の2条件を **追記**（破壊なし） |
| `feeds` 既存ルート | `feeds/mod.rs` の `.route("/api/feeds/{id}", axum::routing::delete(handler::delete))`（実測） | 同じ行に `.patch(handler::update)` をチェーン |
| feeds insert の ON CONFLICT | `feeds/repository.rs` `insert` は `RETURNING id, url, title, created_at, last_fetched_at`（実測） | RETURNING 列に `folder_id` を追加（§5.2-2。新規行は `folder_id = NULL`） |
| stats の集約 read 前例 | `stats` スライス（他テーブルを SELECT する読み取りスライスは許容） | `feeds` が `folders` を存在チェック SELECT するのは越境共通レイヤーではない（同方針） |
| フロント API クライアント | `frontend/src/lib/api.ts`（`http<T>()` が 204→`undefined` を畳む、`URLSearchParams` 利用、実測） | `Folder` 型と各メソッドを追記 |
| Ark UI ラップ前例 | `frontend/src/components/ui/dialog.tsx`（Portal + トークン装飾、part 名は ark-ui.com 確認の注記つき） | `select.tsx` を同手法で追加。ツリー v1 は自前折りたたみ |
| デザイントークン | `frontend/src/app.css`（`bg-accent` / `text-muted-foreground` / `border-border`、`.dark` 配線済み） | ツリーの選択/ホバー/罫線に流用、生 hex を持ち込まない |
| 統合テスト前例 | `scripts/test/api-stats.sh`（稼働中スタック nginx:8081 へ curl、HTTP コード + JSON キー検証、`set -uo pipefail`、実測） | `scripts/test/api-folders.sh` を**機能拡張して**追加（§9。GET 単発の stats と違いステートフルなので jq で ID 抽出 + シードと後始末を足す） |
| 単体テスト前例 | `feeds/domain.rs` の `#[cfg(test)] mod tests`（`FeedUrl::parse` の網羅、実測） | `FolderName::parse` を同様に網羅 + `UpdateFeed` の serde 三値判別 |

> **重要な前提の訂正（土台ドキュメントの不正確さ）**: `docs/design/00-foundation-backend.md` §5 テスト節は「`backend/tests/` の統合テスト（`stats` スライスの統合テスト前例）」に倣えと書くが、**実際には `backend/tests/` ディレクトリも `stats` の Rust 統合テストも存在しない**。リポジトリに実在する結合テスト前例は `scripts/test/*.sh`（稼働スタックへの curl）だけである（`scripts/test/api-stats.sh` / `just-resolves-pnpm.sh` / `pnpm-version-pinned.sh` を確認）。したがって本書は**実在前例＝シェルスクリプトを第一**とし、Rust ハーネス（`backend/tests/`）導入は任意の代替として §9 に記す。隣接スライスの実装者も「Rust ハーネスが既にある」と誤解しないこと。

---

## 4. データモデルとマイグレーション

新規ファイル: **`backend/migrations/0002_folders.sql`**（土台のマイグレーション割り当てで 0002 は本機能に予約済み。`0001_init.sql` は編集しない・追記のみ）。

```sql
-- 0002_folders.sql
-- Folders: user-defined categories for organizing feeds. Flat (no nesting) for now.
CREATE TABLE IF NOT EXISTS folders (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL,
    position    INTEGER NOT NULL DEFAULT 0,   -- display order; editing UI is future scope
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A feed belongs to at most one folder. NULL = "未分類" (unclassified).
-- The FK is the real integrity guard for folder assignment.
-- ON DELETE SET NULL: deleting a folder moves its feeds back to unclassified
-- (never deletes feeds/articles). This is what makes "未分類" robust.
ALTER TABLE feeds
    ADD COLUMN IF NOT EXISTS folder_id UUID REFERENCES folders(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_feeds_folder_id ON feeds(folder_id);
```

設計判断:
- **未分類 = `feeds.folder_id IS NULL`**。専用の DB 行は作らない。フロントが仮想グループとして末尾固定で描画する。
- **`ON DELETE SET NULL`**: フォルダ削除は配下フィードを未分類へ戻すだけ（記事は無関係＝`articles.feed_id` の `ON DELETE CASCADE` には触れない）。
- **FK `feeds.folder_id REFERENCES folders(id)` がフォルダ割当の唯一の整合性保証**。実在しないフォルダへの割当は DB レベルで 23503（外部キー違反）になる。§5.2 の `folder_exists` 事前チェックは**この 500 を 400 に整形するためだけの advisory**（FK が本命のガード。単一ユーザなので「チェック後・UPDATE 前にフォルダが消える」TOCTOU は実害なし）。
- `position` 列は将来の並べ替え用に用意のみ。挿入時に `SELECT COALESCE(MAX(position),0)+1` で採番する（§5.1）。この採番は**並行挿入に対して厳密でない**が、単一ユーザ前提で実害なし。並べ替え UI は非スコープ。
- `name` に UNIQUE 制約は付けない（単一ユーザ・重複名は実害小）。将来付けるなら衝突を `Validation` にマップする（§11）。

---

## 5. バックエンド設計

> **コンパイルに必要な `use` 追加を各ファイルごとに明記する**（別セッションでコピペしてもビルドが通るように）。`?` 伝播・`FromRow`・newtype は既存スライスと同型。

### 5.1 新スライス `folders`（5ファイル・新規）

ディレクトリ: `backend/src/features/folders/{domain,repository,service,handler,mod}.rs`

#### `domain.rs`（新規・全文）
```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// フォルダ主キーの newtype（FeedId / ArticleId と取り違えない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct FolderId(pub Uuid);

/// 検証済みフォルダ名の値オブジェクト。空白のみ・長すぎる名前を構築時に弾く。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderName(String);

impl FolderName {
    pub const MAX_CHARS: usize = 100;

    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let trimmed = raw.into().trim().to_string();
        if trimmed.is_empty() {
            return Err("folder name must not be empty".to_string());
        }
        if trimmed.chars().count() > Self::MAX_CHARS {
            return Err(format!(
                "folder name must be at most {} chars",
                Self::MAX_CHARS
            ));
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 永続化されたフォルダ（folders テーブルをミラー）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    pub position: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_normal_name() {
        let n = FolderName::parse("Tech").unwrap();
        assert_eq!(n.as_str(), "Tech");
    }

    #[test]
    fn parse_trims_whitespace() {
        let n = FolderName::parse("  Tech  ").unwrap();
        assert_eq!(n.as_str(), "Tech");
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(FolderName::parse("").is_err());
    }

    #[test]
    fn parse_rejects_whitespace_only() {
        assert!(FolderName::parse("   ").is_err());
    }

    #[test]
    fn parse_accepts_boundary_100_and_rejects_101() {
        let ok = "a".repeat(FolderName::MAX_CHARS);
        assert!(FolderName::parse(ok).is_ok());
        let too_long = "a".repeat(FolderName::MAX_CHARS + 1);
        assert!(FolderName::parse(too_long).is_err());
    }
}
```

#### `repository.rs`（新規。自由関数・`&PgPool`・ランタイムクエリ）
必要な `use`:
```rust
use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{Folder, FolderId};
use crate::shared::error::{AppError, AppResult};
```
関数（SQL は実行時クエリ。`query!` 不可）:
```rust
/// position は MAX+1 で採番（並行挿入の厳密性は単一ユーザ前提で不問）。
pub async fn insert(pool: &PgPool, name: &str) -> AppResult<Folder> {
    let row = sqlx::query_as::<_, Folder>(
        r#"INSERT INTO folders (id, name, position)
           VALUES ($1, $2, (SELECT COALESCE(MAX(position), 0) + 1 FROM folders))
           RETURNING id, name, position, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn list_all(pool: &PgPool) -> AppResult<Vec<Folder>> {
    let rows = sqlx::query_as::<_, Folder>(
        r#"SELECT id, name, position, created_at
           FROM folders ORDER BY position, created_at"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update_name(pool: &PgPool, id: FolderId, name: &str) -> AppResult<Folder> {
    sqlx::query_as::<_, Folder>(
        r#"UPDATE folders SET name = $2 WHERE id = $1
           RETURNING id, name, position, created_at"#,
    )
    .bind(id.0)
    .bind(name)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn delete(pool: &PgPool, id: FolderId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}
```

#### `service.rs`（新規。自由関数・`&AppState`）
必要な `use`:
```rust
use super::domain::{Folder, FolderId, FolderName};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;
```
関数:
```rust
pub async fn create_folder(state: &AppState, raw_name: &str) -> AppResult<Folder> {
    let name = FolderName::parse(raw_name).map_err(AppError::Validation)?;
    repository::insert(&state.db, name.as_str()).await
}

pub async fn list_folders(state: &AppState) -> AppResult<Vec<Folder>> {
    repository::list_all(&state.db).await
}

pub async fn rename_folder(state: &AppState, id: FolderId, raw_name: &str) -> AppResult<Folder> {
    let name = FolderName::parse(raw_name).map_err(AppError::Validation)?;
    repository::update_name(&state.db, id, name.as_str()).await // None -> NotFound は repo 側
}

pub async fn delete_folder(state: &AppState, id: FolderId) -> AppResult<()> {
    if repository::delete(&state.db, id).await? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(()) // 配下フィードは ON DELETE SET NULL で未分類へ
}
```

#### `handler.rs`（新規。axum）
必要な `use`:
```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{Folder, FolderId};
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;
```
本体:
```rust
#[derive(Debug, Deserialize)]
pub struct CreateFolder {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFolder {
    pub name: String,
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Folder>>> {
    Ok(Json(service::list_folders(&state).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateFolder>,
) -> AppResult<(StatusCode, Json<Folder>)> {
    let folder = service::create_folder(&state, &body.name).await?;
    Ok((StatusCode::CREATED, Json(folder))) // 201（feeds::create 前例）
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFolder>,
) -> AppResult<Json<Folder>> {
    let folder = service::rename_folder(&state, FolderId(id), &body.name).await?;
    Ok(Json(folder)) // 更新後エンティティを返す
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::delete_folder(&state, FolderId(id)).await?;
    Ok(StatusCode::NO_CONTENT) // 204
}
```

#### `mod.rs`（新規・全文）
```rust
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::get;
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/folders", get(handler::list).post(handler::create))
        .route(
            "/api/folders/{id}",
            axum::routing::patch(handler::update).delete(handler::delete),
        )
}
```

#### `features/mod.rs`（既存に **追加2行のみ**）
```rust
pub mod folders;                 // ← 既存 pub mod 群に追加
// router() 内のチェーンに追加:
        .merge(folders::routes())
```

> **コンパイル依存の向き**: `folders` は他スライスに依存しない（最も下層）。`feeds` と `articles` が `folders::domain::FolderId` を import する（§5.2 / §5.3）。これは既存の `articles → feeds`（`articles` が `feeds::domain::FeedId` を使う）と同じ向きの一方向依存で、循環しない。Rust は module 間の循環 use を許すが、本設計では一方向で閉じている。

### 5.2 既存スライス拡張 ①: `feeds`（フォルダ割当 / リネーム）

**正当化**: `folder_id` と `title` は `feeds` テーブルの列＝Feed アグリゲートの書き込み。別スライスから `feeds` を UPDATE するのは越境書き込みで悪化する。よって割当・リネームは feeds 内 PATCH に閉じる（土台設計の判断に一致）。

> **所有権（blocking issue 解消）**: `PATCH /api/feeds/{id}` エンドポイント・`handler::update`・`UpdateFeed` 構造体・`double_option` ヘルパ・`repository::update` は **本書（#02）が唯一の定義所有者**。feature 01 `feed-management` は**リネーム UI からこのエンドポイントを再利用するだけで、ルート/ハンドラ/構造体/リポジトリ関数を再定義しない**（重複定義のコンパイルエラー回避）。`docs/design/00-foundation-backend.md` の §2.2 表と §6 マトリクスも本書を所有者と明記するよう更新済み。確定契約は `{ title?, folder_id? }`、キー無し=据え置き / `folder_id: null`=未分類化 / 値=割当。

変更点（すべて feeds スライス内に閉じる）:

**1. `feeds/domain.rs`** — `Feed` に `folder_id` を追加し `FolderId` を import。
追加 `use`（既存 `use serde::{Deserialize, Serialize};` / `use uuid::Uuid;` に加えて）:
```rust
use crate::features::folders::domain::FolderId;
```
`struct Feed` にフィールドを1つ追加（FromRow 列順は SQL の RETURNING/SELECT と一致させる）:
```rust
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Feed {
    pub id: FeedId,
    pub url: String,
    pub title: Option<String>,
    pub folder_id: Option<FolderId>, // ← 追加
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

**2. `feeds/repository.rs`** — 既存 import に `AppError` と `FolderId` を足し、Feed を返す全クエリの列に `folder_id` を加え、`update` / `folder_exists` を追加。
import 変更（現状は `use crate::shared::error::AppResult;` のみ）:
```rust
use crate::shared::error::{AppError, AppResult};   // AppError を追加（update が NotFound を返す）
use crate::features::folders::domain::FolderId;     // 新規
// 既存の use sqlx::PgPool; use uuid::Uuid; use super::domain::{Feed, FeedId}; はそのまま
```
**Feed を生成する3クエリすべてに `folder_id` 列を含める**（`FromRow` を満たすため）:
```rust
// (a) insert の ON CONFLICT ... RETURNING に folder_id を追加（新規行は NULL）:
//   ... RETURNING id, url, title, folder_id, created_at, last_fetched_at
// (b) list_all の SELECT に folder_id を追加:
//   SELECT id, url, title, folder_id, created_at, last_fetched_at
//   FROM feeds ORDER BY created_at DESC
```
追加関数:
```rust
/// PATCH: title / folder_id をそれぞれ「触る/触らない」で部分更新する。
/// folder_id の三値: 外側 None=未指定(据え置き) / Some(None)=未分類化(NULL) / Some(Some(x))=割当。
pub async fn update(
    pool: &PgPool,
    id: FeedId,
    title: Option<&str>,
    folder_id: Option<Option<FolderId>>,
) -> AppResult<Feed> {
    let touch_folder = folder_id.is_some();
    let folder_val: Option<Uuid> = folder_id.flatten().map(|f| f.0);
    sqlx::query_as::<_, Feed>(
        r#"UPDATE feeds
           SET title     = CASE WHEN $2 THEN $3 ELSE title     END,
               folder_id = CASE WHEN $4 THEN $5 ELSE folder_id END
           WHERE id = $1
           RETURNING id, url, title, folder_id, created_at, last_fetched_at"#,
    )
    .bind(id.0)            // $1 :: uuid（WHERE id = $1）
    .bind(title.is_some()) // $2 :: bool
    .bind(title)           // $3 :: text（CASE 結果型が title 列=TEXT から解決）
    .bind(touch_folder)    // $4 :: bool
    .bind(folder_val)      // $5 :: uuid（CASE 結果型が folder_id 列=UUID から解決）
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// 割当先フォルダの存在チェック（advisory）。FK が本命のガードで、
/// これは 23503(FK 違反=500) を Validation(400) に整形するためだけのもの。
pub async fn folder_exists(pool: &PgPool, id: FolderId) -> AppResult<bool> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM folders WHERE id = $1)")
            .bind(id.0)
            .fetch_one(pool)
            .await?;
    Ok(exists)
}
```
> **sqlx プリペアド文の型解決メモ**: 上記 `CASE WHEN $2 THEN $3 ELSE title END` で `$3` の型は CASE の結果型（`title` 列 = `TEXT`）から、`$5` は `folder_id` 列 = `UUID` から一意に解決される。`$2`/`$4` は `WHEN` の真偽コンテキストで `bool`、`$1` は `WHERE id =` で `uuid`。すべてのバインド型がクエリから決まるので、PostgreSQL のパラメータ型推論で破綻しない（`query` ランタイム経路。`query!` マクロ不使用）。`title.is_some()` が false のとき `$3` は NULL バインドだが ELSE 枝（`title`）が選ばれるため評価されない。

**3. `feeds/service.rs`** — `update_feed` を追加。
追加 `use`（既存 `use crate::shared::error::{AppError, AppResult};` はそのまま。`Feed`/`FeedId` も既存 import 済み）:
```rust
use crate::features::folders::domain::FolderId;
```
関数:
```rust
pub async fn update_feed(
    state: &AppState,
    id: FeedId,
    title: Option<String>,
    folder_id: Option<Option<FolderId>>,
) -> AppResult<Feed> {
    if let Some(t) = &title {
        if t.trim().is_empty() {
            return Err(AppError::Validation("title must not be empty".into()));
        }
    }
    // 実在しないフォルダへの割当は 400 に整形（FK 違反の 500 を避ける advisory）。
    if let Some(Some(fid)) = folder_id {
        if !repository::folder_exists(&state.db, fid).await? {
            return Err(AppError::Validation("folder not found".into()));
        }
    }
    repository::update(&state.db, id, title.as_deref(), folder_id).await
}
```

**4. `feeds/handler.rs`** — `UpdateFeed` と `double_option` と `update` を追加。`folder_id` の「未指定 / null / 値」三値は serde の double-option で判別。
追加 `use`（既存 `use super::domain::{Feed, FeedId};` / `use serde::Deserialize;` / `use uuid::Uuid;` / `use crate::shared::error::AppResult;` に加えて）:
```rust
use crate::features::folders::domain::FolderId;
```
本体:
```rust
#[derive(Debug, Deserialize)]
pub struct UpdateFeed {
    #[serde(default)]
    pub title: Option<String>,
    // 外側 None=キー無し(据え置き) / Some(None)=明示 null(未分類化) / Some(Some)=割当
    #[serde(default, deserialize_with = "double_option")]
    pub folder_id: Option<Option<Uuid>>,
}

// "キー無し" と "null" を区別するためのヘルパ（serde_with の double_option 相当）。
// キーが存在すれば（null でも値でも）呼ばれ、内側 Option を Some で包む。
// キーが無ければ #[serde(default)] が None を与え、本関数は呼ばれない。
fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(de).map(Some)
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFeed>,
) -> AppResult<Json<Feed>> {
    let folder_id = body.folder_id.map(|inner| inner.map(FolderId));
    let feed = service::update_feed(&state, FeedId(id), body.title, folder_id).await?;
    Ok(Json(feed))
}
```

**5. `feeds/mod.rs`** — `{id}` ルートに `.patch` をチェーン（1行変更）。`use axum::routing::{get, post};` に `patch` は不要（フル修飾で書く）:
```rust
.route(
    "/api/feeds/{id}",
    axum::routing::delete(handler::delete).patch(handler::update),
)
```

### 5.3 既存スライス拡張 ②: `articles`（フォルダ絞り込み）

**正当化**: 記事一覧の絞り込みは articles の読み取り責務。既存の `feed_id`/`unread_only` と同じ1クエリに条件を**追記**するだけ（破壊なし・後方互換）。フォルダ選択時に「そのフォルダ配下フィードの記事」を返すために必須。

> **canonical な最終シグネチャ（merge-churn 抑止）**: `articles` は本書(#02: folder 絞り)のほか #09(read-all は**別エンドポイント** `POST /api/articles/read-all` なので `list` は触らない)・#11(既存 `?unread=` を使うだけで**バックエンド変更なし**。`docs/design/11-unread-filter-toggle.md` §3 で確認済み)が関与する。**`list` シグネチャを変えるのは #02 だけ**だが、将来のパラメータ追加が衝突しないよう、確定形を以下に固定する。引数は**末尾追記方向**で積み、既存呼び出し（テスト・ハンドラ）はキーワード位置で更新する。

確定シグネチャ:
```rust
// repository::list
pub async fn list(
    pool: &PgPool,
    feed_id: Option<FeedId>,
    unread_only: bool,
    folder_id: Option<FolderId>,   // 追加
    unclassified: bool,            // 追加: folder_id IS NULL のフィード群
) -> AppResult<Vec<Article>>

// service::list_articles
pub async fn list_articles(
    state: &AppState,
    feed_id: Option<FeedId>,
    unread_only: bool,
    folder_id: Option<FolderId>,   // 追加
    unclassified: bool,            // 追加
) -> AppResult<Vec<Article>>
```

**1. `articles/repository.rs`**
追加 `use`（既存 `use crate::features::feeds::domain::FeedId;` / `use crate::shared::error::{AppError, AppResult};` に加えて）:
```rust
use crate::features::folders::domain::FolderId;
```
`list` 本体（既存の `SELECT *` を維持しつつ WHERE に2条件追記。`Article` 構造体は不変）:
```rust
pub async fn list(
    pool: &PgPool,
    feed_id: Option<FeedId>,
    unread_only: bool,
    folder_id: Option<FolderId>,
    unclassified: bool,
) -> AppResult<Vec<Article>> {
    let rows = sqlx::query_as::<_, Article>(
        r#"SELECT * FROM articles
           WHERE ($1::uuid IS NULL OR feed_id = $1)
             AND ($2 = false OR is_read = false)
             AND ($3::uuid IS NULL
                  OR feed_id IN (SELECT id FROM feeds WHERE folder_id = $3))
             AND ($4 = false
                  OR feed_id IN (SELECT id FROM feeds WHERE folder_id IS NULL))
           ORDER BY published_at DESC NULLS LAST, created_at DESC
           LIMIT 200"#,
    )
    .bind(feed_id.map(|f| f.0))
    .bind(unread_only)
    .bind(folder_id.map(|f| f.0))
    .bind(unclassified)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
```

**2. `articles/service.rs`** — `list_articles` に同2引数を素通し。
追加 `use`（既存 `use crate::features::feeds::domain::FeedId;` に加えて）:
```rust
use crate::features::folders::domain::FolderId;
```
本体:
```rust
pub async fn list_articles(
    state: &AppState,
    feed_id: Option<FeedId>,
    unread_only: bool,
    folder_id: Option<FolderId>,
    unclassified: bool,
) -> AppResult<Vec<Article>> {
    repository::list(&state.db, feed_id, unread_only, folder_id, unclassified).await
}
```

**3. `articles/handler.rs`** — `ListQuery` に2フィールド追加し、`list` ハンドラの呼び出し行を更新。
追加 `use`（既存 `use crate::features::feeds::domain::FeedId;` に加えて）:
```rust
use crate::features::folders::domain::FolderId;
```
`ListQuery`（`#[derive(Debug, Deserialize)]` のまま。`deny_unknown_fields` は付けない＝後方互換）:
```rust
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub feed_id: Option<Uuid>,
    #[serde(default)]
    pub unread: bool,
    pub folder_id: Option<Uuid>,        // 追加
    #[serde(default)]
    pub unclassified: bool,             // 追加
}
```
`list` ハンドラの呼び出し行（現状 `service::list_articles(&state, q.feed_id.map(FeedId), q.unread)` を置換）:
```rust
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Article>>> {
    let articles = service::list_articles(
        &state,
        q.feed_id.map(FeedId),
        q.unread,
        q.folder_id.map(FolderId),
        q.unclassified,
    )
    .await?;
    Ok(Json(articles))
}
```
> `folder_id` と `unclassified` は同時指定しない想定（フロントがどちらか一方を送る）。両方来た場合は両 AND 条件が効き、結果は積集合（実害なし）。`articles::mod.rs` / `articles/domain.rs` は無変更。

### 5.4 AppError の使い分け（`shared/error.rs` は不編集）
- フォルダ/フィード行なし（改名・削除・割当先 PATCH 対象）→ `NotFound`(404)。
- フォルダ名が空白のみ/長すぎ → `Validation`(400)。応答ボディは `{"error":"invalid input: folder name must not be empty"}`（`error.rs` の `#[error("invalid input: {0}")]` 実測）。
- 実在しないフォルダへの割当 → `Validation("folder not found")`(400)。
- title が空白のみ → `Validation("title must not be empty")`(400)。
- DB エラー → `Database`(500、`#[from] sqlx::Error` で自動変換)。**新バリアントは追加しない**。

---

## 6. フロントエンド設計

> 配置先は feature 10 `two-pane-layout` が導入する **Sidebar（左ペイン・永続インスタンス）**。バックエンド（§4/§5）は本機能単独で完結し**先行マージ可**。ツリー UI の最終置き場所と選択導出のみ 10 に依存（§8）。

### 6.1 `lib/api.ts`
`frontend/src/lib/api.ts` に型とメソッドを追記:
```ts
// Feed に1フィールド追加（バックエンドの folder_id をミラー）
export interface Feed {
  id: string;
  url: string;
  title: string | null;
  folder_id: string | null; // ← 追加
  created_at: string;
  last_fetched_at: string | null;
}

// 新型
export interface Folder {
  id: string;
  name: string;
  position: number;
  created_at: string;
}

export const api = {
  // ...既存（listFeeds / addFeed / deleteFeed / listArticles / getArticle /
  //          markRead / summarize / translate）...

  listFolders: () => http<Folder[]>("/api/folders"),
  createFolder: (name: string) =>
    http<Folder>("/api/folders", { method: "POST", body: JSON.stringify({ name }) }),
  updateFolder: (id: string, name: string) =>
    http<Folder>(`/api/folders/${id}`, { method: "PATCH", body: JSON.stringify({ name }) }),
  deleteFolder: (id: string) =>
    http<void>(`/api/folders/${id}`, { method: "DELETE" }),

  /**
   * フィードの部分更新（リネーム / フォルダ割当 / 未分類化）。
   * 注意（double-option セマンティクス）:
   *   - キーを渡さない        => その項目は据え置き（変更しない）
   *   - folder_id: "<uuid>"  => そのフォルダへ割当
   *   - folder_id: null      => 未分類化（割当解除）
   * 「未分類化」したいときは必ず `null` を渡す。`undefined` を渡しても JSON に
   * キーが出ず「据え置き」になってしまうので、undefined で解除を期待しないこと。
   */
  updateFeed: (id: string, patch: { title?: string; folder_id?: string | null }) =>
    http<Feed>(`/api/feeds/${id}`, { method: "PATCH", body: JSON.stringify(patch) }),

  // listArticles に folder_id / unclassified を追加（後方互換。既存呼び出しは無変更で動く）
  listArticles: (params?: {
    feed_id?: string;
    folder_id?: string;
    unclassified?: boolean;
    unread?: boolean;
  }) => {
    const q = new URLSearchParams();
    if (params?.feed_id) q.set("feed_id", params.feed_id);
    if (params?.folder_id) q.set("folder_id", params.folder_id);
    if (params?.unclassified) q.set("unclassified", "true");
    if (params?.unread) q.set("unread", "true");
    const qs = q.toString();
    return http<Article[]>(`/api/articles${qs ? `?${qs}` : ""}`);
  },
};
```
> `JSON.stringify` は値が `undefined` のキーを出力しないので、`updateFeed(id, { title: "x" })` は `{"title":"x"}` になり folder_id は据え置き。`updateFeed(id, { folder_id: null })` は `{"folder_id":null}` になり未分類化（バックエンドの `double_option` が `Some(None)` と解釈）。

### 6.2 コンポーネント

| パス | 役割 | 実装方針 |
|------|------|---------|
| `components/layout/FeedTree.tsx`（新規）| `listFolders()`+`listFeeds()` から「フォルダ→フィード」ツリーを組み、未分類グループを末尾固定で描画。フォルダ見出し→`/folders/:id`、フィード→`/feeds/:feedId`、未分類→`/folders/unclassified`（センチネル, §6.3）へのリンク。フォルダ作成/改名/削除、フィード移動を内包 | **v1 は自前の折りたたみリスト（disclosure）** を推奨（Ark TreeView の API 不確実性を回避）。各フォルダ=展開トグル(`button`)＋子フィードのネストリンク。展開状態は `createSignal`。a11y を厳密化したくなったら `components/ui/tree-view.tsx`(Ark UI TreeView)へ差し替え（差し替えは1ファイルに閉じる） |
| `components/ui/select.tsx`（新規・Ark UI）| フィードの「フォルダへ移動」ピッカー。選択肢 = 全フォルダ + 「未分類」(=`null`)。変更時 `api.updateFeed(feedId, { folder_id })`（解除は `null`） | Ark UI Select を薄くラップ。**part 名（`Select.Root/Control/Trigger/Positioner/Content/Item/ItemText` 等）と `createListCollection` は実装時に ark-ui.com(Solid/Select) で要確認**。`dialog.tsx` と同じくトークン装飾＋Portal |
| `components/ui/dialog.tsx`（既存・再利用）| フォルダ削除の確認、フォルダ作成/改名のフォーム | 既存をそのまま使う（usage はファイル冒頭コメント参照） |
| `components/ui/input.tsx`（任意・新規）| フォルダ名入力（作成/改名） | 自前 cva（土台 §3）。未導入なら素の `<input class="...token...">` でも可 |

FeedTree のデータ整形（擬似コード。CRUD/移動の後は `createResource` を `refetch()`）:
```ts
const grouped = () => {
  const fs = feeds() ?? [];
  const byFolder = new Map<string, Feed[]>(); // folderId -> feeds
  const unclassified: Feed[] = [];
  for (const f of fs) {
    if (f.folder_id) {
      const arr = byFolder.get(f.folder_id) ?? [];
      arr.push(f);
      byFolder.set(f.folder_id, arr);
    } else {
      unclassified.push(f);
    }
  }
  // folders() を position 順に並べ、各フォルダに byFolder の配列をぶら下げる。
  // 最後に擬似グループ { id: "unclassified", name: "未分類", feeds: unclassified } を末尾固定で追加。
};
```
未読数バッジは feature 03/09 が `feed_overview` を載せたとき各ノード右端に差し込む（本機能はプレースホルダのみ。ノード構造とバッジ用スロットだけ用意）。

### 6.3 ルーティング & 選択（URL を正とする・#10 と整合）
土台と #10 の方針どおり「今どこを見ているか」は URL。`docs/design/10-two-pane-layout.md` は既に右ペインルートを `<Route path={["/", "/feeds/:feedId", "/folders/:folderId"]} component={ArticleList} />` と定義し、`scopeFromPath()` が `/folders/:folderId` を `{ kind: "folder", folderId }` に導出する（マウント維持で再フェッチ）。本機能はこの枠に**未分類**と**folder データ取得**を載せる:

- `/`                  … 全記事（scope=all）
- `/feeds/:feedId`     … `listArticles({ feed_id })`（既存対応）
- `/folders/:folderId` … `listArticles({ folder_id })`
- `/folders/unclassified` … **センチネル**。#10 の `scopeFromPath` は `{ kind:"folder", folderId:"unclassified" }` を返すので、`ArticleList` の source 合成で `folderId === "unclassified"` を検出して `listArticles({ unclassified: true })` を呼ぶ（UUID は文字列 `"unclassified"` と決して衝突しないので安全）。

`ArticleList`（#10 が `FeedList.tsx` を改名）は `useSelection()`/`scopeFromPath()` の scope から `listArticles` 引数を組む。**#10 は folder scope では「リクエストを送らずプレースホルダ」を描く設計**（`docs/design/10-two-pane-layout.md` §6.5）なので、本機能のマージ時に「folder scope の source/fetcher を有効化」する作業が #10 との接続点になる:
```ts
// ArticleList の createResource source（#10 の合成点に #02 が folder/unclassified を足す）
const source = () => {
  const s = scope();              // #10: { kind, feedId?/folderId? }
  if (s.kind === "all")    return { unread: ui.filter === "unread" };
  if (s.kind === "feed")   return { feed_id: s.feedId, unread: ui.filter === "unread" };
  // folder:
  if (s.folderId === "unclassified")
    return { unclassified: true, unread: ui.filter === "unread" };
  return { folder_id: s.folderId, unread: ui.filter === "unread" };
};
// fetcher: (p) => api.listArticles(p)
```
> #10 未着手の暫定期間は、ツリーを現 `App.tsx` 単一カラムに仮置きしても API は同じで動く（フロントの最終置き場所だけ 10 に依存。バックエンドは非依存）。
> 代替案（より綺麗だが #10 への波及あり・本書では採らない）: `/folders/unclassified` センチネルの代わりに専用ルート `/unclassified` を追加する。in-handler の文字列分岐は消えるが、#10 の `path` 配列と `scopeFromPath`（`{ kind:"unclassified" }` の追加）への編集が必要になり、既に最終化済みの #10 を触ることになる。本書はセンチネルを採り、#10 を不変に保つ。将来クリーンアップ余地として記録（§11）。

### 6.4 装飾（既存トークンのみ）
- ツリー項目: `text-sm h-8 px-2 rounded-md`、ホバー `hover:bg-accent`、選択中 `bg-accent text-accent-foreground`。
- フォルダ見出し: `font-medium`、展開アイコン（`lucide-solid` のシェブロン採用は feature 07 の決定に従う。未導入なら自前の `▸/▾` で可）。
- 未分類グループ見出しは通常フォルダと同じ体裁＋`text-muted-foreground` で区別。罫線は `border-border`、角丸は `--radius` 由来。生 hex は使わない。
- フォーカスリング `focus-visible:ring-2 ring-ring` を全インタラクティブ要素に維持。

---

## 7. API 契約

> 下記の `position` 値や ID は**例示（illustrative）**。実際の `position` は `MAX(position)+1` 採番（並行挿入非厳密・単一ユーザで実害なし）で決まるため、固定値を期待しないこと。

### folders スライス（新規）

`GET /api/folders` → 200（position, created_at 昇順）
```json
[
  { "id": "11111111-1111-1111-1111-111111111111", "name": "Tech",  "position": 1, "created_at": "2026-06-26T00:00:00Z" },
  { "id": "22222222-2222-2222-2222-222222222222", "name": "News",  "position": 2, "created_at": "2026-06-26T00:01:00Z" }
]
```

`POST /api/folders` body `{ "name": "Design" }` → 201（新規フォルダ。`position` は既存最大+1＝この例では 3）
```json
{ "id": "33333333-3333-3333-3333-333333333333", "name": "Design", "position": 3, "created_at": "2026-06-26T00:02:00Z" }
```
名前が空白のみ / 100字超 → 400 `{ "error": "invalid input: folder name must not be empty" }`（または `... at most 100 chars`）

`PATCH /api/folders/{id}` body `{ "name": "Tech News" }` → 200（更新後 Folder）／対象なし → 404 `{ "error": "resource not found" }`

`DELETE /api/folders/{id}` → 204（ボディなし。配下フィードは `SET NULL` で未分類へ）／対象なし → 404

### feeds スライス（PATCH 追加・本書が唯一の所有者）

`PATCH /api/feeds/{id}` → 200（更新後 Feed、`folder_id` を含む）
- 割当: `{ "folder_id": "33333333-3333-3333-3333-333333333333" }`
- 未分類化（解除）: `{ "folder_id": null }`
- リネーム（feature 01 が再利用）: `{ "title": "New Title" }`
- 未指定フィールドは据え置き（キー自体を送らない）
```json
{
  "id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
  "url": "https://example.com/feed.xml",
  "title": "Example Blog",
  "folder_id": "33333333-3333-3333-3333-333333333333",
  "created_at": "2026-06-20T00:00:00Z",
  "last_fetched_at": "2026-06-26T00:00:00Z"
}
```
- 実在しないフォルダへの割当 → 400 `{ "error": "invalid input: folder not found" }`
- title が空白のみ → 400 `{ "error": "invalid input: title must not be empty" }`
- 対象フィードなし → 404

### articles スライス（クエリ追加）

`GET /api/articles?folder_id=<uuid>` → 200（当該フォルダ配下フィードの記事、published desc, LIMIT 200）
`GET /api/articles?unclassified=true` → 200（`folder_id IS NULL` のフィードの記事）
`?unread=true` と併用可。`feed_id` 指定時は従来どおり。未知/未対応クエリは `deny_unknown_fields` 不在のため無視（後方互換）。

---

## 8. 依存関係

- **このフィーチャをブロックする側はなし**（バックエンドは他機能非依存で先行マージ可能）。
- **このフィーチャに依存する側（本書が契約/構造を提供）**:
  - feature 01 `feed-management` … 本書が定義する `PATCH /api/feeds/{id}`（title リネーム）と `feeds.folder_id` を**再利用**。0002 マイグレーションと PATCH の定義は**本書が所有**（01 は UI のみ。ルート/ハンドラ/`UpdateFeed`/`repository::update` を再定義しない）。
  - feature 03 `feed-stats` / 09 `read-management` … ツリーの未読数バッジ差し込み先（本書がノード構造とスロットを提供）。
- **このフィーチャが依存（dependsOn）**:
  - feature 10 `two-pane-layout` … ツリーの最終的な置き場所（左ペイン Sidebar・永続インスタンス）と URL 駆動の選択（`useSelection`/`scopeFromPath`/`/folders/:folderId` ルート枠）を提供。**ただしバックエンド3点（0002 / folders スライス / feeds・articles 拡張）は他機能に非依存で先行マージ可能**。フロントのツリーも 10 未着手なら現シェルへ仮置き可（API は不変）。
- 横断方針: マイグレーションは追記のみ・`query!` 不使用・新 trait 無し・既存スライス拡張は §5 の正当化に限定。

依存配列（DESIGN_SCHEMA `dependsOn`）: `["10-two-pane-layout"]`。

---

## 9. テスト計画（TDD: Red を先に）

### 単体（`#[cfg(test)] mod tests`、実DB不要・`cargo test` / `just test` で実行）

`backend/src/features/folders/domain.rs`（`FeedUrl::parse` の網羅に倣う。§5.1 にテスト全文同梱）:
1. `parse_accepts_normal_name` … `"Tech"` → Ok、`as_str()=="Tech"`。
2. `parse_trims_whitespace` … `"  Tech  "` → `"Tech"`。
3. `parse_rejects_empty` … `""` → Err。
4. `parse_rejects_whitespace_only` … `"   "` → Err。
5. `parse_accepts_boundary_100_and_rejects_101` … 100 文字 Ok / 101 文字 Err（境界）。

`backend/src/features/feeds/handler.rs`（double-option の三値判別＝今回の最頻バグ源を Red 化。`#[cfg(test)] mod tests` を追加）:
6. `update_feed_omitted_folder_id_is_none` … `serde_json::from_str::<UpdateFeed>("{\"title\":\"x\"}")` → `folder_id == None`（据え置き）。
7. `update_feed_null_folder_id_is_some_none` … `"{\"folder_id\":null}"` → `folder_id == Some(None)`（未分類化）。
8. `update_feed_value_folder_id_is_some_some` … `"{\"folder_id\":\"<uuid>\"}"` → `Some(Some(uuid))`（割当）。
   - 実装メモ: テストでは `UpdateFeed` の各フィールドが `pub` なので `from_str` 後に直接 assert できる。

### 結合（実在前例＝シェルスクリプト。稼働スタック nginx:8081 へ curl）

> **blocking issue 解消**: `scripts/test/api-stats.sh` は GET 単発で ID 抽出も前提条件も後始末も無い。本機能はステートフル（POST/PATCH のレスポンスから UUID を抽出し、割当先フィードを用意し、後始末する）なので、その前例を**そのまま流用できない**。よって以下を明示する: (1) 抽出ツール = `jq`（`/usr/bin/jq` に存在を確認済み）、(2) 割当先フィードは stub URL で POST シードしてその id を抽出（フェッチは best-effort なので到達不能 URL でも 201 でフィード行は作られる＝`feeds/service.rs::create_feed` 実測）、(3) 後始末は `trap ... EXIT` でテストフォルダ/フィードを DELETE。`set -uo pipefail`（`-e` は使わない＝assert で続行・最後に集計）。

新規ファイル `scripts/test/api-folders.sh`（実行ビット付与）。全文:
```bash
#!/usr/bin/env bash
# Integration test: folders CRUD + feeds PATCH(folder assign) + articles folder filter.
# Runs against the running stack (nginx :8081). Requires: jq.
# set -uo pipefail (NOT -e): assertions report and we exit non-zero at the end if any failed.
set -uo pipefail
BASE="${1:-http://localhost:8081}"

pass=0
fail=0
created_folder=""
seeded_feed=""

cleanup() {
  [ -n "$created_folder" ] && curl -s -o /dev/null -X DELETE "$BASE/api/folders/$created_folder"
  [ -n "$seeded_feed" ] && curl -s -o /dev/null -X DELETE "$BASE/api/feeds/$seeded_feed"
}
trap cleanup EXIT

# req METHOD PATH [JSON]  -> sets globals: code, body
req() {
  local method="$1" path="$2" data="${3:-}" out
  if [ -n "$data" ]; then
    out="$(curl -s -m 8 -w $'\n%{http_code}' -X "$method" \
      -H 'Content-Type: application/json' -d "$data" "$BASE$path")"
  else
    out="$(curl -s -m 8 -w $'\n%{http_code}' -X "$method" "$BASE$path")"
  fi
  code="${out##*$'\n'}"
  body="${out%$'\n'*}"
}

want() { # want DESC EXPECTED_CODE
  if [ "$code" = "$2" ]; then echo "PASS: $1 ($code)"; pass=$((pass+1));
  else echo "FAIL: $1 — expected $2 got $code (body: $body)"; fail=$((fail+1)); fi
}

assert() { # assert DESC CONDITION_RESULT(0=ok)
  if [ "$2" = "0" ]; then echo "PASS: $1"; pass=$((pass+1));
  else echo "FAIL: $1 (body: $body)"; fail=$((fail+1)); fi
}

# --- seed a feed to assign (stub URL; fetch is best-effort so 201 regardless) ---
req POST /api/feeds "{\"url\":\"http://127.0.0.1:9/__test_$$.xml\"}"
want "seed feed" 201
seeded_feed="$(echo "$body" | jq -r '.id')"

# A. create folder
req POST /api/folders '{"name":"_t_folder"}'
want "A create folder" 201
created_folder="$(echo "$body" | jq -r '.id')"
[ "$(echo "$body" | jq -r '.name')" = "_t_folder" ]; assert "A name == _t_folder" $?

# B. list contains it
req GET /api/folders
want "B list folders" 200
echo "$body" | jq -e --arg id "$created_folder" 'any(.[]; .id == $id)' >/dev/null; assert "B list contains folder" $?

# C. rename
req PATCH "/api/folders/$created_folder" '{"name":"_t_folder2"}'
want "C rename" 200
[ "$(echo "$body" | jq -r '.name')" = "_t_folder2" ]; assert "C name == _t_folder2" $?

# D. assign feed to folder
req PATCH "/api/feeds/$seeded_feed" "{\"folder_id\":\"$created_folder\"}"
want "D assign feed" 200
[ "$(echo "$body" | jq -r '.folder_id')" = "$created_folder" ]; assert "D folder_id == created" $?

# E. unclassify
req PATCH "/api/feeds/$seeded_feed" '{"folder_id":null}'
want "E unclassify feed" 200
[ "$(echo "$body" | jq -r '.folder_id')" = "null" ]; assert "E folder_id == null" $?

# F. folder filter (re-assign, then filter articles)
req PATCH "/api/feeds/$seeded_feed" "{\"folder_id\":\"$created_folder\"}"
req GET "/api/articles?folder_id=$created_folder"
want "F articles by folder" 200

# G. delete folder -> feed back to unclassified (SET NULL)
req DELETE "/api/folders/$created_folder"
want "G delete folder" 204
created_folder=""  # already deleted; avoid double-delete in cleanup
req GET /api/feeds
echo "$body" | jq -e --arg id "$seeded_feed" 'any(.[]; .id == $id and .folder_id == null)' >/dev/null
assert "G feed unclassified after folder delete (SET NULL)" $?

# H. error cases
req POST /api/folders '{"name":"   "}'
want "H1 whitespace name -> 400" 400
req PATCH "/api/feeds/$seeded_feed" '{"folder_id":"00000000-0000-0000-0000-000000000000"}'
want "H2 assign nonexistent folder -> 400" 400
req PATCH "/api/folders/00000000-0000-0000-0000-000000000000" '{"name":"x"}'
want "H3 patch missing folder -> 404" 404
req DELETE "/api/folders/00000000-0000-0000-0000-000000000000"
want "H4 delete missing folder -> 404" 404

echo "----"
echo "PASS=$pass FAIL=$fail"
[ "$fail" -eq 0 ]
```
実行: スタックを `just up`（または compose 起動）してから `bash scripts/test/api-folders.sh`。`PASS=… FAIL=0` かつ終了コード 0 を確認。

> **代替（任意・Rust ハーネス）**: `backend/tests/folders.rs` を新設し、`sqlx::PgPool::connect(env!("DATABASE_URL"))` で実 DB に接続して `folders::service` / `feeds::service` を直接駆動する手もある。`just test`（=`cd backend && cargo test`）が拾う。ただし `backend/tests/` は**現状リポジトリに存在しない**ため、(1) `DATABASE_URL` を指す稼働 Postgres（`just dev-db`）、(2) テスト用フィクスチャ（feeds 行のシード）と後始末（テスト内でトランザクション or 明示 DELETE）を自前で用意する必要がある。本書は実在前例＝シェルを第一とし、こちらは任意採用とする。

### フロント（手動 + 型）
- `tsc --noEmit` / `just lint` を通す（`Folder` 型・`Feed.folder_id`・`updateFeed`・`listArticles` 拡張の型整合）。
- 手動: フォルダ作成→ツリーに出る／フィードを移動→ツリー再描画／フォルダ選択→記事一覧が絞られる／未分類選択→未分類記事のみ／フォルダ削除→配下フィードが未分類へ移動／`folder_id: null` 送信で割当解除。

---

## 10. 実装手順（順序付きチェックリスト）

1. **マイグレーション**: `backend/migrations/0002_folders.sql` を §4 の SQL で新規作成。`just migrate`（または起動時 `run_migrations`）で適用確認。`0001` は触らない。
2. **folders スライス（TDD）**:
   1. `backend/src/features/folders/domain.rs` に `FolderId`/`FolderName::parse`/`Folder` と `#[cfg(test)]`（テスト1–5）を書く → `just test` で Red→Green。
   2. `repository.rs`（insert/list_all/update_name/delete）→ `service.rs` → `handler.rs` → `mod.rs::routes()` を §5.1 どおり実装（各ファイルの `use` を §5.1 の通り）。
   3. `backend/src/features/mod.rs` に `pub mod folders;` と `.merge(folders::routes())` を追加（2行）。
3. **feeds 拡張**:
   1. `feeds/domain.rs` の `Feed` に `folder_id: Option<FolderId>` を追加（`FolderId` import）。
   2. `feeds/repository.rs`: import に `AppError` と `FolderId` を追加。`insert` の RETURNING と `list_all` の SELECT に `folder_id` を追加。`update` / `folder_exists` を追加。
   3. `feeds/service.rs`: `FolderId` import 追加。`update_feed` を追加。
   4. `feeds/handler.rs`: `FolderId` import 追加。`UpdateFeed` + `double_option` + `update` を追加（テスト6–8 を Red→Green）。
   5. `feeds/mod.rs` の `{id}` ルートに `.patch(handler::update)` をチェーン（1行）。
4. **articles 拡張**: `articles/repository.rs`（`FolderId` import + `list` に2引数・2条件）→ `articles/service.rs`（`FolderId` import + `list_articles` に2引数素通し）→ `articles/handler.rs`（`FolderId` import + `ListQuery` に2フィールド + `list` 呼び出し行を§5.3の5引数版へ置換）。`articles::list` を呼ぶのはこの handler だけ（`feeds/service.rs` のクロールは `articles::repository::upsert` を使うので無影響）。
5. **`cargo fmt` + `just lint`（clippy `-D warnings`）** を通す。
6. **結合テスト**: `scripts/test/api-folders.sh` を §9 の全文で作成（`chmod +x`）。スタック起動して `PASS=… FAIL=0` を確認。
7. **フロント API**: `frontend/src/lib/api.ts` に `Folder` 型・`Feed.folder_id`・`listFolders/createFolder/updateFolder/deleteFolder/updateFeed`・`listArticles` 拡張を追加（§6.1。`updateFeed` の JSDoc も）。
8. **フロント UI**:
   1. `components/ui/select.tsx`（Ark UI Select、part 名を ark-ui.com で確認）を追加。
   2. `components/layout/FeedTree.tsx`（自前折りたたみ + 未分類グループ + フォルダ作成/改名/削除 + フィード移動 select）を実装。
   3. `/folders/:folderId` の `unclassified` センチネル分岐を `ArticleList` の source 合成へ反映（§6.3。#10 の `scopeFromPath`/`useSelection` の実シグネチャを確認して読み替え）。
   4. ツリーを Sidebar（feature 10）へマウント（10 未着手なら現 `App.tsx` に仮置き）。
9. **`tsc --noEmit` / `just lint`** を通し、§9 の手動シナリオを確認。
10. （コミットはユーザー指示時のみ。作業ブランチを切る。）

---

## 11. リスク・未決事項・代替案

- **Ark UI v5 の API 不確実性**: Select の part 名（`Select.Root/Control/Trigger/Positioner/Content/Item/ItemText`）・`createListCollection({ items })`、および TreeView の `createTreeCollection`/`Branch*`/`Item*` は **バージョンで変わりうる。実装時に ark-ui.com(Solid) で確認**（`dialog.tsx` 冒頭コメントと同じ運用）。緩和: ツリー v1 は自前の折りたたみリストで Ark 依存を回避し、後で `tree-view.tsx` に昇格（差し替えは1ファイルに閉じる）。
- **`PATCH /api/feeds/{id}` の所有権（解消済み）**: 本書が**唯一の定義所有者**。feature 01 は再利用のみ（再定義しない）。`docs/design/00-foundation-backend.md` の §2.2 表・§6 マトリクスを本書所有に更新済み。並行実装時はこの契約（`{title?, folder_id?}`、未指定=据え置き、null=未分類化）を単一の真実とする。
- **serde 三値判別（double-option）**: `#[serde(default, deserialize_with = "double_option")]` は「キー無し / null / 値」を `None / Some(None) / Some(Some)` に分ける。手書きヘルパで足りるが、`serde_with` の `double_option` を導入してもよい（依存追加。単一ヘルパで済むなら不要）。テスト6–8 で挙動を固定する。フロント側は `undefined` を「据え置き」、`null` を「未分類化」とする（§6.1 JSDoc。`undefined` で解除を期待しないこと）。
- **`folder_exists` は advisory**: 整合性の本命は FK `feeds.folder_id REFERENCES folders(id)`。`folder_exists` チェックは 23503（FK 違反=500）を `Validation`(400) に整形するためだけにある。単一ユーザなので「チェック後・UPDATE 前にフォルダが消える」TOCTOU は実害なし。チェックを省いても DB 整合性は壊れない（ただしエラーは 500 になる）。
- **未分類のルーティング**: `/folders/unclassified` センチネルを採用（UUID と文字列 `"unclassified"` は衝突不能ゆえ安全）。より綺麗な代替は専用 `/unclassified` ルートだが、最終化済みの #10（`path` 配列 + `scopeFromPath`）への編集が要るため本書では採らない。将来クリーンアップ余地として記録。
- **`articles::list` シグネチャ成長**: §5.3 に**確定シグネチャ**を固定（`feed_id, unread_only, folder_id, unclassified`）。#09 は別エンドポイント、#11 はバックエンド変更なしのため、実際に `list` を変えるのは #02 のみ。将来のフィルタ追加は末尾追記方向で積む。
- **`feeds` → `folders` のコンパイル依存**: `Feed.folder_id: Option<FolderId>` と `folder_exists` で feeds が folders に依存する。`articles → feeds` と同じ一方向で対称・祝福済み（循環なし）。循環を避けたい場合の代替は `folder_id` を生 `Option<Uuid>` にすること（newtype 安全性は下がる）。本書は newtype を採る。
- **`position` の並行採番**: `SELECT COALESCE(MAX(position),0)+1` は並行 INSERT に対して厳密でない（同値が付きうる）。単一ユーザ前提で実害なし。厳密化が要れば将来シーケンス/別設計へ。§7 の `position` 値は例示。
- **フォルダ名の一意性**: 現状 UNIQUE 無し（重複名可）。実害が出たら 0003 以降で UNIQUE 追加し、衝突を `Validation` にマップ（その際 `insert` のエラーハンドリング追加）。
- **土台ドキュメントの不正確さ（記録）**: `00-foundation-backend.md` は `backend/tests/` の Rust 統合テスト前例（`stats`）を引くが、実際には存在しない（§3 末尾）。隣接スライスの実装者は「Rust ハーネスが既にある」と仮定しないこと。本書は `scripts/test/*.sh` を第一前例とした。
- **N+1/性能**: ツリーは `listFolders`+`listFeeds` の2リクエストでクライアント整形。フィード数は家庭内利用で小規模のため問題なし。記事のフォルダ絞り込みはサブクエリ `IN (SELECT id FROM feeds WHERE folder_id=...)` で `idx_feeds_folder_id` が効く。
- **並べ替え**: `position` は採番のみで編集 UI なし。ドラッグ&ドロップ等は将来フィーチャ（`PATCH /api/folders/{id} {position}` 追加で対応可能）。
