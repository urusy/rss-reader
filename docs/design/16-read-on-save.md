# 16 Read-on-Save（Instapaper 送付時に自動既読）— 設計書

> 対象読者: このリポジトリは clone 済みだが、この会話の文脈を知らない別セッションの実装者。本書 1 枚だけで着手・完了できるよう、再利用資産・完全な SQL・関数シグネチャ・サービス手順・API 契約・フロント変更・テスト・番号付き実装手順・リスクまで具体化する。
> **前提（ハード依存）**: 本機能は **#05 Instapaper 連携** と **#06「後で読む」** が実装済みであることを前提にする。両者が新設・追記した `instapaper` スライス（`POST /api/read-later` / `save_for_later` / `read_later_items`）へ **追記**する形の小拡張であり、新スライスは作らない（理由は §5.1）。`backend/src/features/instapaper/` と `migrations/0004_read_later.sql` が存在しない場合は着手しない。

---

## 1. 概要

「後で読む」（Instapaper 送付）に成功した記事を、設定で有効化されていれば **同時に既読にする**。`POST /api/read-later` が記事を Instapaper へ転送して `added` に確定した直後、設定フラグ `mark_read_on_save` が `true` のときだけ、既存の既読化ユースケース（`articles::service::mark_read`）を **ベストエフォートで呼ぶ**。

狙いは **未読数の膨張解消**。RSS リーダーで「あとで読む」に逃がした記事は、本人の中では「処理済み」だが未読のまま残り、未読バッジ（`GET /api/stats` の `unread` / per-feed `unread_count`）を水増しする。Instapaper に送った時点で未読から外せれば、未読一覧は「まだ手をつけていない記事」だけになり、リーダー本来の「未読を追う」体験が保たれる。

挙動はユーザーが切り替えられるべき個人設定なので、**DB 上の singleton 設定テーブル `read_later_settings`** に保持し、`/settings` 画面のトグル（Ark UI Switch）で ON/OFF する。既定は **OFF**（既存挙動を変えない安全側）。バックエンドの追加は「設定テーブル 1 つ」「設定の get/set」「`save_for_later` 成功時の 1 分岐」だけで、既読化ロジック自体は **#09 が用意した既存の `mark_read` をそのまま再利用**する。

> **AI 機能ではない点（明記）**: 本機能は LLM を一切使わない。`shared/llm`・`ANTHROPIC_API_KEY`・要約/翻訳キャッシュとは無関係で、`AppError::NotEnabled` を AI 文脈で返す箇所も無い（`NotEnabled` の用途は §5.6 のとおり Instapaper 資格情報ゲートのみで、これは #05/#06 の既存挙動）。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）

- マイグレーション **`0006_read_later_settings.sql`**（番号は暫定。§4.1 のとおり**着手前に `ls backend/migrations/` で最新番号を確認**し、最小空き整数を採る）。singleton 設定テーブル `read_later_settings`。
- `instapaper` スライスへの追記（**新スライスは作らない**）:
  - ドメイン: `ReadLaterSettings`（FromRow / Serialize）。
  - リポジトリ: 設定の取得 `get_settings` / 更新 `set_mark_read_on_save` / 真偽ヘルパ `mark_read_on_save_enabled`。
  - サービス: 設定の取得 `get_read_later_settings` / 更新 `update_read_later_settings`、および **`save_for_later` の成功分岐に read-on-save を 1 ブロック追記**。
  - ハンドラ + ルート: `GET /api/read-later/settings`、`PUT /api/read-later/settings`。
- 既読化は **#09 の既存 `crate::features::articles::service::mark_read(state, id, true)` を再利用**（articles アグリゲートの書き込みを越境 SQL で複製しない。§5.4）。
- フロント: `lib/api.ts` に型 `ReadLaterSettings` + メソッド 2 つ。`routes/Settings.tsx` に Instapaper セクション内のトグル（`components/ui/switch.tsx` を再利用）。任意で `ArticleView` の保存成功時に楽観的に未読カウントを減らす（§6.3）。
- テスト: `ReadLaterSettings` の serde 単体テスト、`repository` の往復テスト（実 DB・`#[ignore]`）、`scripts/test/read-on-save.sh`（HTTP スモーク）。

### 非スコープ（本機能では作らない）

- Instapaper 連携本体・資格情報 UI・`add` 呼び出し（#05）、`read_later_items` テーブル・`save_for_later` の冪等/失敗トラッキング（#06）。本機能はこれらを**新規作成せず**、`save_for_later` の成功分岐に追記するだけ。
- 既読化ロジックそのもの（#09 の `mark_read` / `set_read` を再利用。新規 SQL を書かない）。
- 「保存解除で未読に戻す」逆操作。read-on-save は前進方向のみ。
- 一覧 API への保存状態の JOIN や、per-article の未読再計算 API（既存 `GET /api/stats` / per-feed `unread_count` の再取得で足りる）。
- 設定の複数プロファイル化・per-feed 設定。単一ユーザ前提で singleton 1 行。
- バックエンドのライブラリターゲット化（`src/lib.rs` 追加）。クレートは binary のみのままにする（テスト方式は §9）。

