# 06 「後で読む」機能（Instapaper 連携）— 設計書

> 対象読者: このリポジトリは clone 済みだが、この会話の文脈を知らない別セッションの実装者。
> この機能は **#05 instapaper-integration の完了を前提（ハード依存）にする**。05 が新設する `instapaper` スライスへ追記する形で実装し、新スライスは作らない（後述の根拠あり）。
> **05 が「マイグレーション 0003（`instapaper_credentials`）＋ `load_credentials` ＋ `add_url`」を取り込むまで、06 は着手してはならない。** 理由は §8・§11 を参照。

---

## 1. 概要

記事ビュー（`ArticleView`）に「後で読む」ボタンを足し、押すと記事の URL を **Instapaper**（外部の後で読むサービス）へ送って保存する。Instapaper 呼び出し自体は #05 が用意する instapaper スライスの「add」機能（`add_url`）を再利用する。

加えて、**ローカルにも保存状態を持つ**（別テーブル `read_later_items`）。理由は次の UX 要件のため:

- 同じ記事を 2 回押しても二重登録にならない（**重複追加の冪等化**）。
- 一度保存した記事は再訪時に「保存済み」と表示する（ボタンが状態を持つ）。資格情報が後で消えても、保存済みの記事は「保存済み」のまま読める（§5.4 の順序設計）。
- Instapaper への送信が失敗したとき、失敗を記録して**再試行**できる。

単一ユーザ・家庭内 LAN 前提。Instapaper 資格情報は #05 が DB（`instapaper_credentials`）に保持し、未設定時は `NotEnabled`(503) を返す方針をそのまま使う。

---

## 2. スコープ / 非スコープ

### スコープ（この機能で実装する）

- マイグレーション `0004_read_later.sql`（`read_later_items` テーブル新設）。
- instapaper スライスへの追記:
  - ドメイン: `ReadLaterItem`（FromRow/Serialize）+ 保存状態の定数（`read_later_status`）。
  - リポジトリ: `read_later_items` の upsert / 状態更新 / 取得 / 一覧、および記事 URL/タイトルの参照クエリ。
  - サービス: `save_for_later`（冪等チェック → 記事参照 → 資格情報確認 → Instapaper add → 状態更新）。
  - ハンドラ + ルート: `POST /api/read-later`、`GET /api/read-later`、`GET /api/read-later/{article_id}`。
- フロント: `ArticleView.tsx` に「後で読む」ボタン + 保存状態表示（保存済み / 失敗時の再試行 / 未設定時の誘導）。`lib/api.ts` にメソッド + `ReadLaterItem` 型 + ステータス判定ヘルパ `errorStatus()` を追加。
- テスト: `read_later_status` の単体テスト、`backend/tests/read_later.rs`（マイグレーション/SQL レベルの結合テスト）、`scripts/test/read-later.sh`（HTTP スモーク）。

### 非スコープ（この機能では作らない）

- Instapaper 資格情報の保存 UI / `instapaper_credentials` テーブル / マイグレーション 0003 / `instapaper status` API / Instapaper への HTTP 呼び出し本体（`add_url`）／資格情報ロード（`load_credentials`）— **すべて #05 の責務**。06 はこれらを**新規に作らない**。05 が公開する関数を呼ぶだけ（§8 に契約を明記）。**05 未完了時に 06 がこれらを肩代わりすることはしない**（理由: §8・§11。`load_credentials` は 0003 のテーブルを前提とし、0003 は 05 が持つため、06 単独実装は非機能になる）。
- 「後で読む」記事の専用一覧ページ／サイドバー項目（一覧 API は用意するが、専用ビューは別機能）。
- 非同期ワーカーによる送信（本設計は**同期送信**。`status` 列は将来の非同期化の余地を残すだけ。§11 参照）。
- 記事本文の Instapaper への全文転送（送るのは URL とタイトルのみ。Instapaper 側が本文を取得する）。
- Instapaper からの「後で読む」解除（unsave）。
- バックエンドのライブラリターゲット化（`src/lib.rs` 追加）。現状クレートはバイナリのみで、結合テストからスライス内部関数を import できない（§9 で扱う）。lib 化は横断的変更のため本スライスでは行わない。

---

## 3. 既存実装の調査と再利用（車輪の再発明をしない）

実ファイルを確認済み。再利用する資産:

