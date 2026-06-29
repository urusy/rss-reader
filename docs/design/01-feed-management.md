# 01 フィード管理機能

> 読み手は「このリポジトリは持っているが、この会話の文脈は知らない別セッションの実装者」。曖昧さを残さず、このドキュメントだけで実装に着手できる粒度で書く。実コード（`backend/src/features/{feeds,stats,articles}/*`, `frontend/src/lib/api.ts`）を裏取り済み。

## 1. 概要

購読中のフィードを **記事一覧とは別画面**（専用ルート `/manage`）で管理する機能を新設する。ユーザーはここでフィードの一覧確認・**改名**・**削除**・**フォルダ割当**・**個別の再取得** を行い、各フィードの **未読件数 / 総記事数** を一目で把握できる。現状フィード操作は記事一覧画面（`frontend/src/routes/FeedList.tsx`。実体は記事一覧で、先頭に追加入力が同居）にしか無く、購読が増えると管理しづらい。これを独立画面へ切り出し、「読む（記事）」と「管理する（フィード）」の関心を分離する。

バックエンドは既存 `feeds` スライスに「改名 / フォルダ割当」用の `PATCH /api/feeds/{id}` を追加し（同一 Feed アグリゲートへの書き込み）、プレースホルダのままになっている per-feed refresh（現状は全件再取得）を本来の挙動へ修正する。フィード別の集計（未読数 / 総記事数）は **読み取り専用の新スライス `feed_overview`**（`GET /api/feeds/overview`）が JOIN 集計で返す（既存 `stats` スライスと同じ CQRS-lite の前例に従う）。

## 2. スコープ / 非スコープ

### 含む（スコープ）
- 専用ルート `/manage` と画面 `frontend/src/routes/FeedManage.tsx`（記事一覧から分離）。
- フィード **改名**: `PATCH /api/feeds/{id}` の `title`。
- フィード **フォルダ割当**: `PATCH /api/feeds/{id}` の `folder_id`（`null` で「未分類」へ戻す）。**feature 02 の `0002_folders.sql` 適用が前提**（§4, §8）。
- フィード **削除**: 既存 `DELETE /api/feeds/{id}` を管理ビューから利用（再実装しない）。
- フィード **個別再取得**: `POST /api/feeds/{id}/refresh` を「全件再取得」から「当該フィードのみ」へ修正。
- フィード別 **未読件数 / 総記事数**: 新スライス `feed_overview`（`GET /api/feeds/overview`）。**このスライスの新設と `features/mod.rs` への `.merge()` は本機能（01）が所有する**（§5.4 の所有契約）。
- `lib/api.ts` への `updateFeed` / `refreshFeed` / `listFeedOverview` 追加、`Feed` 型に `folder_id` 追加、`FeedOverview` 型追加。

### 含まない（非スコープ）
- **フォルダ自体の CRUD**（作成 / 改名 / 削除 / 並び替え）、`folders` テーブル・`feeds.folder_id` 列の **DDL**、`GET /api/folders`、`lib/api.ts::listFolders()`・`Folder` 型 → **feature 02 (feed-folders)** が所有。本機能はそれらに**ハード依存**する（§8）。
- **フィード追加 UI の配置**（追加入力の Dialog 化・サイドバー下部への移動）→ feature 08。管理ビューに追加 UI を再実装しない。
- **最終投稿日時 / 投稿頻度** の表示 → feature 03 (feed-stats)。03 は本機能が立ち上げる `feed_overview` スライスを **列追記で拡張** する（§5.3, §5.4, §11）。
- **二ペインシェル / サイドバー本体** → feature 10。本機能は `/manage` への導線を暫定で 1 リンク足すに留める。
- **一括既読 / 未読フィルタトグル** → feature 09 / 11。

## 3. 既存実装の調査と再利用（車輪の再発明を避ける）

実ファイルを調査済み。以下を **再利用** する。

| 資産 | 場所 | 再利用方法 |
|------|------|-----------|
| `feeds` スライス一式 | `backend/src/features/feeds/{domain,repository,service,handler,mod}.rs` | 改名 / フォルダ割当 / 個別更新を **このスライス内に追記**（新スライスを切らない。理由 §5.1）。 |
| `FeedId` newtype, `FeedUrl::parse` | `feeds/domain.rs`（`FeedId(pub Uuid)` + `#[sqlx(transparent)]`、`parse()->Result<_, String>`） | `FeedId` をそのまま使用。`FeedTitle::parse` を `FeedUrl::parse` と同じ「`parse()->Result<_, String>`」型で新設。 |
| `repository::{insert,list_all,delete,touch_fetched}` | `feeds/repository.rs`（明示カラム SELECT / RETURNING、`query_as::<_, Feed>`） | 既存パターンを踏襲して `get` / `update` を追加。`list_all` は既に `ORDER BY created_at DESC` 済み。 |
| `service::{create_feed,refresh_all_feeds,fetch_and_store}` | `feeds/service.rs` | `fetch_and_store(state, &feed)` を **再利用** して `refresh_one` を実装。`refresh_all_feeds` は scheduler が継続使用するため温存。 |
| `stats` スライス（読み取り集計の前例） | `backend/src/features/stats/*`（`query_as` タプル → `Stats`、`fetch`→`get_stats`→`handler::get`、`routes()` 1 行） | `feed_overview` を **同じ形** で新設。CQRS-lite の前例。 |
| `articles` の `FeedId` 越境 import 前例 | `backend/src/features/articles/repository.rs` が `use crate::features::feeds::domain::FeedId;` 済み | `feed_overview` から `FeedId` を import する根拠（型の共有であり越境書き込みではない）。 |
| `articles.is_read` 列 + 部分インデックス | `migrations/0001_init.sql`（`idx_articles_is_read WHERE is_read=false`） | 未読集計 `COUNT(...) FILTER (WHERE is_read=false)`。**新規列 / インデックス不要**。 |
| 既存 `DELETE /api/feeds/{id}` | `feeds/handler.rs::delete` → `service::delete_feed`（0 件で `NotFound`） | 管理ビューの削除はこれを呼ぶだけ。再実装しない。 |
| `AppError`（6 バリアント） | `shared/error.rs` | `NotFound`(404)/`Validation`(400)/`Upstream`(502)/`Database`(500) を使い分け。**新バリアントを足さない・編集しない**。 |
| フロント `http<T>()`（204→undefined 畳み込み） | `frontend/src/lib/api.ts` | 既存ヘルパに `method`/`body` を渡して新メソッドを追加。 |
| `components/ui/{button,card,dialog}.tsx` | フロント | 削除確認は既存 `dialog.tsx`（Ark UI ラップ）を再利用。`button` を行アクションに使用。 |
| ルーティング配線 | `frontend/src/index.tsx`（`<Router root={App}>` + `<Route>`） | `/manage` ルートを 1 行追加。 |