---

## 3. 既存実装の調査と再利用（車輪の再発明をしない）

実ファイルを確認済み。再利用する資産:

| 資産 | 場所（確認済み） | 16 での使い方 |
|------|------|----------------|
| **`save_for_later`（#06）** | `backend/src/features/instapaper/service.rs:33`（`Ok(()) => repository::mark_added(...)` 分岐あり） | この **成功分岐に read-on-save を 1 ブロック追記**。失敗分岐・冪等短絡は触らない（§5.4） |
| **`mark_read` / `set_read`（#09 / 既存）** | `articles/service.rs:28`（`pub async fn mark_read(state, id, read) -> AppResult<()>`）→ `articles/repository.rs:88`（`set_read`、0 件で `NotFound`） | read-on-save の既読化はこれを**そのまま呼ぶ**。articles の `is_read` 書き込み所有を移さない |
| **`ArticleId` newtype** | `articles/domain.rs`（`#[sqlx(transparent)]`, `Copy`, `Serialize`） | `save_for_later` は既に `ArticleId` を受けている。そのまま `mark_read` へ渡す |
| **singleton 設定テーブルの前例** | `migrations/0003_instapaper.sql`（`instapaper_credentials`、`id INTEGER PK DEFAULT 1 CHECK (id = 1)` + `ON CONFLICT (id) DO UPDATE`） | `read_later_settings` を同型の singleton で作る（§4.2） |
| **設定の get/upsert リポジトリ前例** | `instapaper/repository.rs`（`get_credentials` = `fetch_optional`、`upsert_credentials` = `INSERT ... ON CONFLICT (id) DO UPDATE`） | `get_settings` / `set_mark_read_on_save` を同型で書く（§5.3） |
| **`AppState { db, config, http }`** | `shared/state.rs`（`#[derive(Clone)]`） | `save_for_later` / 新サービスは既に `&AppState` を取る。追加配線なし |
| **`AppError`（不編集）** | `shared/error.rs`（6 バリアント、`IntoResponse` で `{"error": <Display>}`） | 新バリアントを足さない（§5.6） |
| **runtime クエリ + FromRow** | `instapaper/repository.rs` / `articles/repository.rs`（`query` / `query_as::<_, T>`） | `query!` マクロ不使用。すべて runtime クエリ |
| **`instapaper/mod.rs` の `routes()`** | `instapaper/mod.rs:11`（`/api/read-later`・`/api/read-later/{article_id}` を所有） | `/api/read-later/settings` を 2 メソッド分追記。`features/mod.rs` は不変 |
| **自動マイグレーション実行** | `main.rs` 起動時 `db::run_migrations` → `sqlx::migrate!("./migrations")` | ファイルを置くだけで適用。番号順に注意（§4.1） |
| **`http<T>()`（204 畳み込み）** | `frontend/src/lib/api.ts`（204→undefined、非 2xx は `Error("${status} ...")`） | 既存ヘルパで設定 get/put を呼ぶ |
| **`Switch`（Ark UI ラップ）** | `frontend/src/components/ui/switch.tsx`（CHEATSHEET 記載・#04 ダークテーマで導入済み想定） | read-on-save トグルに再利用。新規部品は作らない |
| **`Settings.tsx`（#05）** | `frontend/src/routes/Settings.tsx`（Instapaper 資格情報フォーム） | Instapaper セクションにトグル行を 1 つ追記 |
| **HTTP スモークの前例** | `scripts/test/api-stats.sh`（nginx `:8081` へ curl、HTTP/JSON を assert） | `scripts/test/read-on-save.sh` を同型で新設（§9.3） |

### 確認した重要な事実（設計に影響）

- `save_for_later`（#06）は既に「冪等短絡 → 記事参照（`NotFound`）→ 資格情報（`NotEnabled`）→ `upsert_pending` → 転送 → `mark_added`/`mark_failed`」の順で実装済み。read-on-save は **`mark_added` が成功した直後（新規追加が確定したとき）だけ**動かせばよい。冪等短絡（既に `added`）で早期 return する経路では既読化を**やり直さない**（初回保存時に既読化済みのはずで、再 POST で既読を蒸し返さない）。
- クレートは **binary のみ**（`src/lib.rs` 無し）。`backend/tests/` から `service`/`repository` 内部関数は import できない。よって in-crate `#[cfg(test)]`（serde + リポジトリ往復）と curl スモークで担保する（§9）。
- 既読化（`set_read`）は記事が無いと `NotFound` を返すが、read-on-save 到達時点で記事は存在確認済み。万一の競合削除でも **read-later の成功（`added`）を巻き戻さない**ため、既読化失敗は伝播させずログのみにする（§5.4）。

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方（必読）