| 資産 | 場所（確認済み） | 06 での使い方 |
|------|------|----------------|
| **instapaper スライス（#05）** | `backend/src/features/instapaper/`（05 が新設） | 同一スライスに追記。資格情報ロード・add 呼び出し・`NotEnabled` 方針・`/settings` UI を再利用 |
| **`articles` テーブル** | `migrations/0001_init.sql`（`url TEXT UNIQUE`, `title TEXT NOT NULL`） | Instapaper に送る URL/タイトルを **instapaper スライス内の自前 SQL** で読む（CQRS-lite。`feed_overview` 前例と同じ。articles スライスのコードには依存しない） |
| **`ArticleId` newtype** | `backend/src/features/articles/domain.rs:8`（`#[sqlx(transparent)]`, `Copy`, `Serialize`） | 重複定義せず import して再利用（articles が `feeds::domain::FeedId` を import している前例どおり、newtype のスライス間再利用は許容） |
| **`NotEnabled` パターン** | `articles/service.rs:36`（`AppError::NotEnabled("ANTHROPIC_API_KEY is not set".into())`） | Instapaper 資格情報未設定時に同型のエラーを返す |
| **reqwest 直叩き（trait なし）** | `shared/llm/anthropic.rs`（`AnthropicClient`、非 2xx → `Upstream`） | #05 の `add_url` がこの手法を踏襲する想定。06 は trait/dyn を足さない |
| **`AppError`（不編集）** | `shared/error.rs`（`NotFound`/`Validation`/`NotEnabled`/`Upstream`/`Database`/`Other` の 6 バリアント） | `NotFound`/`NotEnabled`/`Upstream` を使い分け。**新バリアントは足さない** |
| **runtime クエリ + FromRow** | `articles/repository.rs`, `stats/repository.rs`（`query_as::<_, (i64,i64,i64)>` 等） | `sqlx::query`/`query_as::<_, T>` のみ。`query!` マクロ不使用 |
| **`ON DELETE CASCADE`** | `0001_init.sql`（`articles.feed_id` 参照） | `read_later_items.article_id` → `articles(id)` ON DELETE CASCADE |
| **`ArticleView.tsx`** | `frontend/src/routes/ArticleView.tsx`（`Button`・`createResource`・`busy` シグナル・`alert`） | 既存の流儀に合わせてボタンを追加 |
| **`http<T>()`** | `frontend/src/lib/api.ts:27`（`!ok` で `throw new Error(\`${status} ${statusText}: ${body}\`)`、204→undefined） | 既存ヘルパをそのまま使い、新メソッドを追加。**エラーは status コードを含むメッセージ文字列**である点に依存（§6.1） |
| **単体テストの置き場所** | `feeds/domain.rs`（`FeedUrl::parse` の `#[cfg(test)] mod tests`） | 純粋ロジック（状態文字列）をここに置く |
| **「結合テスト」の実体** | `scripts/test/api-stats.sh`（curl で nginx `:8081` を叩く shell） | 06 の HTTP スモークも**同じ curl 方式**を踏襲する（§9.3） |

### 確認した重要な事実（テスト計画に影響）

- **`backend/tests/` ディレクトリは存在しない。** `backend/Cargo.toml` に `[dev-dependencies]` も無い。
- **クレートはバイナリのみ**（`backend/src/main.rs` のみ。`src/lib.rs` は無い）。Rust の結合テスト（`tests/*.rs`）は別クレートとしてコンパイルされ、**lib ターゲットが無いとスライス内部関数（`features::instapaper::repository::*` 等）を import できない**。一方、`tests/*.rs` は通常依存（`sqlx`/`tokio`/`uuid`/`dotenvy` 等、Cargo.toml の `[dependencies]`）と `[dev-dependencies]` を利用できる。よって 06 の結合テストは「クレート内部を import せず、`sqlx::migrate!` + 生 SQL でマイグレーション 0004 の挙動を検証する」方式にする（§9.2）。Rust から `service::save_for_later` を直接叩く E2E は lib 化が要るため非スコープ。
- **stats スライスの「結合テスト」は Rust ではなく curl スクリプト**（`scripts/test/api-stats.sh`、`http://localhost:8081/api/stats` を叩く）。よって「stats に倣って Rust の `backend/tests/` ハーネスを書く」という前例は**存在しない**。本設計はこの誤った前提を採らず、ハーネスを §9.2 で具体的に定義し、HTTP スモークは curl 方式（§9.3）にする。
- 既存マイグレーションは `0001_init.sql` のみ。土台設計のマイグレーション割り当てで **06 は 0004 を予約済み**（0002=folders, 0003=instapaper）。
- 設定は `DATABASE_URL` を読む（`shared/config.rs`）。`TEST_DATABASE_URL` は未定義。テストでは `TEST_DATABASE_URL` があればそれを、無ければ `DATABASE_URL` を使う（§9.2）。

---

## 4. データモデルとマイグレーション

### 4.1 設計判断: articles に列を足さず別テーブル

土台設計どおり **別テーブル `read_later_items`** を採用する。根拠:

- 「後で読む」の関心（送信ステータス・失敗理由・Instapaper 送信時刻）を `articles` に漏らさない（articles は記事の真実だけを持つ）。
- `article_id` を **主キー**にすることで、重複追加が DB レベルで冪等になる（UPSERT で 1 行に収束）。
- articles スライスのマイグレーション/構造を一切触らずに済む（既存スライス不編集の原則）。

### 4.2 マイグレーション `backend/migrations/0004_read_later.sql`（新規・追記のみ）

> 0001 は編集しない。番号 0004 は土台設計で 06 に割り当て済み（0002=folders, 0003=instapaper）。並行開発で番号が衝突したら、マージ時に次の空き整数へリベースし土台設計の表を更新すること。
> **適用順の注意（重要）**: このファイル（0004）は **0003 が先に存在する状態でのみ適用すること**。0003 不在のまま 0004 を適用済みにすると、後から #05 が 0003 を追加した際に「既適用の 0004 より小さいバージョン 0003 が後から現れた」と sqlx が判定し、起動時の `run_migrations` が失敗する（§11 のリスク参照）。05→06 のハード依存はこの順序保証も兼ねる。

```sql
-- Read-later: per-article state of "save to Instapaper".
-- One row per article (PK = article_id) makes duplicate saves idempotent.
-- status lifecycle: 'pending' (about to send) -> 'added' (Instapaper accepted)
--                                            \-> 'failed' (Instapaper rejected; see last_error)
CREATE TABLE IF NOT EXISTS read_later_items (
    article_id          UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    status              TEXT NOT NULL DEFAULT 'pending'
                          CHECK (status IN ('pending', 'added', 'failed')),
    instapaper_added_at TIMESTAMPTZ,         -- set when status becomes 'added'
    last_error          TEXT,                -- set when status becomes 'failed'
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- For listing failed/pending items (small table; index is optional but cheap).
CREATE INDEX IF NOT EXISTS idx_read_later_status ON read_later_items(status);
```