**結論**: 改名 / フォルダ割当の *書き込み* は feeds スライスへの小さな追記、未読集計は stats と同型の新読み取りスライスで賄える。削除・未読列・記事取得ロジックは全て既存資産で、作り直さない。

## 4. データモデルとマイグレーション

**本機能自体は新規マイグレーションを追加しない（DB 変更なし）。**

- 改名は既存 `feeds.title TEXT`（nullable）に書く。
- 未読 / 総数は `articles.is_read` と既存テーブルを読むだけ。
- フォルダ割当に必要な **`feeds.folder_id` 列は feature 02 の `backend/migrations/0002_folders.sql`** が追加する（`folder_id UUID REFERENCES folders(id) ON DELETE SET NULL` + `idx_feeds_folder_id`）。本機能はこの列の存在を前提とする（§8 ハード依存）。

> 既存マイグレーション（`0001_init.sql`）は編集しない。本機能で新ファイルは作らない。`folder_id` 列・`folders` テーブルの DDL は 02 の責務であり、ここでは定義しない。

`feeds` テーブル（**0002 適用後** の想定形）:
```sql
feeds(
  id UUID PRIMARY KEY,
  url TEXT NOT NULL UNIQUE,
  title TEXT,
  folder_id UUID NULL REFERENCES folders(id) ON DELETE SET NULL,  -- 0002 (feature 02) が追加
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_fetched_at TIMESTAMPTZ
)
```
`folder_id IS NULL` = 「未分類」。フォルダ削除時は `ON DELETE SET NULL` で自動的に未分類へ落ちる。

### 4.1 実装順序の制約（重要・実行時クラッシュ回避）

sqlx の `query_as` は **列名（position ではなく column NAME）** でマッピングする。**存在しない `folder_id` 列を SELECT/RETURNING に書くと、`GET /api/feeds`（`list_all`）/`POST /api/feeds`（`insert`）/`POST /api/feeds/{id}/refresh` が実行時にエラーになる**（コンパイルは通る。`query!` マクロを使わないため）。したがって **`folder_id` を参照するコードは 0002 適用後にのみ追加する**。

本機能の実装には次の 2 通りの安全な順序がある。

- **(A) 推奨**: feature 02 の `0002_folders.sql` を **先に適用**してから、本機能を folder_id を含めて一括実装する（本機能は 02 にハード依存しているため、これが自然）。
- **(B) 早期スライス**: まず **02 非依存サブセット**（`FeedTitle` による **title のみ** の改名 PATCH、`refresh_one`、`feed_overview` の未読 / 総数）を folder_id を一切参照せずに実装する。その後 0002 適用を待って、**folder_id サーフェス**（`Feed.folder_id` フィールド・`insert`/`list_all`/`get`/`update` の folder_id SELECT/RETURNING・`folder_exists` 存在チェック・PATCH の folder_id 経路）を追加する。

§5.2 では各編集を **［02非依存］** / **［要0002適用＝folder_idサーフェス］** と明示する。順序の正は **0002 適用 → `Feed`/`insert`/`list_all` に folder_id 追加 → PATCH の folder_id 経路** である。

## 5. バックエンド設計

### 5.1 方針: `feeds` 拡張 + `feed_overview` 新設（土台設計準拠）

- **改名 / フォルダ割当 / 個別更新は `feeds` スライスに追記する**。これらは Feed アグリゲートへの **書き込み** であり、別スライスから `feeds` を UPDATE すると越境書き込みになり悪化する（土台設計 §2.2 の既存スライス拡張の正当化に従う）。
- **未読 / 総数の集計は読み取り専用の新スライス `feed_overview`**（`GET /api/feeds/overview`）。書き込みを持たず、`feeds`/`articles` を JOIN して読むだけ。これは禁止された「越境共通レイヤー」ではなく、`stats` と同じ独立読み取りスライス（CQRS-lite）。
- **trait/dyn は足さない**（差し替える第二実装がない）。

### 5.2 `feeds` スライス拡張（追記のみ）

> **［FromRow に関する注意］** sqlx の `FromRow` は **列名（column NAME）で構造体フィールドへマッピングする。位置（順序）は無関係**。よって `RETURNING`/`SELECT` の列の **並び順をフィールド順に揃える必要はない**。揃えるべきは **列名と構造体フィールド名** だけである（順序合わせを追う必要はない）。