`main.rs` の `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を呼ばないため、**適用済み最大バージョンより小さい未適用マイグレーションを後から足すと起動時に out-of-order エラー**になる（家庭内永続 DB で実害）。

**ルール**:
- **着手前に必ず `ls backend/migrations/` で最新番号を確認**し、`+1`（最小空き整数）を採る。本書では暫定で **`0006_read_later_settings.sql`** と表記する（現状の最新は `0005_search.sql`）。apalis 移行など並行作業が先に `0006` を取っていたら、本ファイルを `0007` 以降へ繰り上げる。
- 既存マイグレーション（`0001`〜`0005`）は**編集しない**（追記のみ）。

### 4.2 スキーマ

新規ファイル **`backend/migrations/0006_read_later_settings.sql`**:

```sql
-- Read-on-Save settings. Single-user app => singleton row pinned to id = 1.
-- When mark_read_on_save = true, a successful POST /api/read-later also marks
-- the article as read (articles.is_read = true) to keep unread counts honest.
-- Default is false to preserve existing behavior until the user opts in.
CREATE TABLE IF NOT EXISTS read_later_settings (
    id                INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    mark_read_on_save BOOLEAN NOT NULL DEFAULT false,
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Seed the singleton row so reads can assume exactly one row exists.
INSERT INTO read_later_settings (id, mark_read_on_save)
VALUES (1, false)
ON CONFLICT (id) DO NOTHING;
```

設計判断:
- **env ではなく DB に置く理由**: ユーザーが UI から実行時に切り替える個人設定だから（オペレーター設定の `ANTHROPIC_API_KEY` とは性質が異なる。`instapaper_credentials` と同型の判断）。env フラグ案との比較は §11。
- **singleton（`id INTEGER PK DEFAULT 1 CHECK (id = 1)`）**: 単一ユーザなので 1 行。`ON CONFLICT (id) DO UPDATE` で「無ければ挿入、有れば更新」を 1 クエリで表現でき行数管理が不要。`instapaper_credentials`（0003）と完全に同じパターン。
- **行をシードする理由**: `get_settings` を `fetch_one` で書けて分岐が減る（行が必ず 1 つ）。`fetch_optional` + 既定値フォールバックでも可だが、シードして単純化する。リポジトリ側は**防御的に**「行が無ければ既定 false」も扱う（§5.3）。
- 既定 **false**: 導入で既存挙動を変えない安全側。ユーザーが明示的に ON にしたときだけ未読が減る。

`articles` / `read_later_items` への列追加は無い（read-on-save の関心を他テーブルに漏らさない）。

---

## 5. バックエンド設計

`instapaper` スライスへ**追記**する。**新スライスは作らない。**

### 5.1 なぜ新スライスにしないか（境界の根拠）

read-on-save の本質は「`save_for_later` の成功時の副作用」であり、`POST /api/read-later`（= `instapaper` スライス所有）と不可分に凝集する。別スライスにすると、(a) `save_for_later` の内部成功イベントを外へ公開する仕組みが要り、(b) 設定テーブルの所有者が宙に浮く。土台設計は「新機能 = 新スライス」を原則としつつ「**同一アグリゲートへの書き込み拡張は正当化される**」と定める（README §アーキテクチャ準拠、#06・#09 が同じ判断で in-slice 追記している前例）。本機能は read-later アグリゲートの内部拡張なので、#06 と同じく `instapaper` スライスに同居させる。これにより `features/mod.rs`（`.merge(instapaper::routes())` は #05 が追加済み）は**不変**。

> 既読化（`articles.is_read` 書き込み）だけは articles アグリゲートの所有物なので、自前 SQL で複製せず **articles スライスの公開ユースケース `mark_read` を呼ぶ**（§5.4）。これは「越境**書き込み**の禁止」（#09 §5）を侵さない: 書き込みの実体は articles スライス内に閉じ、instapaper は公開関数を 1 つ呼ぶだけ。

### 5.2 domain（`instapaper/domain.rs` に追記）

```rust
use serde::Serialize;

/// read_later_settings 1 行をミラーする。Read-on-Save の ON/OFF を保持。
/// GET /api/read-later/settings がそのまま返す安全な射影（秘密情報を含まない）。
#[derive(Debug, Clone, Copy, Serialize, sqlx::FromRow)]
pub struct ReadLaterSettings {
    pub mark_read_on_save: bool,
}

impl Default for ReadLaterSettings {
    /// 行が無い場合の既定（OFF）。リポジトリの防御的フォールバックで使う。
    fn default() -> Self {
        Self { mark_read_on_save: false }
    }
}
```

> `bool` 1 個なので値オブジェクトや newtype は過剰。`FromRow` で `read_later_settings` の `mark_read_on_save` 列だけを射影する（`SELECT mark_read_on_save FROM ...`）。

### 5.3 repository（`instapaper/repository.rs` に追記）

すべて `&PgPool` を取る自由関数、runtime クエリのみ。

```rust
use super::domain::ReadLaterSettings;
// （既存の use: sqlx::PgPool, uuid::Uuid, crate::shared::error::AppResult 等はファイル冒頭に揃っている）

/// 設定を取得。シード済み singleton 行を読む。万一行が無ければ既定（OFF）。
pub async fn get_settings(pool: &PgPool) -> AppResult<ReadLaterSettings> {
    let row = sqlx::query_as::<_, ReadLaterSettings>(
        "SELECT mark_read_on_save FROM read_later_settings WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.unwrap_or_default())
}

/// Read-on-Save の ON/OFF を更新（singleton upsert）。
pub async fn set_mark_read_on_save(pool: &PgPool, enabled: bool) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO read_later_settings (id, mark_read_on_save, updated_at)
           VALUES (1, $1, now())
           ON CONFLICT (id) DO UPDATE
             SET mark_read_on_save = EXCLUDED.mark_read_on_save,
                 updated_at = now()"#,
    )
    .bind(enabled)
    .execute(pool)
    .await?;
    Ok(())
}