関係: `articles 1 ──0..1 read_later_items`。記事削除で CASCADE 削除。`instapaper_credentials`（#05, 0003）とは直接の FK 関係はない（資格情報は singleton）。

---

## 5. バックエンド設計

instapaper スライスへ**追記**する（05 が `domain.rs`/`repository.rs`/`service.rs`/`handler.rs`/`mod.rs` を新設している前提）。06 は各ファイルに read-later 用の要素を足す。**新スライスは作らない。**

### 5.1 なぜ新スライスにしないか（境界の根拠）

「後で読む」のバックエンドは Instapaper のみであり、read-later → instapaper の越境呼び出しが常時発生する。両者は 1 つのアグリゲート（Instapaper 連携）として強く凝集するため、土台設計の判断どおり instapaper スライス内に同居させる（土台設計で唯一許可された「2 機能 1 スライス」例外、理由付き）。これにより越境呼び出しが生まれない。

### 5.2 domain（`instapaper/domain.rs` に追記）

```rust
// 既存 ArticleId を再利用（重複定義しない）。articles → feeds::FeedId と同じ越境 import 前例。
use crate::features::articles::domain::ArticleId;
use serde::Serialize;

/// read_later_items 1 行をミラーする。status は DB の CHECK 制約で 3 値に限定される。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReadLaterItem {
    pub article_id: ArticleId,
    pub status: String, // "pending" | "added" | "failed"
    pub instapaper_added_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// 保存状態の文字列を 1 箇所に固定し、サービス層の分岐・リポジトリの書き込みで参照する。
/// （TEXT 列なので sqlx::Type の enum マッピングは使わず、定数で扱う。enum 化は §11 の任意改善）
pub mod read_later_status {
    pub const PENDING: &str = "pending";
    pub const ADDED: &str = "added";
    pub const FAILED: &str = "failed";
}
```

> `status` を Rust の enum にして「不正状態をコンパイル時に弾く」のが DDD 的には理想だが、TEXT 列への sqlx マッピングは設定が増える。本設計は **DB の CHECK 制約 + 書き込み定数の一元化**で不正値を防ぎ、`FromRow` は `String` のまま（JSON シリアライズも素直）。`ReadLaterStatus` enum 化（`TryFrom<&str>` + `as_str()`）はスライスの DDD スタイルにより合致する任意の堅牢化として §11 に記載。

### 5.3 repository（`instapaper/repository.rs` に追記）

すべて `&PgPool` を取る自由関数、runtime クエリのみ。

```rust
use sqlx::PgPool;
use super::domain::ReadLaterItem;
use crate::features::articles::domain::ArticleId;
use crate::shared::error::{AppError, AppResult};

/// Instapaper に送る記事参照（URL は必須、title は記事の NOT NULL 列）。
/// articles を instapaper スライス内の自前 SQL で読む（read-only / CQRS-lite）。
pub struct ArticleRef { pub url: String, pub title: String }

pub async fn fetch_article_ref(pool: &PgPool, id: ArticleId) -> AppResult<ArticleRef> {
    sqlx::query_as::<_, (String, String)>("SELECT url, title FROM articles WHERE id = $1")
        .bind(id.0)
        .fetch_optional(pool)
        .await?
        .map(|(url, title)| ArticleRef { url, title })
        .ok_or(AppError::NotFound)
}

/// pending として 1 行を確保（既存行があっても pending に戻し last_error をクリア。PK で冪等）。
pub async fn upsert_pending(pool: &PgPool, id: ArticleId) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO read_later_items (article_id, status, updated_at)
           VALUES ($1, 'pending', now())
           ON CONFLICT (article_id) DO UPDATE
             SET status = 'pending', last_error = NULL, updated_at = now()"#,
    ).bind(id.0).execute(pool).await?;
    Ok(())
}

pub async fn mark_added(pool: &PgPool, id: ArticleId) -> AppResult<ReadLaterItem> {
    sqlx::query_as::<_, ReadLaterItem>(
        r#"UPDATE read_later_items
           SET status = 'added', instapaper_added_at = now(), last_error = NULL, updated_at = now()
           WHERE article_id = $1
           RETURNING *"#,
    ).bind(id.0).fetch_one(pool).await.map_err(Into::into)
}

pub async fn mark_failed(pool: &PgPool, id: ArticleId, err: &str) -> AppResult<ReadLaterItem> {
    sqlx::query_as::<_, ReadLaterItem>(
        r#"UPDATE read_later_items
           SET status = 'failed', last_error = $2, updated_at = now()
           WHERE article_id = $1
           RETURNING *"#,
    ).bind(id.0).bind(err).fetch_one(pool).await.map_err(Into::into)
}

pub async fn get_item(pool: &PgPool, id: ArticleId) -> AppResult<Option<ReadLaterItem>> {
    sqlx::query_as::<_, ReadLaterItem>("SELECT * FROM read_later_items WHERE article_id = $1")
        .bind(id.0).fetch_optional(pool).await.map_err(Into::into)
}

pub async fn list(pool: &PgPool) -> AppResult<Vec<ReadLaterItem>> {
    sqlx::query_as::<_, ReadLaterItem>(
        "SELECT * FROM read_later_items ORDER BY updated_at DESC",
    ).fetch_all(pool).await.map_err(Into::into)
}
```