#### domain.rs — `FeedTitle` 値オブジェクト追加 ［02非依存］
```rust
/// 改名で受け取るユーザー指定タイトル。trim 後に空でないこと・200 字以内を保証する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedTitle(String);

impl FeedTitle {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into();
        let t = s.trim();
        if t.is_empty() {
            return Err("feed title must not be empty".to_string());
        }
        if t.chars().count() > 200 {
            return Err("feed title must be 200 characters or fewer".to_string());
        }
        Ok(Self(t.to_string()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

#### domain.rs — `Feed` に `folder_id` 追加 ［要0002適用＝folder_idサーフェス］
```rust
pub struct Feed {
    pub id: FeedId,
    pub url: String,
    pub title: Option<String>,
    pub folder_id: Option<uuid::Uuid>, // 0002 適用後にのみ追加。null = 未分類
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}
```
> `folder_id` は **生 `Uuid`（newtype にしない）**。`FolderId` newtype は feature 02 の `folders` スライス所有で、ここで import すると `feeds → folders` のコンパイル依存（越境）を生む。API は土台設計どおり `folder_id: string | null` をそのまま返せばよく、生 Uuid で十分。serde で `"folder_id": null`／`"folder_id": "<uuid>"` として出力される。

`Feed` に `folder_id` を追加したら、**同時に**既存クエリの列リストも追記する（**0002 適用後にのみ**）:
- `insert` の `RETURNING id, url, title, created_at, last_fetched_at` → `folder_id` を追加。
- `list_all` の `SELECT id, url, title, created_at, last_fetched_at` → `folder_id` を追加。

> **担当競合に注意**: `Feed` 構造体・`insert`/`list_all` への folder_id 追記は 01 と 02 のどちらが先着しても必要になりうる。**一方が追加済みなら重複追加しない**（重複すればコンパイルエラーで気づける）。

#### repository.rs — `get` / `update` / `folder_exists` を追加 ［要0002適用＝folder_idサーフェス］
`AppError` を import に追加（`use crate::shared::error::{AppError, AppResult};`）。`get`/`update` は `folder_id` を SELECT/RETURNING するため 0002 適用後にのみ追加する。
```rust
pub async fn get(pool: &PgPool, id: FeedId) -> AppResult<Feed> {
    sqlx::query_as::<_, Feed>(
        r#"SELECT id, url, title, folder_id, created_at, last_fetched_at
           FROM feeds WHERE id = $1"#,
    )
    .bind(id.0)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// 部分更新。set_* フラグで「変更するか / 据え置くか」を制御し、
/// folder_id は set_folder=true & None で未分類(NULL)に設定できるようにする。
pub async fn update(
    pool: &PgPool,
    id: FeedId,
    set_title: bool,
    title: Option<&str>,     // set_title=true のとき必ず Some（FeedTitle 由来）
    set_folder: bool,
    folder_id: Option<Uuid>, // set_folder=true で None なら未分類
) -> AppResult<Feed> {
    sqlx::query_as::<_, Feed>(
        r#"UPDATE feeds
           SET title     = CASE WHEN $2 THEN $3::text ELSE title END,
               folder_id = CASE WHEN $4 THEN $5::uuid ELSE folder_id END
           WHERE id = $1
           RETURNING id, url, title, folder_id, created_at, last_fetched_at"#,
    )
    .bind(id.0)        // $1
    .bind(set_title)   // $2
    .bind(title)       // $3
    .bind(set_folder)  // $4
    .bind(folder_id)   // $5
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// folder_id の事前存在チェック（FK 違反を 500 ではなく 400 にするため）。
/// folders テーブルへの read-only 参照。02 がハード依存なので必ず存在する。
pub async fn folder_exists(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM folders WHERE id = $1)",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}
```
> **明示キャスト `$3::text` / `$5::uuid` の理由**: `CASE WHEN $2 THEN $3 ELSE title END` のようにパラメータが **CASE の THEN 分岐にしか現れない**と、Postgres が「could not determine data type of parameter」を返すことがある。明示キャストで型を固定して回避する。なお既存 `touch_fetched` の `COALESCE($2, title)` パターンは **NULL を「セットする」表現ができない**（COALESCE は NULL を「据え置き」に潰す）ため未分類化に再利用できない。だから CASE + フラグ方式を採る。
> `query!` コンパイル時マクロは使わない。実行時 `query`/`query_as`/`query_scalar` のみ。

#### service.rs — `update_feed` / `refresh_one` を追加
`refresh_one` は ［02非依存］だが、`get` を呼ぶため（`get` は folder_id サーフェス）、順序 (B) では「`get` から folder_id を外した暫定版」か、0002 適用後に揃えること。`update_feed` の folder 経路は ［要0002適用］。
```rust
pub async fn update_feed(
    state: &AppState,
    id: FeedId,
    title: Option<FeedTitle>,            // 検証済み
    folder_change: Option<Option<Uuid>>, // None=据え置き / Some(None)=未分類 / Some(Some(x))=割当
) -> AppResult<Feed> {
    // 存在しない folder_id を 500(FK) ではなく 400 に倒す事前チェック。
    if let Some(Some(fid)) = folder_change {
        if !repository::folder_exists(&state.db, fid).await? {
            return Err(AppError::Validation(
                "folder_id does not reference an existing folder".to_string(),
            ));
        }
    }
    let set_title = title.is_some();
    let title_ref = title.as_ref().map(|t| t.as_str());
    let set_folder = folder_change.is_some();
    let folder_val = folder_change.flatten();
    repository::update(&state.db, id, set_title, title_ref, set_folder, folder_val).await
}

/// 単一フィードのみ再取得（従来の全件 refresh ではなく当該フィードだけ）。
pub async fn refresh_one(state: &AppState, id: FeedId) -> AppResult<Feed> {
    let feed = repository::get(&state.db, id).await?; // 無ければ NotFound
    fetch_and_store(state, &feed).await?;             // 既存ロジックを再利用
    repository::get(&state.db, id).await              // 更新後（last_fetched_at 反映）を返す
}
```

#### handler.rs — `update` 追加 + `refresh` 差し替え + double-option デシリアライザ
```rust
use serde::{Deserialize, Deserializer};

/// `Option<Option<T>>` を「省略 / null / 値」に正しく分けるためのカスタムデシリアライザ。
///
/// 重要: 素の serde + serde_json では `#[serde(default)] folder_id: Option<Option<Uuid>>`
/// は **省略と null を区別できない**（どちらも外側 None）。serde_json は JSON `null` を
/// 外側 Option の visit_none で短絡し、内側へ再帰しないため。
/// このデシリアライザは「フィールドが存在するときは必ず内側まで再帰」させる。
/// `#[serde(default, ...)]` と併用すると:
///   - フィールド省略 → default が走り `None`（= 据え置き。デシリアライザは呼ばれない）
///   - 明示 null      → `Option::deserialize` が `None` を返し `Some(None)`（= 未分類）
///   - 値             → `Some(x)` を返し `Some(Some(x))`（= 割当）
fn de_double_opt<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(d)?))
}