/// save_for_later の成功分岐から呼ぶ真偽ヘルパ（read を 1 関数に閉じる）。
pub async fn mark_read_on_save_enabled(pool: &PgPool) -> AppResult<bool> {
    Ok(get_settings(pool).await?.mark_read_on_save)
}
```

> `query!` コンパイル時マクロは使わない（ビルドに DB 接続が要るため）。すべて `query` / `query_as`。

### 5.4 service（`instapaper/service.rs` の `save_for_later` 成功分岐に追記 + 新サービス 2 つ）

**(a) `save_for_later` の成功分岐に read-on-save を追記。** 既存コード（§3 で確認した `service.rs:54-60`）の `Ok(())` ブランチだけを差し替える。冪等短絡・`NotFound`・`NotEnabled`・失敗分岐は**一切触らない**。

変更前（現状）:
```rust
    match send_to_instapaper(state, &creds, &url, Some(article.title)).await {
        Ok(()) => repository::mark_added(&state.db, id).await,
        Err(e) => {
            let _ = repository::mark_failed(&state.db, id, &e.to_string()).await;
            Err(AppError::Upstream(format!("instapaper add failed: {e}")))
        }
    }
```

変更後:
```rust
    match send_to_instapaper(state, &creds, &url, Some(article.title)).await {
        Ok(()) => {
            let item = repository::mark_added(&state.db, id).await?;
            // Read-on-Save (#16): 設定が ON のときだけ、保存と同時に既読化する。
            // 未読数の膨張を防ぐのが目的。既読化は articles スライスの公開ユースケースを
            // 再利用し、is_read の書き込み所有を移さない。
            // ベストエフォート: 既読化に失敗しても read-later の成功(added)は巻き戻さない。
            if repository::mark_read_on_save_enabled(&state.db)
                .await
                .unwrap_or(false)
            {
                if let Err(e) =
                    crate::features::articles::service::mark_read(state, id, true).await
                {
                    tracing::warn!(
                        error = %e,
                        article_id = %id.0,
                        "read-on-save: failed to mark article read"
                    );
                }
            }
            Ok(item)
        }
        Err(e) => {
            let _ = repository::mark_failed(&state.db, id, &e.to_string()).await;
            Err(AppError::Upstream(format!("instapaper add failed: {e}")))
        }
    }
```

設計上の要点:
- **成功（新規 `added`）時のみ**実行。冪等短絡（既に `added`）で早期 return する経路では既読化をやり直さない（§3）。
- **設定 read は ON のときだけ意味を持つ**ので成功分岐内に置く。`unwrap_or(false)` で「設定 read が失敗しても read-later は壊さない」フェイルセーフ。
- **既読化は `articles::service::mark_read(state, id, true)` を再利用**。articles の `is_read` 書き込みは articles スライス内に閉じたまま（越境書き込み回避。§5.1）。
- **ベストエフォート**: `mark_read` の `Err`（競合削除による `NotFound` 等）は伝播させず `tracing::warn!` のみ。read-later の成功（`added`）を 1 つの副作用失敗で 502 に化けさせない。レスポンスは従来どおり `ReadLaterItem`（200）。
- `use` 追加は不要（フルパス `crate::features::articles::service::mark_read` で呼ぶ）。`articles` スライスを import する前例は既に多数（`ArticleId` 等）。

**(b) 設定の取得・更新サービスを追記。**
```rust
use super::domain::ReadLaterSettings;
// （既存の use: super::repository, crate::shared::{error::AppResult, state::AppState} を利用）

pub async fn get_read_later_settings(state: &AppState) -> AppResult<ReadLaterSettings> {
    repository::get_settings(&state.db).await
}

pub async fn update_read_later_settings(
    state: &AppState,
    mark_read_on_save: bool,
) -> AppResult<ReadLaterSettings> {
    repository::set_mark_read_on_save(&state.db, mark_read_on_save).await?;
    repository::get_settings(&state.db).await
}
```

### 5.5 handler（`instapaper/handler.rs` に追記）

```rust
use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use super::domain::ReadLaterSettings;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

// GET /api/read-later/settings -> 200 Json<ReadLaterSettings>
pub async fn get_settings(
    State(state): State<AppState>,
) -> AppResult<Json<ReadLaterSettings>> {
    Ok(Json(service::get_read_later_settings(&state).await?))
}