### 5.4 service（`instapaper/service.rs` に追記）

`&AppState` を取り、repository + #05 の Instapaper 呼び出しをオーケストレーションする。**同期送信**。

**処理順序が重要**: 「既に `added` 済みなら即返す」短絡を、資格情報チェックより**前**に置く。こうすると、後で資格情報が削除・変更されても、過去に保存済みの記事は `503` ではなく**キャッシュ済みの `added` を返して読める**（冪等・閲覧性の維持）。記事参照（`NotFound` ガード）と資格情報チェック（`NotEnabled`）は、新規送信が必要なときだけ実行する。

```rust
use super::domain::{read_later_status, ReadLaterItem};
use super::repository;
use crate::features::articles::domain::ArticleId;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn save_for_later(state: &AppState, id: ArticleId) -> AppResult<ReadLaterItem> {
    // 1) 冪等: 既に added 済みなら、資格情報も記事参照も見ずに即返す。
    //    （資格情報が後で消えても保存済みは読める / Instapaper を再度叩かない）
    if let Some(item) = repository::get_item(&state.db, id).await? {
        if item.status.as_str() == read_later_status::ADDED {
            return Ok(item);
        }
    }

    // 2) 記事の URL/タイトルを取得（無ければ NotFound=404）
    let article = repository::fetch_article_ref(&state.db, id).await?;

    // 3) 資格情報を確認（未設定なら NotEnabled=503）— #05 の関数を再利用
    let creds = load_credentials(&state.db)
        .await?
        .ok_or_else(|| AppError::NotEnabled("Instapaper credentials are not set".into()))?;

    // 4) pending を確保 → Instapaper add → 結果で状態確定
    repository::upsert_pending(&state.db, id).await?;
    match add_url(&state.http, &state.config, &creds, &article.url, Some(&article.title)).await {
        Ok(()) => repository::mark_added(&state.db, id).await,
        Err(e) => {
            // 失敗も DB に残して可視化・再試行可能にしてから 502 を返す
            let _ = repository::mark_failed(&state.db, id, &e.to_string()).await;
            Err(AppError::Upstream(format!("instapaper add failed: {e}")))
        }
    }
}

pub async fn get_read_later(state: &AppState, id: ArticleId) -> AppResult<Option<ReadLaterItem>> {
    repository::get_item(&state.db, id).await
}

pub async fn list_read_later(state: &AppState) -> AppResult<Vec<ReadLaterItem>> {
    repository::list(&state.db).await
}
```

> `load_credentials` / `add_url` は **#05 が instapaper スライス内に提供**する（§8 の契約参照）。同一スライス内なので可視。関数名/シグネチャが 05 の実装と異なる場合は、この `service.rs` 内の呼び出し箇所だけを合わせる（差分は 1 ファイルに閉じる）。`add_url` は reqwest で Instapaper Simple API（HTTP Basic 認証 + form `url`/`title`）を直叩きし、非 2xx を `AppError::Upstream` にする（`anthropic.rs` と同手法、trait なし）。
> 効率の補足: 手順 1（`get_item`）と手順 2（`fetch_article_ref`）は 2 回の往復になる。単一ユーザ・低頻度操作では許容（必要なら 1 クエリに統合可能だが、可読性を優先して分ける）。

### 5.5 handler（`instapaper/handler.rs` に追記）

```rust
use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use super::domain::ReadLaterItem;
use super::service;
use crate::features::articles::domain::ArticleId;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

#[derive(serde::Deserialize)]
pub struct SaveBody { pub article_id: Uuid }

// POST /api/read-later  -> 200 Json<ReadLaterItem>（既存 summarize/translate と同様に更新後を返す）
pub async fn save_for_later(
    State(state): State<AppState>,
    Json(body): Json<SaveBody>,
) -> AppResult<Json<ReadLaterItem>> {
    let item = service::save_for_later(&state, ArticleId(body.article_id)).await?;
    Ok(Json(item))
}

// GET /api/read-later/{article_id} -> 200 Json<ReadLaterItem> | 404
pub async fn get_one(
    State(state): State<AppState>,
    Path(article_id): Path<Uuid>,
) -> AppResult<Json<ReadLaterItem>> {
    service::get_read_later(&state, ArticleId(article_id))
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

// GET /api/read-later -> 200 Json<Vec<ReadLaterItem>>
pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<ReadLaterItem>>> {
    Ok(Json(service::list_read_later(&state).await?))
}
```

### 5.6 routes（`instapaper/mod.rs` の `routes()` に行追加）

05 の credentials/status ルートに加えて:

```rust
use axum::routing::{get, post};
// ...routes() 内...
.route("/api/read-later", post(handler::save_for_later).get(handler::list))
.route("/api/read-later/{article_id}", get(handler::get_one))
```

> **発見性メモ**: instapaper スライスは `/api/instapaper/...`（05）に加えて `/api/read-later`・`/api/read-later/{article_id}`（06）も所有する。プレフィックスが揃っていないが axum 上は問題ない。将来の読者向けに「read-later ルートは instapaper スライス内にある」と覚えておくこと。
> `features/mod.rs` への変更は**不要**（`.merge(instapaper::routes())` は #05 が追加済み。06 はスライス内に閉じる）。

### 5.7 AppError 使い分け（`error.rs` 不編集）