#[derive(Debug, Deserialize)]
pub struct UpdateFeed {
    #[serde(default)]
    pub title: Option<String>, // 省略=改名しない（null と省略は区別しない）
    #[serde(default, deserialize_with = "de_double_opt")]
    pub folder_id: Option<Option<Uuid>>, // 省略=据え置き / null=未分類 / 値=割当
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFeed>,
) -> AppResult<Json<Feed>> {
    let title = match body.title {
        Some(raw) => Some(FeedTitle::parse(raw).map_err(AppError::Validation)?),
        None => None,
    };
    let feed = service::update_feed(&state, FeedId(id), title, body.folder_id).await?;
    Ok(Json(feed))
}

// 既存 refresh を「全件 + 202 無ボディ」から「単一 + 200 + 更新後 Feed」へ差し替え。
pub async fn refresh(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Feed>> {
    Ok(Json(service::refresh_one(&state, FeedId(id)).await?))
}
```
> `title` は `#[serde(default)]` のみなので **省略と `null` を区別しない**（どちらも `None` = 改名しない）。タイトルを `null` へクリアする操作は非対応（要件外。§11）。`folder_id` は上記 `de_double_opt` により **省略 / null / 値** を区別する。`StatusCode` import は既存 `delete` 用に handler に残るが、`refresh` が `Json<Feed>` を返すようになるため不要なら整理する（`create`/`delete` がまだ使う）。

#### mod.rs — ルート追記（同一パスにメソッド合成）
```rust
use axum::routing::{get, post}; // patch を追加
use axum::routing::patch;
// ...
Router::new()
    .route("/api/feeds", get(handler::list).post(handler::create))
    .route(
        "/api/feeds/{id}",
        axum::routing::delete(handler::delete).patch(handler::update), // PATCH 追加
    )
    .route("/api/feeds/{id}/refresh", post(handler::refresh))
```

### 5.3 新スライス `feed_overview`（読み取り専用）

`backend/src/features/feed_overview/` に 5 ファイル。

**domain.rs**
```rust
use serde::Serialize;
use crate::features::feeds::domain::FeedId; // articles スライスと同じ型 import 前例

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FeedOverview {
    pub feed_id: FeedId,
    pub unread_count: i64,
    pub total_count: i64,
    // feature 03 がここに last_published_at: Option<DateTime<Utc>>, posts_per_week: f64 を
    // 列追記で拡張する（後方互換。§5.4）。
}
```

**repository.rs**
```rust
use sqlx::PgPool;
use super::domain::FeedOverview;
use crate::shared::error::AppResult;

pub async fn list(pool: &PgPool) -> AppResult<Vec<FeedOverview>> {
    let rows = sqlx::query_as::<_, FeedOverview>(
        r#"SELECT
             f.id AS feed_id,
             COUNT(a.id) FILTER (WHERE a.is_read = false) AS unread_count,
             COUNT(a.id)                                  AS total_count
           FROM feeds f
           LEFT JOIN articles a ON a.feed_id = f.id
           GROUP BY f.id
           ORDER BY f.created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
```
- `LEFT JOIN` により記事ゼロのフィードも `unread_count=0, total_count=0` の 1 行を返す。
- `ORDER BY f.created_at DESC` で **決定的な並び**を保証（フロントは `feed_id` で突合するが、安定順は将来のページングやレスポンス差分にも有用。`feeds::list_all` と同じ並び）。
- **性能注記**: `total_count` は既読 / 未読を問わず全記事を数えるため、プランナはこのフィード配下の全 `articles` を走査する。部分インデックス `idx_articles_is_read` が `FILTER` 句に効くとは限らない。記事数が増えて遅くなったら、土台設計どおり集計列の materialized 化（将来・新マイグレーション）へ昇格する。**「部分インデックスを活用する」という最適化前提は置かない**。

**service.rs**
```rust
use super::repository;
use super::domain::FeedOverview;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn get_overview(state: &AppState) -> AppResult<Vec<FeedOverview>> {
    repository::list(&state.db).await
}
```

**handler.rs**
```rust
use axum::extract::State;
use axum::Json;
use super::domain::FeedOverview;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<FeedOverview>>> {
    Ok(Json(service::get_overview(&state).await?))
}
```

**mod.rs**
```rust
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::get;
use axum::Router;
use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/feeds/overview", get(handler::list))
}
```

### 5.4 `features/mod.rs` への合成（1 行追加）＋ 所有契約
```rust
pub mod feed_overview; // 追加
// ...
    .merge(feeds::routes())
    .merge(articles::routes())
    .merge(stats::routes())
    .merge(feed_overview::routes()) // 追加（本機能 01 が所有する唯一の merge 行）
```

> **所有契約（重複 merge の panic 回避）**: `feed_overview` スライスの **新設**（5 ファイル作成）と **`features/mod.rs` への `.merge(feed_overview::routes())` 1 行** は **本機能（01）が単独で所有する**。feature 03 (feed-stats) は同じ `feed_overview` スライスの **SELECT と `FeedOverview` FromRow を列追記で拡張するだけ**で、**`.merge()` 行を追加してはならない**。`features/mod.rs` に同一 `/api/feeds/overview` ルートを 2 回 `.merge()` すると axum は **起動時に panic** する。03 が先着した場合は、03 がスライス新設 + merge を行い、01 はフロント消費コードのみ足す（どちらが先着しても merge 行は 1 本だけ）。

> **ルート衝突なし**: axum 0.8 は静的セグメント `/api/feeds/overview` を `/api/feeds/{id}` より優先するため、`feeds` スライスの `{id}` パスとぶつからない。複数スライスが同一プレフィックスに `.merge()` するのは結合ではない（土台設計 §2.1）。

### 5.5 AppError の使い分け（`shared/error.rs` 不編集）
| 状況 | バリアント | HTTP |
|------|-----------|------|
| 存在しない feed への PATCH / refresh | `NotFound`（`get`/`update` の 0 行 → `ok_or`） | 404 |
| 空 / 空白のみ / 200 字超のタイトル | `Validation`（`FeedTitle::parse` の `Err(String)`） | 400 |
| 存在しない `folder_id` の割当 | `Validation`（`folder_exists` 事前チェックが false） | 400 |
| refresh 時の上流 HTTP / フィードパース失敗 | `Upstream`（既存 `fetch_and_store`） | 502 |
| 想定外の DB エラー | `Database`（sqlx エラー） | 500 |

> 存在しない `folder_id` は **事前存在チェックを既定**にして 400 を返す（FK 違反任せの 500 にしない）。02 がハード依存なので `folders` テーブルは必ず存在し、`folder_exists` は **read-only な存在確認**にすぎない（禁止される「越境**書き込み**」ではない）。

## 6. フロントエンド設計

### 6.1 ルーティング（`frontend/src/index.tsx`）
`<Route path="/manage" component={FeedManage} />` を 1 行追加（`import FeedManage from "./routes/FeedManage";`）。`<Router root={App}>` 構造は維持。

### 6.2 新規 `frontend/src/routes/FeedManage.tsx`
専用の管理ビュー。記事一覧（`FeedList.tsx`）には手を入れない。

- データ取得（局所 `createResource`、グローバルストアは使わない）:
  - `api.listFeeds()` → `Feed[]`（`folder_id` 含む）
  - `api.listFeedOverview()` → `FeedOverview[]`（`feed_id` で突合し未読 / 総数を表示）
  - `api.listFolders()` → `Folder[]`（**feature 02 提供**。フォルダ割当 Select の選択肢。02 未着手時は §8/§11）
- 表示（最小デザイン: Card 羅列ではなく **罫線区切りリスト** `divide-y divide-border`、各行 `py-3`）。各行:
  - タイトル（`text-sm font-medium`。未設定時は `url` をフォールバック表示、`text-muted-foreground`）
  - URL（`text-xs text-muted-foreground truncate`）
  - 未読バッジ（`unread_count > 0` のとき）+ 総数（`text-xs`）
  - 最終取得（`last_fetched_at` を相対表示。整形はフロント）
  - フォルダ Select（現在の `folder_id`、`null`=「未分類」）
  - 行アクション: 改名 / 再取得 / 削除
- 操作:
  - **改名**: インライン入力 or 小ダイアログ → `api.updateFeed(id, { title })` → 成功で該当行を再フェッチ or 楽観更新。空文字は送らない（送れば 400）。
  - **フォルダ割当**: Select 変更 → `api.updateFeed(id, { folder_id })`（「未分類」選択時は `null` を送る）。
  - **削除**: 既存 `dialog.tsx`（Ark UI 確認ダイアログ）→ `api.deleteFeed(id)` → 行を除去。
  - **再取得**: ボタン → `api.refreshFeed(id)` → 返却 Feed で `last_fetched_at` 更新、未読 / 総数は `listFeedOverview()` を再フェッチ。
- 変更後は `feeds` と `feedOverview` の両リソースを `refetch()`。将来 feature 10 のグローバル未読数ストア（`useApp().counts.refresh()`）が存在すれば併せて呼ぶ（§8 ソフト依存）。

### 6.3 必要な UI 部品（`components/ui/`）
**実装時に ark-ui.com（Solid）で各 part 名・props を必ず確認**（既存 `dialog.tsx` と同運用。Ark UI v5 はバージョンで compound API が変わりうるため「この通り動く」と断定しない）。

| 部品 | 実装 | 用途 | 備考 |
|------|------|------|------|
| `dialog` | 既存 | 削除確認 / 改名 | `components/ui/dialog.tsx` を再利用 |
| `button` | 既存 | 行アクション | `variant="ghost"/"destructive"`, `size="icon"/"sm"` |
| `input` | 自前 cva（未存在なら新規） | 改名入力 | `FeedList.tsx` のインライン input 相当を `components/ui/input.tsx` 化してもよい |
| `badge` | 自前 cva（未存在なら新規） | 未読件数 | oklch トークン（`bg-muted`/`text-foreground` 等）で装飾 |
| `select` | **Ark UI** Select（未存在なら新規） | フォルダ割当 | feature 02 が先に追加していれば再利用。`createListCollection`, `Select.Root/Control/Trigger/Positioner/Content/Item/ItemText`（**要確認**） |
| `dropdown-menu`（任意） | Ark UI Menu | 行アクションをまとめる場合 | 省略可。`Button` 直置きでも可 |

> `input`/`badge`/`select` は他機能（02/08/09）でも要求される共通部品。**既に `components/ui/` にあれば再利用し、無ければここで最小実装する**。新色は持ち込まず既存 oklch トークン（`bg-background`/`bg-muted`/`border-border`/`text-muted-foreground`/`bg-accent` 等）のみで装飾。重ければ Select を一旦自前 `<select>` + Tailwind で代用し、後で `select.tsx` へ昇格してよい（差し替えは 1 ファイルに閉じる）。

### 6.4 `frontend/src/lib/api.ts` への追加
型:
```ts
// 既存 Feed に folder_id を追加
export interface Feed {
  id: string;
  url: string;
  title: string | null;
  folder_id: string | null; // 追加（feature 02 の feeds.folder_id 列をミラー）
  created_at: string;
  last_fetched_at: string | null;
}

export interface FeedOverview {
  feed_id: string;
  unread_count: number;
  total_count: number;
  // feature 03 で last_published_at / posts_per_week が追記される
}
```
メソッド（`api` オブジェクトに追加。命名は既存 `動詞+リソース`（camelCase）に揃える。`http<T>()` を再利用）:
```ts
updateFeed: (id: string, body: { title?: string; folder_id?: string | null }) =>
  http<Feed>(`/api/feeds/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
refreshFeed: (id: string) =>
  http<Feed>(`/api/feeds/${id}/refresh`, { method: "POST" }),
listFeedOverview: () => http<FeedOverview[]>("/api/feeds/overview"),
```
> **`folder_id` 省略 vs null vs 値**: 据え置き＝`updateFeed(id, { title })` のように **`folder_id` キー自体を渡さない**。未分類化＝`updateFeed(id, { folder_id: null })`。割当＝`updateFeed(id, { folder_id: "<uuid>" })`。`JSON.stringify` は `undefined` のキーを出力しないので、`{ title }` だけ渡せば `folder_id` は JSON に現れず「省略＝据え置き」になる（バックエンドの double-option と整合）。
> `listFolders()` / `Folder` 型は **feature 02 が追加**する（本機能は呼ぶだけ）。

> **命名の確定（土台設計間の表記ゆれの解決）**: バックエンド土台設計は `feed_overview` / `GET /api/feeds/overview` / `FeedOverview`、フロント土台設計 §4.4 は `feed_stats` / `GET /api/feeds/stats` / `FeedStat` / `listFeedStats()` と食い違っている。**本書は `feed_overview` / `/api/feeds/overview` / `FeedOverview` / `listFeedOverview()` を確定採用とする**（バックエンド土台設計に揃え、03 が同スライスを拡張する前提と一致するため）。**feature 03 の設計書とフロント土台設計 §4.4 は、本確定に合わせて `feed_overview` 系へ更新すること**（api.ts のメソッド名 / ルートが 03・10 の期待とずれないように、いずれかが実装される前に揃える）。

### 6.5 `/manage` への導線
暫定で `App.tsx` ヘッダに `/manage` リンクを 1 行追加してよい（feature 10 の Sidebar 完成時にそちらの「管理」項目へ移し、ヘッダの暫定リンクは撤去）。App.tsx の大改造（二ペイン化）は feature 10 の責務なので、本機能ではリンク追加以上の変更を入れない。

## 7. API 契約

### 7.1 `PATCH /api/feeds/{id}`（新規・feeds 拡張）
改名 / フォルダ割当の部分更新。両フィールドとも任意。`folder_id` 機能は **0002 適用後**に有効（§4.1）。
- リクエスト（改名のみ）:
```json
{ "title": "Hacker News (front page)" }
```
- リクエスト（フォルダ割当）/（未分類へ戻す）:
```json
{ "folder_id": "7b1f0c2e-1111-2222-3333-444455556666" }
```
```json
{ "folder_id": null }
```
- リクエスト（両方同時）:
```json
{ "title": "Tech News", "folder_id": "7b1f0c2e-1111-2222-3333-444455556666" }
```
- リクエスト（`folder_id` キー省略＝据え置き）: `{ "title": "..." }`（`folder_id` を含めない）
- レスポンス `200 OK`（更新後 Feed）:
```json
{
  "id": "0c9e8a7b-...", "url": "https://news.ycombinator.com/rss",
  "title": "Hacker News (front page)", "folder_id": "7b1f0c2e-...",
  "created_at": "2026-06-20T01:02:03Z", "last_fetched_at": "2026-06-26T00:00:00Z"
}
```
- エラー: `404`（id 不在）/ `400`（空・空白のみ・200 字超 title、または存在しない folder_id）。

### 7.2 `POST /api/feeds/{id}/refresh`（挙動変更）
- 変更点: 旧実装は **全フィード** を再取得し `202 Accepted`（無ボディ、`Path` の id は無視）。新実装は **当該フィードのみ** 再取得し `200 OK` で **更新後 Feed** を返す。
- リクエストボディなし。
- レスポンス `200 OK`: §7.1 と同じ Feed JSON（`last_fetched_at` が更新済み）。
- エラー: `404`（id 不在）/ `502`（上流取得・フィードパース失敗）。
> 既存フロントは未使用（`lib/api.ts` に `refreshFeed` は未定義）なので互換破壊の影響なし。scheduler は `service::refresh_all_feeds` を直接使い続けるため無影響。

### 7.3 `GET /api/feeds/overview`（新規・feed_overview）
- リクエストなし。
- レスポンス `200 OK`（`created_at DESC` 順）:
```json
[
  { "feed_id": "0c9e8a7b-...", "unread_count": 12, "total_count": 134 },
  { "feed_id": "1a2b3c4d-...", "unread_count": 0,  "total_count": 0 }
]
```
- 記事ゼロのフィードも `0/0` の行を返す（`LEFT JOIN`）。フロントは `feed_id` で `Feed` と突合。

## 8. 依存関係

### 依存する機能（ブロッカー）
- **feature 02 (feed-folders) — ハード依存（`dependsOn: ["feed-folders"]`）**:
  - `backend/migrations/0002_folders.sql`（`feeds.folder_id` 列）。これが適用されるまで、`Feed.folder_id` を含む SELECT/RETURNING（`insert`/`list_all`/`get`/`update`）と `folder_exists` は **実行時に失敗する**（§4.1）。folder_id サーフェスは全て 0002 にハードブロックされる。
  - `GET /api/folders` と `lib/api.ts::listFolders()` / `Folder` 型（フォルダ割当 Select の選択肢）。
  - 02 非依存サブセット（title のみの改名、`refresh_one`、`feed_overview` の未読 / 総数）は先行実装可能だが、本機能はフォルダ割当を中核に含むため **機能全体としては 02 に依存** する。

### 連携（ソフト依存・本機能は非ブロック）
- **feature 03 (feed-stats)**: `feed_overview` スライスを共有。**本機能（01）がスライス新設 + merge を所有**し、03 は `last_published_at`/`posts_per_week` を **同スライスへ列追記で拡張**（merge 行は追加しない。§5.4 所有契約）。03 が先着した場合は 03 が新設し、01 はフロント消費コードのみ足す。
- **feature 10 (two-pane-layout)**: `/manage` への正式ナビ（Sidebar 項目）と未読数バッジのグローバルストアを提供。本機能は暫定リンクと局所リソースで自立し、10 完成後に導線を移管。
- **feature 08 (feed-add-placement)**: フィード追加 UI は 08 が所有。本機能の管理ビューには追加 UI を置かない。

### 本機能がブロック / 有効化するもの
- feature 03 の per-feed 集計（共有 `feed_overview` スライスの土台を 01 が用意）。
- feature 09 / Sidebar の未読数バッジ（`GET /api/feeds/overview` を再利用可能）。

## 9. テスト計画（TDD: Red → 理解 → Green。書いたら必ず実行）

> **テスト形式の注意（リポジトリの実態 vs 文章規約）**: プロジェクトの文章規約（CLAUDE.md / 体裁ルール）は「結合テストは `backend/tests/`（Rust）」と書くが、**実際のリポジトリには `backend/tests/` ディレクトリも Rust 結合テストハーネスも存在せず、`Cargo.toml` に dev-dependencies も無い**。唯一の前例は `scripts/test/api-stats.sh`（起動中スタックの nginx :8081 に対し curl で HTTP 200 + JSON キーを assert）。**本書は前例に合わせ shell スクリプトを結合テストの主とする**（パッケージ名は `rss-reader-backend`。単体テストは `cargo test -p rss-reader-backend` で走る）。レビュアはこの「規約と実態の乖離」を理由に shell スクリプト方式を却下しないこと。Rust 結合テスト導入は §11 の代替案。

### 9.1 単体テスト（`#[cfg(test)] mod tests`）

**`feeds/domain.rs` — `FeedTitle::parse`**（`FeedUrl::parse` の前例に倣い、先に Red で書く）:
1. `parse_accepts_normal_title` — 通常文字列を受理し `as_str()` が一致。
2. `parse_trims_whitespace` — 前後空白を trim して保持。
3. `parse_rejects_empty` — `""` は `Err`。
4. `parse_rejects_whitespace_only` — `"   "` は `Err`。
5. `parse_rejects_too_long` — 201 文字は `Err`（境界 200 字は OK）。
意図: 改名の不正入力（空・過長）をドメイン構築時に弾き、ハンドラの `Validation`(400) を保証する。

**`feeds/handler.rs` — `UpdateFeed` の double-option デシリアライズ**（`serde_json` は既存 dep。先に Red で書く）:
6. `update_feed_absent_folder_is_none` — `{}` をデシリアライズ → `folder_id == None`（据え置き）。
7. `update_feed_null_folder_is_some_none` — `{"folder_id": null}` → `folder_id == Some(None)`（未分類）。
8. `update_feed_value_folder_is_some_some` — `{"folder_id": "<uuid>"}` → `folder_id == Some(Some(uuid))`（割当）。
意図: **省略 / null / 値の 3 状態が確かに分かれること**を保証。これが無いと「未分類へ戻す」経路が到達不能になる（§11、レビュー指摘の中核）。`de_double_opt` が正しく機能している証跡。

### 9.2 結合テスト（`scripts/test/api-feeds.sh` を新設。`scripts/test/api-stats.sh` に倣う）
起動中スタック（`just up` または `just dev-db` + `just back`）に対し curl で検証。事前に最低 1 フィードを `POST /api/feeds` で投入するか既存データを前提にする。folder_id 系（#3）は **0002 適用後**に有効。
1. `GET /api/feeds/overview` → 200、配列、各要素に `feed_id`/`unread_count`/`total_count` キーが存在。
2. `PATCH /api/feeds/{id}`（`{"title":"renamed"}`）→ 200、レスポンス `title == "renamed"`。
3. `PATCH /api/feeds/{id}`（`{"folder_id":null}`）→ 200、`folder_id == null`（未分類化。**0002 適用後**。double-option が正しく動く証跡）。
4. `PATCH /api/feeds/{id}`（`{"title":""}`）→ 400。
5. `PATCH /api/feeds/{nonexistent-uuid}`（`{"title":"x"}`）→ 404。
6. `POST /api/feeds/{id}/refresh` → 200、Feed を返し `last_fetched_at` が非 null。
7. `POST /api/feeds/{nonexistent-uuid}/refresh` → 404。

### 9.3 フロント（手動 + 型）
- `tsc` 型チェック（`just lint`）を通す。`Feed.folder_id` 追加・新メソッドのシグネチャ整合。
- 手動: `/manage` で 一覧表示 → 改名 → フォルダ変更（未分類化含む）→ 削除（確認ダイアログ）→ 個別再取得 → 未読数の更新を目視確認。

### 9.4 lint
`just lint`（clippy `-D warnings` / tsc）を通してからコミット。`cargo fmt` / prettier 適用。

## 10. 実装手順（順序付きチェックリスト）

> **前提と順序（§4.1 再掲）**: folder_id サーフェスは feature 02 の `0002_folders.sql` 適用後にのみ追加する。推奨は **0002 を先に適用してから一括実装**。早期に rename/refresh/overview だけ出すなら、それらを **folder_id 参照ゼロ**で先行し、0002 適用後に folder_id を足す。

**バックエンド — feeds 拡張（02非依存サブセット）**
1. `feeds/domain.rs`: `FeedTitle` を追加し、`#[cfg(test)]` に §9.1 の #1–#5 を **先に Red** で書く → `cargo test -p rss-reader-backend` で失敗確認 → `parse` 実装で Green。
2. `feeds/service.rs`: `refresh_one` を追加。`feeds/handler.rs`: `refresh` を `refresh_one` 呼び出し + `Json<Feed>` 返却へ差し替え。`feeds/mod.rs`: `/api/feeds/{id}/refresh` のハンドラ参照はそのまま（パスは不変）。
3. `feeds/handler.rs`: `de_double_opt` + `UpdateFeed` + `update` ハンドラを追加。`#[cfg(test)]` に §9.1 の #6–#8 を **先に Red** で書く → Green。`AppError` を import。
4. `feeds/mod.rs`: `/api/feeds/{id}` に `.patch(handler::update)` を合成。`patch` を `use`。

**バックエンド — feed_overview 新設（本機能が所有）**
5. `backend/src/features/feed_overview/` に `domain.rs`/`repository.rs`/`service.rs`/`handler.rs`/`mod.rs` を作成（§5.3）。
6. `backend/src/features/mod.rs`: `pub mod feed_overview;` と `.merge(feed_overview::routes())` を追加（**この merge 行は 01 が単独所有。03 は追加しない**。§5.4）。

**バックエンド — folder_id サーフェス（0002 適用後にのみ実施）**
7. 0002 適用を確認後: `feeds/domain.rs` の `Feed` に `folder_id: Option<Uuid>` を追加。
8. `feeds/repository.rs`: `insert` RETURNING と `list_all` SELECT に `folder_id` を追記（02 が未追記の場合のみ。重複追加しない）。`get`/`update`/`folder_exists` を追加（§5.2）。`AppError` を import。
9. `feeds/service.rs`: `update_feed`（`folder_exists` 事前チェック含む）を追加。
10. `cargo build` / `just lint`（clippy -D warnings）/ `cargo fmt`。`cargo test -p rss-reader-backend` 全通過。

**結合テスト（スクリプト）**
11. `scripts/test/api-feeds.sh` を `api-stats.sh` に倣って作成（§9.2 の 7 観点）。スタック起動下で実行し PASS を確認（#3 は 0002 適用後）。

**フロント**
12. `lib/api.ts`: `Feed.folder_id` 追加、`FeedOverview` 型追加、`updateFeed`/`refreshFeed`/`listFeedOverview` 追加（§6.4）。
13. `components/ui/`: 不足する `input`/`badge`/`select` を最小実装（既存なら再利用）。Select は ark-ui.com で part 名確認。
14. `routes/FeedManage.tsx` を新規作成（§6.2）。削除は既存 `dialog.tsx` を再利用。
15. `index.tsx` に `/manage` ルートを追加。`App.tsx` ヘッダに暫定リンクを追加。
16. `just lint`（tsc）/ prettier。`/manage` を手動で一通り操作確認（§9.3）。

**仕上げ**
17. `just lint` 全体通過 → コミット（ユーザー指示があれば）。

## 11. リスク・未決事項・代替案

- **double-option の正しさ（レビュー指摘の中核・本書で解決済み）**: 素の serde + serde_json では `#[serde(default)] folder_id: Option<Option<Uuid>>` は **省略と `null` を区別できない**（どちらも外側 `None`。serde_json が JSON `null` を外側 Option の `visit_none` で短絡し内側へ再帰しないため）。これを放置すると「未分類へ戻す」経路（`Some(None)` → `SET folder_id = NULL`）が **到達不能**になり、§9.2 #3 が失敗する。`serde_with` は依存に無い。**本書はカスタムデシリアライザ `de_double_opt`（常に内側へ再帰）を `#[serde(default, deserialize_with = "de_double_opt")]` で適用して解決**し、§9.1 #6–#8 で 3 状態を unit test で固定する。`serde_with` クレート追加は採らない（依存を増やさない）。
- **実装順序の自己矛盾（レビュー指摘・本書で解決済み）**: 「rename/refresh/overview は 02 非依存で先行可能」と「`Feed`/`insert`/`list_all` に folder_id を足す」を同時に指示すると、0002 未適用時に `GET/POST /api/feeds` と refresh が実行時クラッシュする（`query_as` は列名でマップ）。**本書は folder_id サーフェス全体を 0002 にハードブロックし、02 非依存サブセットを folder_id 参照ゼロに分離した**（§4.1, §5.2 のタグ, §10 のフェーズ分割）。
- **`feed_overview` の所有 / 重複 merge**: スライス新設 + `.merge()` 1 行は **01 が単独所有**。03 は SELECT/FromRow を列追記するだけで merge 行を足さない（足すと axum 起動時 panic）。先着順に関わらず merge は 1 本（§5.4）。**実装着手時に 03 の進捗を確認**すること。
- **スライス名の表記ゆれ**: `feed_overview`（本書 / バックエンド土台）vs `feed_stats`（フロント土台 §4.4）。**本書が `feed_overview` 系を確定**（§6.4）。**feature 03 設計書とフロント土台 §4.4 を `feed_overview` 系へ更新する**こと。未統一のまま実装すると api.ts のメソッド名 / ルートが食い違う。
- **存在しない `folder_id` の UX**: 事前存在チェック（`folder_exists`）を **既定**にし 400（`Validation`）を返す。02 がハード依存で `folders` は必ず存在し、これは read-only 参照なので越境書き込みではない。チェックを省くと FK 違反が `Database`→500 になり、クライアント起因のエラーをサーバエラーとして返してしまう。
- **`feed_overview` の性能**: `total_count` が全記事走査になるため `idx_articles_is_read` 部分インデックスは効きにくい。記事数増大時は集計列の materialized 化（将来・新マイグレーション）へ昇格（§5.3）。**「部分インデックス活用」を最適化前提にしない**。
- **`title` の null クリア非対応**: `#[serde(default)]` のみの `title` は省略と `null` を区別しないため、「カスタムタイトルを消してフィード提供タイトルへ戻す」操作は未対応（要件外）。必要になれば `title` も `de_double_opt` で double-option 化する。
- **`refresh` の契約変更**: `202`（無ボディ）→`200`（Feed）。既存フロントは未使用のため安全だが、外部クライアントがいれば要周知。再取得は同期的（`fetch_and_store` を await）でフィードが重いと応答が遅い。将来 apalis 化（`shared/scheduler.rs` 差し替え）で非同期ジョブ + `202` へ戻す余地。
- **結合テストの形式（代替案）**: Rust 結合テスト（`backend/tests/feeds.rs`）を採るなら `Cargo.toml` に `[dev-dependencies]`（`reqwest`/`tokio`/`sqlx`）とテスト用 DB（`DATABASE_URL`）配線が必要で、これは横断的な追加投資。本書は前例（shell スクリプト）を主とする。チームで形式を統一すること（要判断）。
- **Ark UI v5 の part 名**: `Select`（任意の `Menu`）の compound part 名 / props はバージョンで変わりうる。「この通り動く」と断定せず、`dialog.tsx` の運用どおり **実装時に ark-ui.com（Solid）で確認**。重ければ自前 `<select>` で代用し後で昇格。
- **`folder_id` 列追記の担当競合**: `Feed` 構造体・`insert`/`list_all` の列リスト追記は 01 と 02 のどちらが先着しても必要。**一方が追加済みなら重複追加しない**（重複すればコンパイルエラーで気づける）。