#[derive(Debug, Deserialize)]
pub struct SettingsBody {
    pub mark_read_on_save: bool,
}

// PUT /api/read-later/settings -> 200 Json<ReadLaterSettings>（更新後を返す）
pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsBody>,
) -> AppResult<Json<ReadLaterSettings>> {
    Ok(Json(
        service::update_read_later_settings(&state, body.mark_read_on_save).await?,
    ))
}
```

> `mark_read_on_save` は必須フィールド（`#[serde(default)]` を付けない）。欠落 / 非 bool は axum `Json` 抽出器が 422 を返す既存挙動に委ねる。`save_for_later` ハンドラ（#06）は不変。

### 5.6 routes（`instapaper/mod.rs` の `routes()` に 1 行追記）

```rust
use axum::routing::{get, post, put}; // put は既存（credentials で使用済み）

// ...routes() 内、既存の read-later ルート群に追加...
        .route(
            "/api/read-later/settings",
            get(handler::get_settings).put(handler::update_settings),
        )
```

> **ルート衝突に関する注意**: 既存 `/api/read-later/{article_id}`（capture）と新規 `/api/read-later/settings`（静的セグメント）は同じ位置に capture と静的が並ぶ。axum 0.8 同梱の matchit 0.8 は **静的セグメントを優先**し衝突とみなさない（"static route ... takes precedence"）。登録順は結果に影響しない。`settings` は UUID としてパースされ得ない literal なので `get_read_later_one`（`{article_id}`）とも混同しない。
> `features/mod.rs` は**不変**（`.merge(instapaper::routes())` は #05 が追加済み）。

### 5.7 AppError の使い分け（`error.rs` 不編集）

| 状況 | バリアント | HTTP | 補足 |
|------|-----------|------|------|
| read-on-save の既読化が `NotFound` 等で失敗 | （エラーにしない） | — | `tracing::warn!` のみ。read-later の 200 を保つ（§5.4 ベストエフォート） |
| `PUT /settings` の body が欠落 / 非 bool | （axum `Json` 抽出器） | 422 | 既存挙動。明示の `Validation` は不要 |
| 設定 / 既読化の DB 障害 | `Database`（`?` 経由） | 500 | `{"error":"internal error"}` |

新バリアントは追加しない。`save_for_later` の既存エラー（`NotFound`/`NotEnabled`/`Upstream`）は #06 のまま不変。

---

## 6. フロントエンド設計

### 6.1 `lib/api.ts`（型 + メソッド 2 つ）

型を追加（backend JSON をミラー）:
```ts
export interface ReadLaterSettings {
  mark_read_on_save: boolean;
}
```

`api` オブジェクトにメソッド追加（既存 `http<T>()` を再利用、命名は既存の「動詞+リソース」に揃える）:
```ts
  getReadLaterSettings: () =>
    http<ReadLaterSettings>("/api/read-later/settings"),
  setReadLaterSettings: (mark_read_on_save: boolean) =>
    http<ReadLaterSettings>("/api/read-later/settings", {
      method: "PUT",
      body: JSON.stringify({ mark_read_on_save }),
    }),
```

### 6.2 `routes/Settings.tsx`（Instapaper セクションにトグル 1 行）

#05 が作成済みの `Settings.tsx` の Instapaper セクション（Card）に、read-on-save トグルを 1 行追記する。状態はローカル（`createResource` で取得、`createSignal` 不要 — Switch の値は resource から引く）。

骨子:
- `const [settings, { mutate, refetch }] = createResource(api.getReadLaterSettings);`
- `components/ui/switch.tsx` の `Switch` を使い、`checked={settings()?.mark_read_on_save ?? false}`。
- `onCheckedChange`（Ark UI Switch のコールバック名は実装時に ark-ui.com で要確認。CHEATSHEET の `switch.tsx` ラッパ API に合わせる）で:
  ```tsx
  const onToggle = async (next: boolean) => {
    // 楽観更新 → 失敗時にロールバック
    const prev = settings();
    mutate({ mark_read_on_save: next });
    try {
      const updated = await api.setReadLaterSettings(next);
      mutate(updated);
    } catch (e) {
      mutate(prev);
      alert(`設定の更新に失敗しました: ${String(e)}`);
    }
  };
  ```
- ラベル: 「Instapaper に送ったら自動で既読にする」、補足: `text-xs text-muted-foreground` で「後で読むに送った記事を未読一覧から外し、未読数の膨張を防ぎます」。
- `busy`（resource ローディング中）は Switch を `disabled` にしてもよい。

> #05/#06 と非干渉。Instapaper Card 内に行を 1 つ足すだけ。`Settings.tsx` が無い環境（#05 未実装）では着手前提が崩れるので着手しない（§前提）。

### 6.3 未読カウント・一覧への即時反映（任意・推奨）

read-on-save はサーバ側で `is_read=true` にするため、**次回の一覧/カウント再取得で自然に反映**される。即時性が欲しい場合のみ、`ArticleView`（#06 が「後で読む」ボタンを置いた場所）で保存成功後に楽観更新する:

- `api.saveForLater(id)` 成功後、`settings.mark_read_on_save` が `true` なら:
  - `store.markReadLocal(id)`（`lib/store.tsx` の既存 `markReadLocal`、CHEATSHEET 記載）でセッション内既読集合へ追加 → 一覧の見た目が即「既読」に。
  - 未読カウントは `useApp().counts?.refresh()`（#09/#10 のグローバル `counts`）または `getStats()` の再取得で反映。
- 設定値は `ArticleView` でも `createResource(api.getReadLaterSettings)` で引くか、保存レスポンス（`ReadLaterItem`）には設定が含まれないため、**判定をサーバに委ねて単に refetch するだけ**でも要件は満たす。低 effort 実装では §6.3 全体を省略し「操作後に一覧 + カウントを refetch」で十分。

### 6.4 必要な Ark UI 部品

- `Switch` のみ（`components/ui/switch.tsx` を再利用。#04 ダークテーマで導入済みの想定。未導入なら #04 のラッパを先に入れるか、本機能で薄くラップする — Ark UI Switch を `bg-accent`/`bg-input` トークンで装飾）。
- 新規の複雑 a11y 部品は不要。

---

## 7. API 契約

> すべて `/api` プレフィックス。本文エラーは `shared/error.rs` の `{"error": <message>}` 形式。

### 7.1 `GET /api/read-later/settings` — Read-on-Save 設定の取得
レスポンス（200）:
```json
{ "mark_read_on_save": false }
```

### 7.2 `PUT /api/read-later/settings` — Read-on-Save 設定の更新
リクエスト:
```json
{ "mark_read_on_save": true }
```
レスポンス（200、更新後を返す）:
```json
{ "mark_read_on_save": true }
```
エラー:
- 422 `{ "error": "..." }`（`mark_read_on_save` 欠落 / 非 bool。axum `Json` 抽出器由来）
- 500 `{ "error": "internal error" }`（DB 障害）

### 7.3 `POST /api/read-later` — 既存契約は不変（挙動だけ拡張）
リクエスト / 成功レスポンス / エラーは **#06 のまま**（`{article_id}` → 200 `ReadLaterItem` / 404 / 503 / 502）。**契約は変えない。** 変わるのは「設定 ON かつ新規 `added` 成功時に、サーバが内部で当該記事を既読化する」副作用のみ。既読化の成否はレスポンスに**現れない**（ベストエフォート、§5.4）。

```json
// 成功時（#06 と同一。is_read はこの応答に含まれない＝articles 側で反映される）
{
  "article_id": "0f8b2c4e-1111-2222-3333-444455556666",
  "status": "added",
  "instapaper_added_at": "2026-06-30T12:34:56Z",
  "last_error": null,
  "created_at": "2026-06-30T12:34:56Z",
  "updated_at": "2026-06-30T12:34:56Z"
}
```

---

## 8. 依存関係

### 依存する（先行が必須・ブロッカー）
- **#05 Instapaper 連携**（`instapaper` スライス・`migrations/0003`・`Settings.tsx`）。
- **#06「後で読む」**（`save_for_later` / `read_later_items` / `migrations/0004` / `POST /api/read-later`）。本機能はこの成功分岐に追記する。
- **#09 既読管理**（`articles::service::mark_read`）。read-on-save の既読化に再利用。`mark_read` は #09 以前から既存（`articles/service.rs:28`）なので、厳密には #09 完了前でも `mark_read` 自体は存在するが、未読カウント整合の UI（counts / per-feed バッジ）が活きるのは #09/#10 後。

### ソフトな統合点（ハード依存ではない）
- **#04 ダークテーマ**（`components/ui/switch.tsx` の供給元）。Switch が未導入なら本機能で薄くラップ。
- **#10 二ペイン / #09 counts**（§6.3 の即時カウント反映）。無くても操作後の refetch で成立。

### ブロックする（本機能が前提になる先）
- 特になし（末端機能）。

`features/mod.rs` への変更は**無し**（`.merge(instapaper::routes())` は #05 が追加済み。本機能はスライス内に閉じる）。

---

## 9. テスト計画（TDD: Red → 理解 → Green）

テストは 3 層。**(A) 純粋ロジック（serde）の単体テスト**、**(B) リポジトリ往復の自動テスト（実 DB・`#[ignore]`）**、**(C) HTTP スモーク（curl）**。クレートは binary のみのため、`save_for_later` のオーケストレーション（成功時に既読化する分岐）は (C) + §9.4 手動 E2E で担保する（lib 化は非スコープ）。

### 9.1 単体テスト（`instapaper/domain.rs` の `#[cfg(test)] mod tests` に追記、DB 不要）

| テスト | 意図 |
|--------|------|
| `settings_default_is_off` | `ReadLaterSettings::default().mark_read_on_save == false`（既定 OFF の回帰防止） |
| `settings_serializes_to_expected_json` | `serde_json::to_string(&ReadLaterSettings{mark_read_on_save:true})` が `{"mark_read_on_save":true}`（API 契約 §7.1 と一致） |