| 状況 | バリアント | HTTP | 補足 |
|------|-----------|------|------|
| 既に `added` 済みの再 POST | （エラーにしない） | 200 | 資格情報・記事参照を見ずに冪等返却（順序: §5.4 手順 1） |
| `article_id` の記事が存在しない | `NotFound` | 404 | |
| Instapaper 資格情報が未設定 | `NotEnabled` | 503 | `load_credentials` が `None` |
| Instapaper add が失敗（403/400/5xx 含む） | `Upstream` | 502 | 行は `status='failed'`, `last_error` 付きで残る |
| GET `/api/read-later/{id}` で行なし | `NotFound` | 404 | フロントは「未保存」に畳む（§6.1） |

---

## 6. フロントエンド設計

### 6.1 `lib/api.ts`（型 + メソッド + ステータス判定ヘルパ）

`http<T>()` は `!res.ok` のとき `throw new Error(\`${res.status} ${res.statusText}: ${body}\`)` する。**構造化された status フィールドは無く、メッセージ文字列の先頭が 3 桁ステータス**になる。よって 503/502/404 を区別するには `e.message` の先頭を見る。これを 1 箇所のヘルパに固める。

```ts
// http<T> は Error(`${status} ${statusText}: ${body}`) を投げる。先頭の status コードを取り出す。
export function errorStatus(e: unknown): number | null {
  const msg = e instanceof Error ? e.message : String(e);
  const m = /^(\d{3})\b/.exec(msg);
  return m ? Number(m[1]) : null;
}

export interface ReadLaterItem {
  article_id: string;
  status: "pending" | "added" | "failed";
  instapaper_added_at: string | null;
  last_error: string | null;
  created_at: string;
  updated_at: string;
}

// api オブジェクトに追加（命名は既存の 動詞+リソース に揃える）
saveForLater: (articleId: string) =>
  http<ReadLaterItem>("/api/read-later", {
    method: "POST",
    body: JSON.stringify({ article_id: articleId }),
  }),

// 404 のみ「未保存」を意味するので null に畳む。それ以外（500/ネットワーク等）は握り潰さず再 throw。
getReadLater: async (articleId: string): Promise<ReadLaterItem | null> => {
  try {
    return await http<ReadLaterItem>(`/api/read-later/${articleId}`);
  } catch (e) {
    if (errorStatus(e) === 404) return null; // 未保存
    throw e;                                  // 一時的な 500 等を「未保存」と誤表示しない
  }
},

listReadLater: () => http<ReadLaterItem[]>("/api/read-later"),
```

> 土台設計（フロント §4.4）は暫定で `saveToInstapaper(url)` を挙げているが、本設計は **`saveForLater(articleId)`** を正とする。理由: サーバが `article_id` から URL を引くため、クライアントの URL を信用せずに済み、`read_later_items` の PK（冪等・状態追跡）と一致する。土台設計のメソッド名はこの設計で置き換える。
> `errorStatus()` は他機能（05 の 503 ハンドリング等）でも使えるよう汎用ヘルパとして export する。

### 6.2 `routes/ArticleView.tsx`（ボタン + 保存状態）

既存の summarize/translate と同じ流儀（`Button`・ローカル `createSignal`・失敗時 `alert`）で実装。

- マウント時に `createResource(() => params.id, (id) => api.getReadLater(id))` で保存状態を取得（`null`=未保存）。`getReadLater` は 404 以外を再 throw するので、一時的なエラーは `resource.error` に出て「未保存」と誤表示されない。
- 既存のボタン行（要約・翻訳の `<div class="flex gap-2">`）に「後で読む」`Button`（`variant="outline"`）を追加。送信中フラグは要約/翻訳と衝突しない独立の `createSignal<boolean>` を使う。
- ボタンの表示状態（`item()?.status` で分岐。`item` は read-later の resource）:
  - **未保存**（`null`）→ 「後で読む」。押下で `api.saveForLater(id)` → 戻り値で resource を `mutate` 更新。
  - **送信中** → 「保存中…」（`disabled`）。
  - **`added`** → 「保存済み ✓」（`disabled`。冪等なので押せなくてよい）。
  - **`failed`** → 「再試行」。押下で再 POST。`last_error` を小さなテキスト（`text-xs text-muted-foreground`）で補足表示。
- エラーハンドリング（catch）— `errorStatus(e)` で分岐:
  - `503`（`NotEnabled`）→ `alert("Instapaper が未設定です。設定画面で資格情報を登録してください。")`。可能なら `/settings`（#05）への導線を出す。
  - `502`（`Upstream`）→ サーバは `status:"failed"` で行を残すが POST 自体は throw する。catch で `api.getReadLater(id)` を再取得して `mutate` し「再試行」状態 + `last_error` を反映、`alert` で失敗を知らせる。
  - それ以外 → `alert(\`保存に失敗しました: ${String(e)}\`)`。
- アイコンを使う場合は土台設計の `lucide-solid`（採否は #07/#04 と合わせて決定）に倣う。未導入なら絵文字/テキストで可（本機能で新規依存は必須化しない）。

> Ark UI 部品は不要（単純なボタン + テキスト状態）。新しい複雑 a11y 部品は使わない。装飾は既存トークン（`text-muted-foreground`, `border-border` 等）のみ。

### 6.3 状態管理

ArticleView 内のローカル状態（`createResource` + `createSignal`）で完結。グローバルストアは不要。記事ごとの保存状態は URL（`params.id`）に紐づくため、ルート遷移で自然にリセットされる。

---

## 7. API 契約

### POST `/api/read-later`
記事を Instapaper に保存し、ローカル状態を返す。冪等。

Request:
```json
{ "article_id": "0f8b2c4e-1111-2222-3333-444455556666" }
```
Response `200 OK`:
```json
{
  "article_id": "0f8b2c4e-1111-2222-3333-444455556666",
  "status": "added",
  "instapaper_added_at": "2026-06-26T12:34:56Z",
  "last_error": null,
  "created_at": "2026-06-26T12:34:56Z",
  "updated_at": "2026-06-26T12:34:56Z"
}
```
エラー（本文は `shared/error.rs` の `{"error": <message>}` 形式・文言は verbatim）:
- `404` 記事が存在しない: `{"error":"resource not found"}`。
- `503` Instapaper 資格情報未設定: `{"error":"feature not yet enabled: Instapaper credentials are not set"}`。
- `502` Instapaper add 失敗: `{"error":"upstream request failed: instapaper add failed: …"}`。このとき行は `status:"failed"`, `last_error` 付きで残る。
- 既に `added` 済みの再 POST → `200`（Instapaper を再度叩かず既存行を返す。資格情報未設定でも 200）。

### GET `/api/read-later/{article_id}`
単一記事の保存状態。Response `200` は POST と同じ JSON。未保存なら `404`（フロントは null に畳む）。

### GET `/api/read-later`
保存状態の一覧（`updated_at` 降順）。Response `200`:
```json
[
  {
    "article_id": "…",
    "status": "failed",
    "instapaper_added_at": null,
    "last_error": "instapaper add failed: 403 …",
    "created_at": "…",
    "updated_at": "…"
  }
]
```

---

## 8. 依存関係

### 依存する（先行が必須・ブロッカー）

**#05 instapaper-integration（ハード依存）。** 06 は 05 がマージ済みであることを前提に着手する。具体的には 05 が次を提供していること:

1. マイグレーション **0003（`instapaper_credentials`）** が `backend/migrations/` に存在する（06 の `service::save_for_later` の `load_credentials` と、結合テスト「資格情報未設定 → NotEnabled」が依存）。
2. instapaper スライス内に次の関数（同一スライスなので可視）:
   ```rust
   // instapaper スライス内に存在することを期待する契約:
   pub struct InstapaperCredentials { pub username: String, pub password: String /* + updated_at 等 */ }
   pub async fn load_credentials(db: &PgPool) -> AppResult<Option<InstapaperCredentials>>;
   // Instapaper Simple API /api/add を reqwest で叩く。非 2xx は AppError::Upstream。
   pub async fn add_url(
       http: &reqwest::Client,
       config: &AppConfig,          // base_url をここから取る想定（テスト容易性。§11）
       creds: &InstapaperCredentials,
       url: &str,
       title: Option<&str>,
   ) -> AppResult<()>;
   ```
   関数名/シグネチャが 05 の実装と異なる場合は、06 の `service.rs` 内の呼び出し箇所のみを合わせる（差分は 1 ファイルに閉じる）。

**05 未完了時に 06 を単独実装してはならない。** `load_credentials`/`add_url`/0003 を 06 が肩代わりすると、(a) 非スコープで 05 に割り当てた責務を二重実装し、(b) 0003 不在のまま 0004 を適用すると後で 05 の 0003 を入れた時にマイグレーションが順序エラーで失敗する（§11）。必ず 05 → 06 の順で進める。

### ブロックする（この機能が前提になる）

- 「後で読む」一覧ビュー（将来機能）。本機能の `GET /api/read-later` を使う。

### 連動（疎結合・直接依存ではない）

- #10 two-pane-layout（`ArticleView` の置き場所）。レイアウトが変わってもボタンの実装は不変。

---

## 9. テスト計画（TDD: Red → 理解 → Green）

テストは 3 層。**(A) 純粋ロジックの単体テスト**、**(B) マイグレーション/SQL レベルの結合テスト（実 DB、クレート内部は import しない）**、**(C) HTTP スモーク（curl、既存 `api-stats.sh` 前例）**。

> 補足: 現状クレートはバイナリのみ（`src/lib.rs` 無し）なので、Rust の結合テストから `service::save_for_later` 等のスライス内部を直接呼ぶことはできない。サービスのオーケストレーション（順序・`NotFound`/`NotEnabled` マッピング）は (C) の HTTP スモーク + §9.4 手動 E2E で担保する。lib 化して Rust から直接叩く E2E は非スコープ（§2）。

### 9.1 単体テスト（`instapaper/domain.rs` の `#[cfg(test)] mod tests`）

`feeds/domain.rs` の `#[cfg(test)]` 前例に倣う。純粋ロジックのみ・DB 不要:

- `status_constants_match_db_check`: `read_later_status::{PENDING,ADDED,FAILED}` が `"pending"/"added"/"failed"` であること（マイグレーション 0004 の `CHECK (status IN (...))` と文字列を一致させる回帰防止）。
- （`ReadLaterStatus` enum を §11 の任意改善として導入する場合のみ）`TryFrom<&str>` / `as_str()` の round-trip と未知値が `Err` になること。

### 9.2 結合テスト（`backend/tests/read_later.rs`・実 DB・クレート内部を import しない）

`backend/tests/` は未存在のため**このファイルが最初に作る**。クレート（lib）を import せず、**通常依存**の `sqlx`/`tokio`/`uuid`/`dotenvy`（すべて `[dependencies]` 済み・test ターゲットから利用可）だけを使う。ハーネスは:

1. `let _ = dotenvy::dotenv();`（`just test` でも素の `cargo test` でも `.env` を読む）。
2. DB URL を `std::env::var("TEST_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL"))` で取得。どちらも無ければ `panic!`（DB 必須を明示）。
3. `sqlx::postgres::PgPoolOptions::new().connect(&url).await` で `PgPool` を作る。
4. `sqlx::migrate!("./migrations").run(&pool).await` で全マイグレーション適用（`db.rs::run_migrations` と同じ。パスは test クレートの `CARGO_MANIFEST_DIR=backend/` 基準で解決）。**0003 を含む全 0001〜0004 が `migrations/` に揃っている前提**（= 05 が先行している）。
5. 各テストは**トランザクションを開始してロールバック**で隔離する（`let mut tx = pool.begin().await?; … // tx は drop で rollback`）。共有 dev DB を汚さない。フィード/記事は乱数 UUID + 一意 URL で INSERT する。

> 依存追加: `[dev-dependencies]` は**不要**（`sqlx`/`tokio`/`uuid`/`dotenvy` は通常依存で test ターゲットから使える。`#[tokio::test]` は tokio "full" の `macros`+`rt` で動作、`sqlx::migrate!` は "migrate" feature で利用可、いずれも Cargo.toml で有効）。`wiremock` は §11 の「ハッピーパスをモックする任意 E2E」を書く場合にのみ `[dev-dependencies]` へ追加する。
> 実行: `just test`（= `cd backend && cargo test`）。**DB が起動している必要がある**（`just dev-db`）。

Red を先に書く検証項目（生 SQL で 0004 の挙動を確認 — リポジトリ関数が依存する SQL と同一の振る舞い）:

1. `check_rejects_bad_status`: `INSERT INTO read_later_items(article_id, status) VALUES ($1,'bogus')` が **失敗**する（CHECK 制約）。
2. `upsert_is_idempotent_on_pk`: §5.3 の `upsert_pending` 相当の `INSERT … ON CONFLICT (article_id) DO UPDATE …` を同一 `article_id` で 2 回 → 行数 **1**（PK 冪等）。
3. `repend_clears_last_error`: 一旦 `UPDATE … SET status='added'`（mark_added 相当）した行に再度 upsert_pending 相当を流すと、`status='pending'` かつ `last_error IS NULL` に戻る（再試行シナリオ）。
4. `mark_failed_sets_error`: `UPDATE … SET status='failed', last_error=$2`（mark_failed 相当）で `status='failed'` かつ `last_error` がセットされる。
5. `cascade_delete`: 記事を `DELETE FROM articles WHERE id=$1` すると対応する `read_later_items` 行も消える（FK CASCADE）。
6. `defaults_are_applied`: 列省略 INSERT で `status='pending'`, `created_at`/`updated_at` が非 NULL（DEFAULT）。

> これらは 0004（新規成果物）の正しさを実 DB で担保する。クレート内部関数を呼ばないため lib 化不要。

### 9.3 HTTP スモーク（`scripts/test/read-later.sh`・curl・既存 `api-stats.sh` 前例）

`scripts/test/api-stats.sh` と同じ書式で、稼働中スタック（nginx `:8081`）を curl で叩く。サービスのオーケストレーション（404/503 の経路）を full path で確認する:

- `POST /api/read-later` にランダム UUID（存在しない記事）→ **404** を期待。
- 資格情報未設定の状態で実在記事を `POST` → **503** を期待（`NotEnabled`）。
- （資格情報設定済み + 実在記事 id があれば）`POST` → 200 `added`、`GET /api/read-later/{id}` → 200、`GET` に未保存 id → 404。これらは実 Instapaper 送信を伴うため、スクリプト内で「引数に記事 id を渡したときのみ実行」する任意チェックとして書く。

### 9.4 フロント（手動 + 型）

- `tsc` 型チェック（`just lint` の `pnpm typecheck`）で `ReadLaterItem`・`errorStatus`・新メソッドの型が通ること。
- 手動 E2E: 未設定 → 保存（503 誘導）→ 資格情報登録 → 保存（保存済み表示）→ リロードで保存済み維持 →（資格情報を消して）保存済み記事を開いても「保存済み」のまま表示される（§5.4 順序）→（資格情報を不正にして別記事を）保存で失敗 → 再試行、の一連を確認。

すべてのテストは書いたら実行し、`just lint`（clippy `-D warnings` + `tsc`）を通す。

---

## 10. 実装手順（順序付きチェックリスト）

1. **前提確認（ブロッカー）**: #05 instapaper スライスがマージ済みか確認する。
   - `backend/src/features/instapaper/` が存在し、`features/mod.rs` に `.merge(instapaper::routes())` がある。
   - `backend/migrations/0003_instapaper.sql`（`instapaper_credentials`）が存在する。
   - `load_credentials` / `add_url` が instapaper スライス内に存在する（§8 の契約）。
   - **どれか欠けていれば 06 は着手しない**。05 と調整して 05 を先に完了させる（06 が肩代わりしない）。