`handler.rs` の `SettingsBody`（`#[cfg(test)] mod tests`）:

| テスト | 意図 |
|--------|------|
| `settings_body_parses_bool` | `{"mark_read_on_save":true}` が `mark_read_on_save==true` にデコード |
| `settings_body_rejects_missing` | `{}` のデコードが `Err`（必須フィールド。実機では axum が 422 に変換） |
| `settings_body_rejects_non_bool` | `{"mark_read_on_save":"yes"}` が `Err` |

### 9.2 リポジトリ往復テスト（`instapaper/repository.rs` の `#[cfg(test)] mod tests`、実 DB / `#[ignore]`）

`DATABASE_URL`（`just dev-db` の DB、`0006` 適用済み）に接続。`#[tokio::test]` + `#[ignore]`（`cargo test -- --ignored` で実行）。#05 の `credentials_roundtrip_*` と同じ雛形。

| テスト | 意図 |
|--------|------|
| `settings_default_when_seeded_is_off` | 初期状態（シード行）で `get_settings` が `mark_read_on_save==false` |
| `settings_set_true_then_get` | `set_mark_read_on_save(true)` → `get_settings` が `true`、`mark_read_on_save_enabled` も `true` |
| `settings_set_false_then_get` | `true` の後に `set_mark_read_on_save(false)` → `false` に戻る（singleton 更新確認） |

雛形（#05 の pool() ヘルパに倣う）:
```rust
#[tokio::test]
#[ignore = "requires a running Postgres (DATABASE_URL)"]
async fn settings_set_true_then_get() {
    let pool = pool().await;
    set_mark_read_on_save(&pool, true).await.unwrap();
    assert!(get_settings(&pool).await.unwrap().mark_read_on_save);
    assert!(mark_read_on_save_enabled(&pool).await.unwrap());
    // 後始末: 既定へ戻す
    set_mark_read_on_save(&pool, false).await.unwrap();
}
```

### 9.3 HTTP スモーク（`scripts/test/read-on-save.sh`、`api-stats.sh` に倣う・nginx `:8081`）

設定エンドポイントの疎通と契約を curl で検証（Instapaper 本体は叩かない）。**Red 先行**: スクリプトを先に書くと現状エンドポイント不在で 404 → 実装後 PASS。

| 手順 / アサーション | 意図 |
|---|---|
| `GET /api/read-later/settings` → 200、JSON に `mark_read_on_save` キー | スライス合成 + 取得疎通 |
| `PUT /api/read-later/settings` body `{"mark_read_on_save":true}`（`-H 'Content-Type: application/json'`）→ 200、`mark_read_on_save:true` | 更新 + 更新後返却 |
| 直後の `GET` → `mark_read_on_save:true` | 永続化確認 |
| `PUT` body `{}` → 422 | 必須フィールド欠落の拒否（axum `Json`） |
| 後始末: `PUT {"mark_read_on_save":false}` → 200 | DB を既定へ戻す（後続テストへの副作用防止） |

> read-on-save の**副作用（既読化）の E2E** はライブ Instapaper 送信が要るため自動 CI に含めない。手動手順は §9.4 / §10。

### 9.4 フロント + 手動 E2E
- `tsc`（`just lint`）で `ReadLaterSettings` / 2 メソッド / `Settings.tsx` の型整合。
- 手動 E2E:
  1. `/settings` で read-on-save トグルを ON → `GET` で `true` を確認。
  2. 未読記事を開き「後で読む」→ 200 `added`。当該記事が既読になり、未読バッジ（`/api/stats` / per-feed）が 1 減ることを確認。
  3. トグル OFF → 別の未読記事を「後で読む」→ 既読化されず未読のまま（従来挙動）。
  4. （競合確認・任意）既読化が失敗してもボタン操作はエラーにならない（read-later は 200 のまま、サーバログに warn）。

書いたテストは必ず実行し、`just lint`（clippy `-D warnings` + tsc）を通す。

---

## 10. 実装手順（順序付きチェックリスト）

1. **前提確認**: `backend/src/features/instapaper/`、`migrations/0003_instapaper.sql`・`0004_read_later.sql`、`articles/service.rs::mark_read`、`frontend/src/routes/Settings.tsx` が存在することを確認。欠けていれば #05/#06/#09 を先に。
2. **マイグレーション番号採番**: `ls backend/migrations/` で最新番号を確認し `+1`（暫定 `0006`）。
3. **マイグレーション作成**: 採番したファイルを §4.2 の SQL で新規作成（既存は触らない）。
4. **domain（Red 先行）**: `instapaper/domain.rs` に `ReadLaterSettings` + `Default` を追記。§9.1 の `#[cfg(test)]`（serde）を先に書いて Red→Green。
5. **repository**: `instapaper/repository.rs` に `get_settings` / `set_mark_read_on_save` / `mark_read_on_save_enabled` を追記（§5.3）。§9.2 の `#[ignore]` 往復テストも書く。
6. **service**: `instapaper/service.rs` の `save_for_later` の `Ok(())` 分岐を §5.4(a) に差し替え（**他の分岐は触らない**）。`get_read_later_settings` / `update_read_later_settings`（§5.4(b)）を追記。
7. **handler**: `instapaper/handler.rs` に `get_settings` / `SettingsBody` / `update_settings`（§5.5）。§9.1 の `SettingsBody` テストを追記。
8. **routes**: `instapaper/mod.rs` の `routes()` に `/api/read-later/settings`（§5.6）。`features/mod.rs` は不変。
9. **ビルド & lint**: `cargo fmt` → `just lint`（clippy `-D warnings`）。
10. **DB 起動 & マイグレーション**: `just dev-db` →（起動時自動 migrate、または `just migrate`）。
11. **リポジトリ往復テスト**: `DATABASE_URL=... cargo test -- --ignored` で §9.2 を Green に。
12. **HTTP スモーク**: `scripts/test/read-on-save.sh` を §9.3 で作成・`chmod +x`・実行（稼働スタックに対し 200/422 を assert、後始末で OFF へ戻す）。
13. **フロント**: `lib/api.ts` に型 + 2 メソッド（§6.1）。`Settings.tsx` にトグル行（§6.2）。任意で `ArticleView` の即時反映（§6.3）。`just lint`（tsc）。
14. **手動 E2E**: §9.4 の 1〜4。実 Instapaper 資格情報で ON→保存→既読化を目視。
15. **コミット**: マイグレーション・スライス追記・スクリプト・フロントをまとめて。秘密情報 / `.env` はコミットしない。

---

## 11. リスク・未決事項・代替案

- **マイグレーション番号の順序ハザード**: §4.1 のとおり `run_migrations` は out-of-order を許さない。**着手直前に `ls backend/migrations/` で最新番号を確認**し最小空き整数を採る（暫定 `0006`）。apalis 移行など並行作業が先に取っていたら繰り上げる。
- **設定の置き場所（DB vs env、決定済み: DB）**: ユーザーが UI から切り替える個人設定なので DB singleton にした（`instapaper_credentials` と同型）。**代替案=env フラグ** `INSTAPAPER_MARK_READ_ON_SAVE`（`config.rs` に `bool` フィールド追加、`save_for_later` で `state.config` を読む）。env 案はマイグレーション不要で最小だが、(a) UI から切り替えられない（再起動が要る）、(b) オペレーター設定とエンドユーザー設定が混ざる、ため不採用。要件「設定フラグで切替」を UI トグルで満たすため DB を採る。
- **既読化のベストエフォート（決定済み）**: `mark_read` 失敗を read-later の失敗に昇格させない（§5.4）。理由: 記事は既に Instapaper に保存済み（`added` 確定）で、未読フラグを落とせなかっただけで 502 を返すのは UX 上不適切。代償として「設定 ON なのに稀に既読化されない」ことがあり得るが、ユーザーは再度「後で読む」を押せば（冪等短絡で `added` 即返しになり既読化はスキップされる点に注意 — §下記）解消しづらい。**未決**: 「冪等短絡経路でも設定 ON なら既読化を試みる」かは要検討。本書は MVP として**新規 `added` 時のみ**既読化する（再 POST では既読化しない）。膨張解消の主目的は初回保存で達成されるため許容。厳密化したい場合は冪等短絡の return 直前にも同じ read-on-save ブロックを置く（重複コードになるので小ヘルパ関数に切り出す）。
- **冪等短絡と read-on-save の相互作用**: `save_for_later` は既に `added` の記事で早期 return する。よって「保存後に手動で未読へ戻し、再度保存して既読化し直す」はできない（再 POST は Instapaper も既読化もスキップ）。これは #06 の冪等設計の帰結で、read-on-save 単体の問題ではない。必要なら上記「厳密化」を採る。
- **設定取得の追加クエリ**: read-on-save ブロックは成功時に `get_settings`（1 往復）を足す。単一ユーザ・低頻度操作では無視できる。気になれば `save_for_later` の冒頭で 1 回だけ読んでクロージャに渡す最適化が可能だが、可読性優先で成功分岐内 read のままにする。
- **`switch.tsx` の供給（#04 依存）**: Switch ラッパが未導入なら本機能がブロックされる。緩和: #04 未了なら本機能で Ark UI Switch を薄くラップ（`bg-accent`/`bg-input` トークン）。Ark UI v5 の Switch part 名・`onCheckedChange` 等の props は **実装時に ark-ui.com（Solid）で要確認**（断定しない）。
- **未読カウントの即時整合**: サーバ側で `is_read` を更新するため、フロントが何もしなければ次回 refetch まで一覧/バッジは古いまま。§6.3 の楽観更新（`markReadLocal` + `counts.refresh()`）で緩和。低 effort 実装では省略し refetch に委ねてよい。
- **AI 非該当**: 本機能は LLM を使わない（§1）。`ANTHROPIC_API_KEY` / `shared/llm` / DB キャッシュの要件は適用外。`AppError::NotEnabled` は #05/#06 の Instapaper 資格情報ゲートのみで使われ、本機能で新たに返す箇所は無い。