2. `backend/migrations/0004_read_later.sql` を §4.2 の SQL で新規作成（既存ファイルは触らない）。`just migrate` で適用（**0003 適用後に**）。
3. **Red**: `instapaper/domain.rs` に §5.2 を追記し `#[cfg(test)]` の単体テスト（§9.1）を書く → 失敗を確認。
4. `instapaper/repository.rs` に §5.3 の関数群を追記。
5. **Red**: `backend/tests/read_later.rs` を新設し §9.2 のハーネス + テスト 1〜6 を書く → 失敗を確認（DB は `just dev-db` で起動）。
6. `instapaper/service.rs` に §5.4 を追記（**順序: 冪等短絡 → 記事参照 → 資格情報 → 送信**。`load_credentials`/`add_url` を呼ぶ）。
7. `instapaper/handler.rs` に §5.5 を追記。`instapaper/mod.rs` の `routes()` に §5.6 の 2 ルートを追加。
8. **Green**: `just test`（単体 + 結合）が通るまで実装を詰める。`cargo fmt` + `just lint`（clippy `-D warnings`）。
9. `scripts/test/read-later.sh` を §9.3 で新設（`api-stats.sh` をひな型に）。稼働スタックに対し 404/503 を確認。
10. `frontend/src/lib/api.ts` に §6.1 の型 + `errorStatus` + 3 メソッドを追加。
11. `frontend/src/routes/ArticleView.tsx` に §6.2 のボタン + 保存状態（`createResource` 初期取得、`status` で表示分岐、503/502/その他のエラー処理）を実装。
12. フロント `tsc` 型チェック（`just lint`）。
13. 手動 E2E（§9.4）。Instapaper 実送信は資格情報を `/settings`（#05）で登録してから確認。
14. （任意・推奨）05 と協調して Instapaper base URL を `AppConfig` 化し、`wiremock`（`[dev-dependencies]` 追加）でハッピーパスをモックするテストを追加。※ Rust から `service` を直接叩くには lib 化が要るため、当面は HTTP スモーク + 手動で代替する。

---

## 11. リスク・未決事項・代替案

- **マイグレーション適用順（05→06 の厳守）**: 0004 を 0003 不在の DB に適用済みにすると、後から 05 が 0003 を追加した際に sqlx が「既適用の 0004 より小さい新規バージョン 0003」を検出し、起動時 `run_migrations` が失敗する。**緩和**: 05 をハード依存にし、`migrations/` に 0003 が揃ってから 0004 を適用する（§8・§10 手順 1〜2）。並行開発で番号が衝突したら、マージ時に空き整数へリベースし土台設計の表を更新。
- **Instapaper Simple Developer API の正確な仕様は実装時に `instapaper.com/api` で要確認**（「この通り動く」と断定しない）。想定: `POST https://www.instapaper.com/api/add` に HTTP Basic 認証（username:password）+ form `url`（必須）/`title`/`selection`。成功 201、認証失敗 403、不正 400、障害 1500/5xx。非 2xx は `AppError::Upstream`。実体は #05 の `add_url` が担うが、06 の失敗 UX（`failed` + 再試行）が依存するため挙動を確認する。
- **base URL のハードコード問題（テスト容易性）**: `anthropic.rs` は URL を const にしている。同様に Instapaper URL を const にすると結合テストでハッピーパスをモックできない。**推奨**: #05 が `AppConfig` に `instapaper_base_url`（既定 `https://www.instapaper.com`）を持たせ、`add_url(http, config, …)` がそこから読む。これは 05 側の小変更だが 06 のテスト品質に効く。実現しない場合、06 のハッピーパスはモック不能なので §9.2（SQL レベル）+ §9.3（404/503 経路）+ 手動でカバーする。
- **Rust からサービスを直接テストできない（lib 不在）**: 現状クレートはバイナリのみ。`service::save_for_later` の順序・エラーマッピングを Rust の結合テストで直接検証するには `src/lib.rs` を足してモジュールを公開する横断的変更が要る。本スライスでは行わず、(C) HTTP スモーク + 手動 E2E で代替。将来 lib 化されたら §9.2 のハーネスから `service`/`repository` を直接叩くテストへ昇格できる。
- **同期送信 vs 非同期**: 本設計は POST 内で Instapaper を同期呼び出しする（summarize/translate と同じ流儀でシンプル）。`status='pending'` 列は、(a) 呼び出し中にプロセスが落ちた中間状態の記録、(b) 将来 `apalis` 等で非同期リトライ化する余地、のために残す。単一ユーザ・低頻度操作では同期で十分。レイテンシ/タイムアウトが問題化したら非同期ワーカーへ昇格（別機能）。
- **資格情報の取り扱い**: 06 はパスワードを参照するが、レスポンス（`/api/read-later`）には一切含めない。平文保存・GET で password を返さない方針は #05 の責務。06 は password をログにも出さない（`last_error` に Instapaper のレスポンス本文を入れる際、認証情報が混入しないか確認）。
- **`status` を String にした点**: 不正状態の排除を DB の CHECK 制約 + 書き込み定数に委ねている。より型で縛りたい場合は `ReadLaterStatus` enum を導入し、`FromRow` で `String` を受けて `TryFrom<&str>` で変換、`Serialize` を `as_str()` で実装する（任意の堅牢化。スライスの newtype/DDD スタイルにより合致するが、sqlx の enum 直マッピングは TEXT 列だと設定が増えるため既定は String）。
- **未保存判定のための per-article GET**: `GET /api/read-later/{article_id}`（404=未保存）を用意したが、記事一覧で各記事の保存状態を一括表示したくなったら N+1 になる。その場合は記事一覧 API に状態を JOIN するのではなく（articles スライス不編集）、フロントが `GET /api/read-later`（一覧）を 1 回引いて `article_id` で突合する設計に寄せる。
- **メソッド名の不整合**: 土台設計（フロント）の暫定 `saveToInstapaper(url)` を本設計は `saveForLater(articleId)` に置き換える（§6.1 の理由）。他機能ドキュメントと齟齬が出たら、本設計の `article_id` ベースに統一する。
