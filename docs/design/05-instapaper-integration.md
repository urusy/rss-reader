# 05 Instapaper 連携

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッションの実装者。本書だけで着手できるよう、再利用資産・SQL・関数シグネチャ・ルート文字列まで具体化する。
> **重要な但し書き**: Instapaper Simple Developer API の正確な仕様（エンドポイント URL・パラメータ名・ステータスコード）は **実装時に [instapaper.com/api](https://www.instapaper.com/api) で必ず確認すること**。本書のリクエスト/レスポンス記述は「想定」であり断定ではない。`classify_add_status` / `classify_auth_status`（§5.1）の境界値はそこで確定する。

---

## 1. 概要

購読中の記事を **Instapaper**（後で読むサービス）へ送れるようにする。MVP は **Instapaper Simple Developer API** を使い、HTTP Basic 認証で記事 URL を送るだけのシンプルな連携。ユーザーは家庭内 LAN の設定画面（`/settings`）で Instapaper の資格情報（メールアドレス + パスワード）を一度登録し、以降はワンクリックで記事を Instapaper に保存できる。これにより「あとでちゃんと読む記事」を本リーダー外の常用ツールへ逃がせる。

本機能はバックエンドに新スライス `instapaper` を1枚追加し、(a) 資格情報の保存・状態取得・削除、(b) 記事を Instapaper へ送る `POST /api/read-later` を担う。LLM 連携（`shared/llm`）と同じく **公式 Rust SDK は無いので reqwest で直接呼ぶ**。資格情報が未登録なら `AppError::NotEnabled`（503）を返す「任意機能」パターンに従う（要約/翻訳が `ANTHROPIC_API_KEY` 未設定時に取る挙動と同型）。

**05 と 06（後で読む）の責務分担（重要・レビュー反映点）**: 05 は「Instapaper 接続の確立・検証」と「記事 URL を Instapaper に転送する経路（`POST /api/read-later {article_id}`）」までを所有する。**保存状態の永続トラッキング（`read_later_items` テーブル / 冪等化・リトライ・失敗 UX）と一覧（`GET /api/read-later`）、`ArticleView` の保存ボタン UI は機能 06 が同じ `instapaper` スライスに追記する**（後述 §8）。06 は 05 が公開する **同一の `POST /api/read-later {article_id}` 契約を一切変えずに**、内部サービス関数 `add_to_read_later` に永続化の副作用を足すだけで成立する。すなわち「06 は単なる UI 追加」という誤った主張は採らず、「06 は status 行の追記と一覧 + UI 導線を **同一スライス内に追記**する」と明記する。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）
- マイグレーション（番号は **マージ時の最小空き整数**。§4 参照。プランニング上の枠は `0003` だが、05 を単独先行リリースする場合は `0002` を取る）: 資格情報 singleton テーブル `instapaper_credentials`。
- 新スライス `backend/src/features/instapaper/`（`domain` / `repository` / `service` / `handler` / `mod`）。
- 資格情報の保存 `PUT /api/instapaper/credentials`。保存前に Instapaper の認証エンドポイントで検証してから永続化（即時フィードバック。§11 に検証スキップ代替案）。
- 資格情報の状態取得 `GET /api/instapaper/status` → `{ configured: bool }`（**パスワード・ユーザー名は絶対に返さない**）。
- 資格情報の削除 `DELETE /api/instapaper/credentials`。
- 記事を Instapaper に送る `POST /api/read-later { article_id }`。URL はサーバ側で `articles` から引く（クライアントに生 URL を持たせない）。資格情報未設定なら `NotEnabled`、記事不在なら `NotFound`。**05 ではこのエンドポイントは「転送のみ」**（status 行は書かない。永続化は 06）。
- フロント `/settings` ルート（`routes/Settings.tsx`）に Instapaper 資格情報フォーム + 接続状態表示。
- `lib/api.ts` に型 `InstapaperStatus` と **4 メソッド**（`getInstapaperStatus` / `saveInstapaperCredentials` / `deleteInstapaperCredentials` / `saveToReadLater`）。`saveToReadLater(articleId)` は 05 が所有する `POST /api/read-later` の呼び口。**UI 導線（`ArticleView` の保存ボタン）は 06 が付ける。**
- `components/ui/input.tsx`（自前 Tailwind。バリアント無しのため **cva は使わない**。未存在のため本機能で新設。パスワード欄は `type="password"`）。
- バックエンドのリポジトリ往復（upsert/get/delete）の自動テスト（`#[cfg(test)] mod tests` を `repository.rs` 内に置き、実 DB を `DATABASE_URL` で叩く `#[ignore]` テスト。§9.2）。

### 非スコープ（本機能では実装しない）
- 保存状態の永続トラッキング（`read_later_items` テーブル / `read_later` マイグレーション / `pending|added|failed` ステータス / 冪等化・リトライ UX）と一覧 `GET /api/read-later` → **機能 06**。06 は本スライスに**追記**する（同一スライス共同所有。foundation §2.1）。
- `ArticleView` の「後で読む」保存ボタン本体 → **機能 06**。05 は `saveToReadLater()` メソッドのみ提供し、配線は委ねる。
- パスワードの暗号化保存（MVP は平文列。理由は §4 / §11）。
- Instapaper Full API（OAuth / xAuth、ブックマーク一覧取得・ハイライト等）。MVP は Simple API のみ。
- 複数アカウント。単一ユーザー前提なので資格情報は1行（singleton）。
- テーマ切替（機能 04）。`/settings` を共有するだけで干渉しない（§6.3）。

---

## 3. 既存実装の調査と再利用

実ファイルを確認済み。以下を **再利用し、車輪の再発明をしない**。

| 再利用資産 | 実体（確認済みファイル） | 本機能での使い方 |
|---|---|---|
| reqwest 直叩きパターン | `backend/src/shared/llm/anthropic.rs`（`self.http.post(URL).header(...).send().await.map_err(\|e\| AppError::Upstream(e.to_string()))`、`status().is_success()` 判定、失敗時 `resp.text()` を本文に含める） | Instapaper への HTTP 呼び出しを同型で書く。**trait は足さない**（2つ目の実装予定が無い境界） |
| `AppState { db, config, http }` | `backend/src/shared/state.rs`（`#[derive(Clone)]`、`http: reqwest::Client`） | `state.http`（共有 reqwest クライアント、UA/30s timeout 設定済み = `main.rs`）をそのまま使う。新規 Client を作らない |
| `AppError` 6 バリアント | `backend/src/shared/error.rs`（`NotFound`/404, `Validation(String)`/400, `NotEnabled(String)`/503, `Upstream(String)`/502, `Database(#[from] sqlx::Error)`/500, `Other(#[from] anyhow::Error)`/500。`IntoResponse` で `Json({"error": <Display>})`） | 新バリアントを足さず既存で表現（§5.7）。**`error.rs` は編集しない** |
| 任意機能 = `NotEnabled` パターン | `articles/service.rs::llm_client()` が `anthropic_api_key` 無し時に `NotEnabled("ANTHROPIC_API_KEY is not set")` | 資格情報が DB に無い時に同型で `NotEnabled("Instapaper credentials are not set")` |
| 値オブジェクト `parse() -> Result<_, String>` | `feeds/domain.rs::FeedUrl::parse`（`http://`/`https://` 検査 + `trim`、`#[cfg(test)] mod tests` 付き） | 送信 URL 検証用 `SaveUrl::parse` を同型でスライス内に新設。`Err(String)` は `map_err(AppError::Validation)` |
| 主キー newtype + 値オブジェクト | `feeds/domain.rs::FeedId`、`articles/domain.rs::ArticleId`（`#[derive(... sqlx::Type)] #[sqlx(transparent)]`） | 同型で必要に応じて使う（本機能の article 参照は **素の `Uuid`** を bind するだけで足りるため newtype 依存は持たない。§5.2） |
| スライス構成 + `routes()` | `articles/mod.rs`・`feeds/`・`stats/`（`domain/repository/service/handler/mod`、`fn routes() -> Router<AppState>`、`.route("/path", get(...).post(...))`） | 同じ5ファイル構成で `instapaper` を作る |
| `features/mod.rs` の合成 | `pub mod ...;` + `.merge(...::routes())`（既存4スライスを `router()` で merge） | `pub mod instapaper;` と `.merge(instapaper::routes())` を1行ずつ追加。既存スライスは触らない |
| sqlx ランタイムクエリ + upsert | `stats/repository.rs`（`query_as::<_, T>(SQL).fetch_one`）、`articles/repository.rs`（`fetch_optional().ok_or(AppError::NotFound)`、`INSERT ... ON CONFLICT (url) DO UPDATE` の upsert、`UPDATE` の `rows_affected()` チェック） | 資格情報取得は `fetch_optional`、保存は `ON CONFLICT (id) DO UPDATE`、記事 URL 取得は `fetch_optional` → `NotFound` |
| クロステーブル read を自スライス内 SQL で完結 | `articles/repository.rs` が `feeds::domain::FeedId` を import（**クロススライス domain 参照は既存の前例**）/ foundation の `feed_overview` は feeds+articles を JOIN 読み | `instapaper` から `articles` を **読み取り専用 SQL** で引く（`SELECT url, title FROM articles WHERE id=$1`）。書き込み所有は移さない（§5.2 で正当化） |
| フロント API クライアント | `frontend/src/lib/api.ts`（`http<T>()` は 204→`undefined` 畳み込み済み、`api` オブジェクトに `動詞+リソース` 命名でメソッド集約） | 既存 `http<T>()` をそのまま使い4メソッド追加 |
| 自前 UI 部品 | `frontend/src/components/ui/button.tsx`（`cva`+`cn(@/lib/utils)`+`splitProps`）、`card.tsx`、`dialog.tsx` | `input.tsx` を同型（ただしバリアント無しなので cva 抜きの `cn`+`splitProps`）で新設。`card.tsx`/`button.tsx` を `Settings.tsx` で使う |
| 自動マイグレーション実行 | `main.rs` が起動時に `db::run_migrations(&pool)` → `sqlx::migrate!("./migrations").run(pool)` | マイグレーションファイルを置くだけで適用。コード側の追加配線は不要。**ただし番号順序に注意（§4 / 下記注）** |
| HTTP スモークテストの慣習 | `scripts/test/api-stats.sh`（稼働中スタック nginx `:8081` に curl、HTTP コードと JSON キーを assert） | `scripts/test/api-instapaper.sh` を同型で新設（§9.3） |

> **reqwest の機能フラグ（確認済み・リスクではない）**: `backend/Cargo.toml` は `reqwest = { version="0.12", default-features=false, features=["json","rustls-tls","gzip","brotli"] }`。`.basic_auth()` と `.form()` は **pin されている reqwest 0.12.x で feature gate されていない**（`serde_urlencoded` は hard dependency で `form` という feature は 0.12.x に存在しない）。よって **Cargo.toml への依存追加は不要**。`chrono`/`uuid`/`serde`/`sqlx`/`tokio`/`axum` も既存依存で足りる。
>
> **マイグレーション自動実行と順序の注意（§4 で詳述）**: `run_migrations` は `set_ignore_missing` を呼んでいないため、**先に高い番号を適用した永続 DB にあとから低い番号を足すと起動時マイグレーションがエラー**になる。番号はマージ時に最小空き整数を取る（§4）。

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方（ブロッキング指摘の解消）

土台設計は「0002=folders / 0003=instapaper / 0004=read_later」を **プランニング上の予約**として割り当てている。しかしこれは**機能番号であって適用順序ではない**。`main.rs` の `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を設定していないため、sqlx migrator は **適用済み最大バージョンより小さい未適用マイグレーションを後から発見すると `VersionMissing`（out-of-order）でエラーになり、起動が壊れる**（家庭内サーバの永続 DB で実害）。

**ルール（必ず守る）**:
- マイグレーションファイル名は **マージ時点の最小空き整数**を取る。`ls backend/migrations/` で現状最大番号 +1 を採番する。
- **05 を 02（folders）/06（read_later）より先にマージ・リリースするなら、本マイグレーションは `0002_instapaper.sql` になる**（現状の最新は `0001_init.sql` のみ）。あとから folders は `0003_*`、read_later は `0004_*` と、その時点の空き整数へ繰り上げる。土台設計の番号表は「先着順に最小空き整数へリベースし、表を更新する」運用で読む。
- 既に高い番号（例 `0003`）を永続 DB に適用済みの状態で、より小さい番号を新規追加してはならない。複数機能を並行開発する場合は **マージ順に連番**を割り当てる。
- 既存 `0001_init.sql` は**編集しない**（追記のみ）。

本書では以下、ファイルを **`000N_instapaper.sql`**（`N` = マージ時の最小空き整数。単独先行なら `0002`）と表記する。

### 4.2 スキーマ

新規ファイル **`backend/migrations/000N_instapaper.sql`**:

```sql
-- Instapaper credentials. Single-user app => singleton row pinned to id = 1.
-- Stored reversibly (plaintext for MVP) because the Simple Developer API needs
-- the cleartext password to perform HTTP Basic auth on every request.
-- GET /status never returns the password; encryption-at-rest is a future step (see §11).
CREATE TABLE IF NOT EXISTS instapaper_credentials (
    id         INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    username   TEXT NOT NULL,
    password   TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

設計判断:
- **env ではなく DB に置く理由**: ユーザーが実行時に UI から設定・変更する値だから（要約機能の `ANTHROPIC_API_KEY` は env だが、あちらはオペレーター設定。こちらはエンドユーザー設定）。
- **singleton（`id INTEGER PK DEFAULT 1 CHECK(id = 1)`）**: 単一ユーザーなので資格情報は常に1行。`ON CONFLICT (id) DO UPDATE` で「無ければ挿入、有れば更新」を1クエリで表現でき、行数管理が不要。
- **`updated_at` 列はテーブルに残すが、アプリ層（`StoredCredentials` / `GET /status`）では読まない**。`status` は `configured` のみ返す方針のため、`SELECT` でも取得しない（§5.1 でレビュー指摘の dead field を解消）。将来「最終更新日時」を status に出すなら、ここを読むだけで拡張できる。
- **平文列の理由と制約**: Simple Developer API の Basic 認証は毎回平文パスワードを送る必要があり、一方向ハッシュでは復元できないため **可逆保存が必須**。MVP は家庭内 LAN・単一ユーザー前提で平文列とする。漏洩面を最小化するため `GET /status` は `configured` のみ返す。将来は env 由来の鍵で対称暗号化して保存する拡張余地を残す（§11）。

他テーブル（`feeds`/`articles`）への列追加は無い。記事ごとの保存状態トラッキングは機能 06 の `read_later` マイグレーションが担うため、本機能では `articles` に列を足さない（read-later の関心を articles に漏らさない）。

---

## 5. バックエンド設計

新スライス **`backend/src/features/instapaper/`**。5ファイル構成。

### 5.1 `domain.rs`（値オブジェクト + 純粋ロジック）

```rust
use serde::Serialize;

/// 検証済みの資格情報入力。空文字は構築時に弾く（不正状態を表現不能にする）。
/// password を持つので Serialize は付けない（クライアントに漏らさない）。
#[derive(Debug, Clone)]
pub struct InstapaperCredentials {
    username: String,
    password: String,
}

impl InstapaperCredentials {
    pub fn parse(username: impl Into<String>, password: impl Into<String>) -> Result<Self, String> {
        let username = username.into().trim().to_string();
        let password = password.into(); // パスワードは前後空白も有意なので trim しない
        if username.is_empty() {
            return Err("username must not be empty".into());
        }
        if password.is_empty() {
            return Err("password must not be empty".into());
        }
        Ok(Self { username, password })
    }
    pub fn username(&self) -> &str { &self.username }
    pub fn password(&self) -> &str { &self.password }
}

/// DB から読んだ生の資格情報（add 時の Basic 認証に使う）。
/// Serialize は付けない（password 漏洩防止）。updated_at は読まないので持たない。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredCredentials {
    pub username: String,
    pub password: String,
}

/// GET /status が返す安全な射影。configured のみ公開。
#[derive(Debug, Clone, Serialize)]
pub struct InstapaperStatus {
    pub configured: bool,
}

/// Instapaper へ送る URL の値オブジェクト。FeedUrl と同じスキーム検査だが、
/// スライス越境結合を避けるため instapaper スライス内に閉じる。
/// （add 時は articles から引いた URL を防御的に通す。§5.3）
#[derive(Debug, Clone)]
pub struct SaveUrl(String);

impl SaveUrl {
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

/// Instapaper /api/add のステータスコード分類（純粋関数 = 単体テスト対象）。
#[derive(Debug, PartialEq, Eq)]
pub enum AddOutcome {
    Saved,      // 200/201
    BadRequest, // 400（URL 不正など、クライアント修正可能）
    Failed,     // 403/5xx/その他（資格情報不正・障害）
}

pub fn classify_add_status(code: u16) -> AddOutcome {
    match code {
        200 | 201 => AddOutcome::Saved,
        400 => AddOutcome::BadRequest,
        _ => AddOutcome::Failed,
    }
}

/// Instapaper /api/authenticate のステータスコード分類（純粋関数 = 単体テスト対象）。
#[derive(Debug, PartialEq, Eq)]
pub enum AuthOutcome {
    Valid,   // 200
    Invalid, // 403（資格情報が誤り → フォームにエラー表示したい）
    Failed,  // その他（障害）
}

pub fn classify_auth_status(code: u16) -> AuthOutcome {
    match code {
        200 => AuthOutcome::Valid,
        403 => AuthOutcome::Invalid,
        _ => AuthOutcome::Failed,
    }
}
```

> ステータスコード分類を純粋関数に切り出すのは、外部 API を叩かずに TDD で Red→Green を回すため（MEMORY の「書いたら必ず実行」「バグ修正もテスト先行」方針）。実際のコードがどの値を返すかは instapaper.com/api で要確認（§11）。

### 5.2 `repository.rs`（`&PgPool` を取る free async fn、ランタイムクエリのみ）

```rust
use sqlx::PgPool;
use uuid::Uuid;

use super::domain::StoredCredentials;
use crate::shared::error::AppResult;

/// 記事 URL/タイトル取得用の読み取り射影。
/// 本スライス内に閉じた read-only projection（articles の書き込み所有は移さない）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArticleRef {
    pub url: String,
    pub title: String,
}

pub async fn get_credentials(pool: &PgPool) -> AppResult<Option<StoredCredentials>> {
    let row = sqlx::query_as::<_, StoredCredentials>(
        "SELECT username, password FROM instapaper_credentials WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn upsert_credentials(pool: &PgPool, username: &str, password: &str) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO instapaper_credentials (id, username, password, updated_at)
           VALUES (1, $1, $2, now())
           ON CONFLICT (id) DO UPDATE
             SET username = EXCLUDED.username,
                 password = EXCLUDED.password,
                 updated_at = now()"#,
    )
    .bind(username)
    .bind(password)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_credentials(pool: &PgPool) -> AppResult<()> {
    sqlx::query("DELETE FROM instapaper_credentials WHERE id = 1")
        .execute(pool)
        .await?;
    Ok(())
}

/// article_id から URL/タイトルを引く（読み取り専用）。素の Uuid を bind するので
/// articles スライスの domain 型には依存しない（結合面を最小化）。
pub async fn get_article_ref(pool: &PgPool, article_id: Uuid) -> AppResult<Option<ArticleRef>> {
    let row = sqlx::query_as::<_, ArticleRef>(
        "SELECT url, title FROM articles WHERE id = $1",
    )
    .bind(article_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
```

> **`articles` を読むことの正当化**: `POST /api/read-later` は記事 **id** を受け取り、クライアントに生 URL を持たせない方針（レビュー指摘の「URL をサーバ側で引く」）。よって article_id→URL の解決が必須で、これは instapaper スライス内の **読み取り専用 SQL** で完結させる。foundation の `feed_overview`（feeds+articles を JOIN 読み）や、既存 `articles/repository.rs` が `feeds::domain::FeedId` を import している前例どおり、**読み取りのクロステーブル参照はこのコードベースで許容**。articles の**書き込み所有は移していない**ので「越境共通レイヤー」には当たらない。
>
> `query!` コンパイル時マクロは使わない（ビルドに DB 接続が要るため禁止）。すべて `query`/`query_as` のランタイムクエリ。

### 5.3 `service.rs`（`&AppState` を取り repository + http を統合）

```rust
use uuid::Uuid;

use super::domain::{
    classify_add_status, classify_auth_status, AddOutcome, AuthOutcome,
    InstapaperCredentials, InstapaperStatus, SaveUrl, StoredCredentials,
};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

// 実エンドポイントは instapaper.com/api で要確認。www. 有無に注意（§11）。
const ADD_URL: &str = "https://www.instapaper.com/api/add";
const AUTH_URL: &str = "https://www.instapaper.com/api/authenticate";

/// 保存前に Instapaper で検証してから永続化（誤入力を即座に弾く）。
/// 検証をスキップする代替は §11。
pub async fn save_credentials(state: &AppState, creds: InstapaperCredentials) -> AppResult<()> {
    verify(state, creds.username(), creds.password()).await?;
    repository::upsert_credentials(&state.db, creds.username(), creds.password()).await
}

pub async fn get_status(state: &AppState) -> AppResult<InstapaperStatus> {
    let configured = repository::get_credentials(&state.db).await?.is_some();
    Ok(InstapaperStatus { configured })
}

pub async fn clear_credentials(state: &AppState) -> AppResult<()> {
    repository::delete_credentials(&state.db).await
}

/// 記事を Instapaper に送る（05 の所有エンドポイント `POST /api/read-later` の本体）。
/// 順序: (1) 資格情報あり? なければ NotEnabled、(2) 記事あり? なければ NotFound、
///       (3) Instapaper へ転送。
/// 06（read-later）は本関数を **同一スライス内で拡張**し、(2)の後に read_later_items を
/// `pending` で upsert、(3)の結果で `added`/`failed` に更新する（HTTP 契約は不変）。
pub async fn add_to_read_later(state: &AppState, article_id: Uuid) -> AppResult<()> {
    let creds = repository::get_credentials(&state.db)
        .await?
        .ok_or_else(|| AppError::NotEnabled("Instapaper credentials are not set".into()))?;

    let article = repository::get_article_ref(&state.db, article_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // 保存済み記事の URL は本来 http(s) のはずだが、防御的に値オブジェクトへ通す。
    let url = SaveUrl::parse(article.url).map_err(AppError::Validation)?;
    send_to_instapaper(state, &creds, &url, Some(article.title)).await
}

/// Instapaper /api/add への低レベル転送プリミティブ。
async fn send_to_instapaper(
    state: &AppState,
    creds: &StoredCredentials,
    url: &SaveUrl,
    title: Option<String>,
) -> AppResult<()> {
    // Simple API は url 必須、title 任意（要確認）。form-encoded で送る想定。
    let mut form: Vec<(&str, String)> = vec![("url", url.as_str().to_string())];
    if let Some(t) = title {
        form.push(("title", t));
    }

    let resp = state
        .http
        .post(ADD_URL)
        .basic_auth(&creds.username, Some(&creds.password))
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?;

    let status = resp.status();
    match classify_add_status(status.as_u16()) {
        AddOutcome::Saved => Ok(()),
        AddOutcome::BadRequest => {
            let text = resp.text().await.unwrap_or_default();
            Err(AppError::Validation(format!("instapaper rejected the request: {text}")))
        }
        AddOutcome::Failed => {
            let text = resp.text().await.unwrap_or_default();
            Err(AppError::Upstream(format!("instapaper {status}: {text}")))
        }
    }
}

/// /api/authenticate で資格情報を検証。403 は誤資格情報 → Validation（保存フォームに表示）。
async fn verify(state: &AppState, username: &str, password: &str) -> AppResult<()> {
    let resp = state
        .http
        .post(AUTH_URL)
        .basic_auth(username, Some(password))
        .send()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?;

    let status = resp.status();
    match classify_auth_status(status.as_u16()) {
        AuthOutcome::Valid => Ok(()),
        AuthOutcome::Invalid => Err(AppError::Validation("invalid Instapaper credentials".into())),
        AuthOutcome::Failed => {
            let text = resp.text().await.unwrap_or_default();
            Err(AppError::Upstream(format!("instapaper {status}: {text}")))
        }
    }
}
```

> HTTP 呼び出しを `service.rs` 内に閉じるのは、本スライスに trait/dyn の抽象境界を作らない方針（`shared/llm` 以外に抽象境界を増やさない）に沿うため。`anthropic.rs` は専用 struct だが、あちらは差し替え可能な `LlmClient` trait の実装だから別ファイル。Instapaper は2つ目の実装予定が無いので struct 化も trait 化もしない。将来肥大化したら slice 内に `client.rs` を追加して切り出せばよい（slice 内に閉じる）。
>
> **dead_code に関する注意**: 本クレートは binary crate（`lib.rs` 無し）。`add_to_read_later` / `send_to_instapaper` / `verify` はすべて 05 内（handler / 互いの呼び出し）から呼ばれるため `-D warnings` でも未使用警告は出ない。06 が `add_to_read_later` を**拡張**しても呼び出し元は不変。

### 5.4 `handler.rs`（axum ハンドラ）

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{InstapaperCredentials, InstapaperStatus};
use super::service;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CredentialsBody {
    pub username: String,
    pub password: String,
}

pub async fn save_credentials(
    State(state): State<AppState>,
    Json(body): Json<CredentialsBody>,
) -> AppResult<Json<InstapaperStatus>> {
    let creds = InstapaperCredentials::parse(body.username, body.password)
        .map_err(AppError::Validation)?;
    service::save_credentials(&state, creds).await?;
    Ok(Json(InstapaperStatus { configured: true }))
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<InstapaperStatus>> {
    Ok(Json(service::get_status(&state).await?))
}

pub async fn delete_credentials(State(state): State<AppState>) -> AppResult<StatusCode> {
    service::clear_credentials(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ReadLaterBody {
    pub article_id: Uuid,
}

pub async fn add_read_later(
    State(state): State<AppState>,
    Json(body): Json<ReadLaterBody>,
) -> AppResult<StatusCode> {
    service::add_to_read_later(&state, body.article_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

### 5.5 `mod.rs`（routes）

```rust
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post, put};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/instapaper/credentials",
            put(handler::save_credentials).delete(handler::delete_credentials),
        )
        .route("/api/instapaper/status", get(handler::status))
        // 06（read-later）は同一ファイルでこの行に .get(handler::list) を足し、
        // service::add_to_read_later に status 永続化を加える（HTTP 契約は不変）。
        .route("/api/read-later", post(handler::add_read_later))
}
```

### 5.6 `features/mod.rs` への追加（2行のみ）

```rust
pub mod instapaper; // 既存 pub mod 群に追加（pub mod articles; feeds; health; stats; の並びに）
// router() 内の .merge チェーンに追加:
        .merge(instapaper::routes())
```

既存スライス（feeds/articles/stats/health）は一切触らない。

### 5.7 AppError の使い分け（`error.rs` は不編集）

| 状況 | バリアント | HTTP | レスポンス `error` 文字列（Display） |
|---|---|---|---|
| `read-later` 呼び出し時に資格情報が DB に無い | `NotEnabled` | 503 | `feature not yet enabled: Instapaper credentials are not set` |
| 入力 username/password が空 | `Validation` | 400 | `invalid input: username must not be empty` 等 |
| 保存時 `/api/authenticate` が 403（資格情報誤り） | `Validation` | 400 | `invalid input: invalid Instapaper credentials` |
| `read-later` の `article_id` に該当記事が無い | `NotFound` | 404 | `resource not found` |
| `add` 時 Instapaper が 400（URL 不正等） | `Validation` | 400 | `invalid input: instapaper rejected the request: ...` |
| `add`/`verify` 時 Instapaper が 403/5xx・ネットワーク障害（検証時の非 403 失敗含む） | `Upstream` | 502 | `upstream request failed: instapaper 5xx: ...` |
| DB エラー | `Database`（`?` で自動 `From`） | 500 | `internal error` |

> `add` の 403 を `Upstream`、保存検証の 403 を `Validation` に分けているのは意図的: 保存時は「ユーザーが今入力した資格情報が誤り」なのでフォームに 400 で返したい。`add` 時は「登録済み資格情報が無効化された」上流事象なので 502 が妥当。
> **チェック順序の明示**: `add_to_read_later` は (1) 資格情報 → (2) 記事存在 → (3) 転送 の順。資格情報未設定時は記事の有無に関わらず 503（機能ゲートを先に判定。`llm_client()` を先に判定する既存パターンと同型）。新バリアントは追加しない。

---

## 6. フロントエンド設計

### 6.1 `lib/api.ts` への追加（型1 + メソッド4）

型を追加（backend JSON をミラー）:

```ts
export interface InstapaperStatus {
  configured: boolean;
}
```

`api` オブジェクトにメソッド追加（既存 `http<T>()` を再利用。命名は既存の `動詞+リソース` に揃える）:

```ts
  getInstapaperStatus: () => http<InstapaperStatus>("/api/instapaper/status"),
  saveInstapaperCredentials: (creds: { username: string; password: string }) =>
    http<InstapaperStatus>("/api/instapaper/credentials", {
      method: "PUT",
      body: JSON.stringify(creds),
    }),
  deleteInstapaperCredentials: () =>
    http<void>("/api/instapaper/credentials", { method: "DELETE" }),
  // 05 が所有する POST /api/read-later の呼び口。記事 id を取る（生 URL は取らない）。
  // ArticleView の保存ボタン UI 導線は機能 06 が付ける。
  saveToReadLater: (articleId: string) =>
    http<void>("/api/read-later", {
      method: "POST",
      body: JSON.stringify({ article_id: articleId }),
    }),
```

> **メソッド数は 4 で一貫**（§2 スコープと一致）。旧ドラフトの `saveToInstapaper(url)`（生 URL を取る）は**廃止**。`POST /api/instapaper/add` という低レベル URL エンドポイントは設けず、公開契約は article 起点の `POST /api/read-later {article_id}` に一本化した（レビュー指摘の解消）。

### 6.2 新規 UI 部品 `components/ui/input.tsx`

バリアントが無い単純部品なので **自前 Tailwind（cva 不使用）**。`button.tsx` と同じく `cn(@/lib/utils)` + `splitProps` を使う点だけ揃える。oklch トークンで装飾（`bg-background`, `border-input`, `placeholder:text-muted-foreground`, `focus-visible:ring-2 ring-ring`）。

```tsx
import { splitProps, type ComponentProps } from "solid-js";
import { cn } from "@/lib/utils";

export function Input(props: ComponentProps<"input">) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <input
      class={cn(
        "flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm",
        "placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2",
        "focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        local.class,
      )}
      {...rest}
    />
  );
}
```

### 6.3 新規ルート `routes/Settings.tsx`

Instapaper 資格情報の登録/更新/削除フォームと接続状態表示。状態は **ローカル**（`createResource` で status 取得、`createSignal` でフォーム入力）。グローバルストアは不要。

骨子:
- `const [status, { refetch }] = createResource(api.getInstapaperStatus);`
- `username` / `password` の `createSignal`、`busy` シグナル、`error` シグナル。
- 送信: `await api.saveInstapaperCredentials({ username, password })` → 成功で `refetch()` し、フォーム（特に password）をクリア。失敗（400=資格情報誤り、502=障害）は `catch` で `error()` に表示。
- 状態表示: `status()?.configured` が true なら「接続済み」バッジ + 「資格情報を削除」ボタン（`await api.deleteInstapaperCredentials()` → `refetch()`）。false なら未接続表示。
- `password` 入力は `<Input type="password" autocomplete="off" />`。値はサーバから返らない（status は `configured` のみ）ので、更新時は再入力させる。
- 装飾は `card.tsx` で **Instapaper 用 Card を1枚**置く。ラベルは `text-sm font-medium`、説明は `text-xs text-muted-foreground`、保存ボタンは `Button`、削除は `Button variant="destructive"`。

> 土台設計では `/settings` はテーマ切替（機能 04）も同居予定。本機能は **Instapaper セクション（Card 1枚）のみ**を `Settings.tsx` に置き、04 とは非干渉に共存する。`Settings.tsx` が未存在なら本機能で新規作成し、04 が後からテーマ用 Card を追記する（先着が骨格を作り、後着は Card を足すだけ）。

### 6.4 ルーティング `index.tsx`

既存の `<Router root={App}>` 内に1ルート追加:

```tsx
import Settings from "./routes/Settings";
// ...
<Route path="/settings" component={Settings} />
```

設定画面への導線（Sidebar / ヘッダのリンク）は二ペインレイアウト（機能 10）が整備する。本機能では `/settings` を直接開けば使える状態にしておけば足りる（必要なら既存 `App.tsx` ヘッダに暫定リンクを1つ足してよい）。

### 6.5 Ark UI について

本機能で必要な UI は input / button / card のみで、いずれも自前 Tailwind で賄える。**Ark UI 部品は本機能では不要**。テーマ切替の `switch`（04）やドロップダウン等は別機能の担当。

---

## 7. API 契約

> すべて `/api` プレフィックス。Instapaper 側の実仕様（URL・パラメータ・ステータス）は instapaper.com/api で要確認。

### 7.1 `PUT /api/instapaper/credentials` — 資格情報の登録/更新
リクエスト:
```json
{ "username": "you@example.com", "password": "secret" }
```
レスポンス（200、検証成功して保存）:
```json
{ "configured": true }
```
エラー:
- 400 `{ "error": "invalid input: username must not be empty" }`（空入力）
- 400 `{ "error": "invalid input: invalid Instapaper credentials" }`（Instapaper が 403）
- 502 `{ "error": "upstream request failed: instapaper 500 ..." }`（Instapaper 障害）

### 7.2 `GET /api/instapaper/status` — 接続状態
レスポンス（200）:
```json
{ "configured": false }
```
**パスワード・ユーザー名は返さない。**

### 7.3 `DELETE /api/instapaper/credentials` — 資格情報の削除
レスポンス: `204 No Content`（行が無くても 204。冪等）

### 7.4 `POST /api/read-later` — 記事を Instapaper に保存
リクエスト:
```json
{ "article_id": "1f1c0e8a-..." }
```
レスポンス: `204 No Content`（Instapaper への転送成功）
エラー:
- 503 `{ "error": "feature not yet enabled: Instapaper credentials are not set" }`（資格情報未登録）
- 404 `{ "error": "resource not found" }`（article_id に該当記事なし）
- 400 `{ "error": "invalid input: instapaper rejected the request: ..." }`（Instapaper 400）
- 502 `{ "error": "upstream request failed: instapaper 403 ..." }`（資格情報無効/障害）

> **05 → 06 のシーム**: 本契約（`{article_id}` in / 204 out / 上記エラー）は **06 でも変えない**。06 は `service::add_to_read_later` の本体に `read_later_items` の `pending`→`added/failed` 書き込みと冪等化を足し、`GET /api/read-later`（一覧）を **同一 `.route("/api/read-later", ...)` に `.get()` 追加**する。フロントの `saveToReadLater(articleId)` も不変。

---

## 8. 依存関係

- **ブロックする機能（本機能に依存する）**:
  - **機能 06（read-later / 「後で読む」）**。06 は `read_later` マイグレーション（`read_later_items`、article_id を PK にして冪等化）を追加し、本 `instapaper` スライスに **追記**する: (a) `service::add_to_read_later` に status 永続化（`pending`→`added/failed`）を足す、(b) `GET /api/read-later`（一覧）を `mod.rs` の同一ルートに `.get()` で足す、(c) `ArticleView` に保存ボタンを置き `api.saveToReadLater(article.id)` を呼ぶ、(d) 失敗/再試行 UX。**06 は 05 の公開契約（ルート名・リクエスト/レスポンス・`saveToReadLater` シグネチャ）を一切変えない**。これが「05/06 シームを今確定し foundation の `/api/read-later` と整合させた」というレビュー反映の要点。
- **本機能が依存する機能**: 機能上の必須依存は無い（`instapaper` スライスは自己完結。`articles` テーブルは読み取りのみ参照し、既に存在する）。ソフトな協調:
  - 機能 04（ダークテーマ）/ 機能 10（二ペイン）と `/settings` ルート・設定画面導線を共有する（Card 単位で非干渉に共存。先着が `Settings.tsx` を新規作成、後着が追記）。
- 既存スライス（feeds / articles / stats / health）への変更は無し。`features/mod.rs` への2行追加のみが既存ファイルへの接触点。

---

## 9. テスト計画（TDD）

> **テスト配置の方針と foundation からの明示的な逸脱**: foundation backend §5 は「DB を触る結合テストは `backend/tests/`（実 DB）に置く（stats が前例）」とするが、実際には **`backend/tests/` は存在せず**、stats の結合テストは shell スクリプト（`scripts/test/api-stats.sh`）である。さらに本クレートは **binary crate（`lib.rs` 無し）**のため、`backend/tests/` の別クレートから `features::instapaper::repository::*` の内部関数を直接呼ぶことは **lib ターゲットを足さない限り不可能**（lib 化はスライス横断の構造変更で本機能のスコープ外）。
> したがって本書は次の二段で結合テストを置き、foundation の「`backend/tests/`」慣習からは意図的に逸脱する:
> 1. **リポジトリ往復（upsert/get/delete）の自動テスト**は `repository.rs` 内の `#[cfg(test)] mod tests` に置く（同一モジュールなので内部関数を直接呼べる）。実 DB を `DATABASE_URL` で叩き、`#[ignore]` を付けて通常の `cargo test`（DB 無し）を妨げない。**検証（verify）を経由せず DB 経路だけを網羅**する（レビュー指摘の「repository に自動カバレッジが無い」を解消）。
> 2. **HTTP 表面のスモークテスト**は実プロジェクトの前例どおり shell スクリプト（稼働スタックに curl）で置く。

### 9.1 単体テスト（`#[cfg(test)] mod tests` in `domain.rs`、外部 API も DB も不要）

`backend/src/features/instapaper/domain.rs` 末尾に追加。Red を先に書く。

| テスト | 意図 |
|---|---|
| `parse_rejects_empty_username` | 空 username を `Err` にする（`Validation` 経路） |
| `parse_rejects_empty_password` | 空 password を `Err` にする |
| `parse_trims_username` | username の前後空白を除去する |
| `parse_keeps_password_verbatim` | password は trim せず原文保持（前後空白も有意） |
| `parse_accepts_valid_credentials` | 正常系で `username()/password()` が取得できる |
| `save_url_accepts_http_and_https` | `http://`/`https://` を受理 |
| `save_url_rejects_missing_scheme` | スキーム無し URL を拒否 |
| `save_url_rejects_empty` | 空文字を拒否 |
| `classify_add_status_maps_2xx_to_saved` | 200/201 → `AddOutcome::Saved` |
| `classify_add_status_maps_400_to_bad_request` | 400 → `BadRequest` |
| `classify_add_status_maps_403_and_5xx_to_failed` | 403/500 → `Failed` |
| `classify_auth_status_maps_200_valid_403_invalid_else_failed` | 認証分類の3分岐（200/403/その他） |

> 外部 HTTP を叩く `add_to_read_later`/`send_to_instapaper`/`verify` の本体は単体テストせず、ステータス分類を純粋関数に切り出して網羅する。これで Instapaper を呼ばずに分岐ロジックを Red→Green できる。

### 9.2 リポジトリ往復テスト（`#[cfg(test)] mod tests` in `repository.rs`、実 DB / `#[ignore]`）

`backend/src/features/instapaper/repository.rs` 末尾に追加。`DATABASE_URL`（`just dev-db` の DB）で実 DB に接続し、`#[tokio::test]` + `#[ignore]`。マイグレーション適用済みの DB を前提（`cargo test -- --ignored` で実行、CI 任意）。**`verify`（ネットワーク）を経由しないので Instapaper 不要。**

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
    async fn credentials_roundtrip_upsert_get_delete() {
        let pool = pool().await;
        // クリーンな前提に揃える
        delete_credentials(&pool).await.unwrap();
        assert!(get_credentials(&pool).await.unwrap().is_none());

        upsert_credentials(&pool, "user@example.com", "pw1").await.unwrap();
        let got = get_credentials(&pool).await.unwrap().expect("row present");
        assert_eq!(got.username, "user@example.com");
        assert_eq!(got.password, "pw1");

        // 2回目は単一行を更新（singleton）
        upsert_credentials(&pool, "user2@example.com", "pw2").await.unwrap();
        let got = get_credentials(&pool).await.unwrap().expect("row present");
        assert_eq!(got.username, "user2@example.com");
        assert_eq!(got.password, "pw2");

        delete_credentials(&pool).await.unwrap();
        assert!(get_credentials(&pool).await.unwrap().is_none());
    }
}
```

| テスト | 意図 |
|---|---|
| `credentials_roundtrip_upsert_get_delete` | upsert→get（挿入確認）→ 再 upsert（singleton 更新確認）→ delete→get（None 確認）。**repository の DB 経路を network 抜きで自動カバー** |

### 9.3 HTTP スモークテスト（稼働スタックへの shell スクリプト = 実プロジェクト前例）

`scripts/test/api-instapaper.sh` を新設（`scripts/test/api-stats.sh` と同型。nginx `:8081` 経由）。**Instapaper 本体は叩かない**範囲を検証。資格情報を先に削除して決定的にする:

| 手順 / アサーション | 意図 |
|---|---|
| `DELETE /api/instapaper/credentials` → 204 | DELETE 配線確認 + 以降を未設定状態に固定 |
| `GET /api/instapaper/status` → 200 かつ JSON `configured` が `false` | スライス合成 + status（外部呼び出し無し）+ 未設定表現 |
| `POST /api/read-later`（body `{"article_id":"00000000-0000-0000-0000-000000000000"}`）→ 503 | `NotEnabled` を**資格情報チェックで先に**返す配線（外部呼び出し前に弾かれる。記事不在より前に 503） |

> `PUT /credentials` と `add` の成功パスは Instapaper 実 API への到達が要るため自動 CI では検証しない（ライブ資格情報が必要）。手動手順は §10 step 13。

### 9.4 フロント（手動 + 型）
- `tsc` 型チェック（`just lint`）で `api.ts` / `Settings.tsx` / `input.tsx` の型整合を確認。
- 手動: `/settings` で誤資格情報→エラー表示、正資格情報→「接続済み」表示、削除→未接続表示。

---

## 10. 実装手順（順序付きチェックリスト）

1. **マイグレーション番号を採番**: `ls backend/migrations/` で最大番号を確認し、`+1` を採る（現状最新が `0001_init.sql` のみなら **`0002_instapaper.sql`**）。**既に高い番号が永続 DB に適用済みなら、より小さい番号を新規追加しない**（§4.1 の out-of-order 注意）。
2. **マイグレーション作成**: 採番したファイルを §4.2 の SQL で新規作成（既存ファイルは触らない）。
3. **ドメイン（Red 先行）**: `backend/src/features/instapaper/domain.rs` を作り、§5.1 の型と純粋関数 + §9.1 の `#[cfg(test)] mod tests` を書く。まずテストが落ちる（型未実装）ことを確認 →実装で Green に。`cargo test`（該当クレート）で実行。
4. **repository**: `repository.rs` を §5.2 で作成（`query`/`query_as` のみ、`query!` 不可）。§9.2 の `#[cfg(test)] mod tests`（`#[ignore]`）も書く。
5. **service**: `service.rs` を §5.3 で作成。`state.http` を使い `basic_auth` + `form`。定数 URL は instapaper.com/api で確認して確定。
6. **handler**: `handler.rs` を §5.4 で作成。資格情報 parse の `Err(String)` を `map_err(AppError::Validation)`。
7. **mod + 合成**: `mod.rs` を §5.5 で作成。`features/mod.rs` に `pub mod instapaper;` と `.merge(instapaper::routes())` を追加（§5.6）。
8. **ビルド & lint**: `just lint`（clippy `-D warnings` / tsc）を通す。`reqwest` の `basic_auth`/`form` はデフォルト機能で使える（§3 で確認済み。Cargo.toml 追加不要の見込み。万一リンクエラーなら feature 要否を判断）。
9. **DB 起動 & マイグレーション**: `just dev-db` →（バックエンド起動で自動 migrate、または `just migrate`）。
10. **リポジトリ往復テスト実行**: `DATABASE_URL=... cargo test -- --ignored`（または該当テスト名指定）で §9.2 を Green に。
11. **HTTP スモークスクリプト**: `scripts/test/api-instapaper.sh` を §9.3 で作成・`chmod +x`・実行。DELETE=204 / status `configured:false` / 未設定 read-later=503 を assert。
12. **フロント**: `frontend/src/lib/api.ts` に型 `InstapaperStatus` と4メソッド（§6.1）。`components/ui/input.tsx`（§6.2）、`routes/Settings.tsx`（§6.3）を作成。`index.tsx` に `/settings` ルート追加（§6.4）。必要なら `App.tsx` ヘッダに暫定リンク。`just lint` の tsc を通す。
13. **手動 E2E**: 実 Instapaper 資格情報で `/settings` から `PUT`→`status:true`→（一時的に curl 等で）`POST /api/read-later {article_id}`（Instapaper アカウントに記事が入るか目視）→`DELETE`→`status:false` を確認。
14. **コミット**: マイグレーション・スライス・スクリプト・フロントをまとめて。秘密情報/`.env` はコミットしない。

---

## 11. リスク・未決事項・代替案

- **【要確認・最重要】Instapaper Simple Developer API の実仕様**: エンドポイント URL（`https://www.instapaper.com/api/add` / `/api/authenticate` の `www.` 有無）、必須/任意パラメータ名（`url` 必須、`title`/`selection` 任意の想定）、成功ステータス（`201 Created` か `200` か）、認証失敗（`403`）・不正リクエスト（`400`）・障害（`5xx`）のコード、レート制限の有無を **instapaper.com/api で実装時に確認**。`classify_add_status` / `classify_auth_status` の境界値はそこで確定する。
- **マイグレーション番号の順序ハザード（解消済みだが運用注意）**: §4.1 のとおり、`run_migrations` は `set_ignore_missing` を呼ばないため、**先に高い番号を適用した永続 DB にあとから低い番号を足すと起動が壊れる**。05 を単独先行リリースするなら最小空き整数（`0002`）を取り、02/06 はそれ以降へ繰り上げること。土台設計の番号表（0002/0003/0004）は機能対応の覚書であり、適用順序の保証ではない。
- **資格情報の平文保存**: 家庭内 LAN・単一ユーザー前提の MVP 判断。Simple API は平文パスワードでの Basic 認証が必須のため可逆保存が要る。緩和策として `GET /status` は `configured` のみ返し、`StoredCredentials` に `Serialize` を付けない。将来は env 由来の鍵で対称暗号化（保存時暗号化・取得時復号）に拡張可能。テーブル定義は変えずに列値の暗号化で対応できる。
- **保存時検証のネットワーク依存（テスト容易性と結合）**: `save_credentials` は upsert の **前**に `verify`（`/api/authenticate`）を同期で叩く。利点は即時フィードバック、欠点は (a) Instapaper 障害時に登録不可（`Upstream`/502）、(b) `PUT` を叩く自動テストが必ずライブ Instapaper に当たること。後者は §9.2 で **repository を直接叩く `#[ignore]` テスト**を別に用意して回避済み（DB 経路は network 抜きで自動カバー）。**代替案**: `save_credentials` から `verify` 呼び出しを外し「保存のみ + `add` 時にエラー検知」に切り替える（1行削除で可能）。本書は UX を優先して「検証あり」を既定とするが、運用方針で切替可。
- **`/api/authenticate` が存在しない/挙動が違う場合**: Simple API に認証専用エンドポイントが無い、または期待と異なる場合は、検証を諦め「保存のみ + `add` 時にエラー表示」へフォールバック（上の代替案と同じ）。`/api/add` に実 URL を1回送って成否を見る方式は副作用が大きいので採らない。instapaper.com/api で要確認。
- **`reqwest` の機能フラグ（確認済み・非リスク）**: `default-features=false` でも `.basic_auth()`/`.form()` は pin された 0.12.x で使える（feature gate 無し、`serde_urlencoded` は hard dep、`form` feature は 0.12.x に存在しない）。Cargo.toml 変更不要。Context7 等が「`form` は feature が要る」と示す場合は将来の master 向けリファクタの記述で、pin バージョンには当てはまらない。
- **`title` の扱い**: `add` で `title` を送るかは任意。Instapaper 側がページタイトルを自動取得するなら省略してよい。本機能では `articles.title`（NOT NULL）を渡す実装にしておく。機能 06 が UI 導線を作る際に、フィードのタイトルを使うか Instapaper の自動取得に任せるかを最終決定する。
- **`POST /api/read-later` のチェック順序**: 資格情報 → 記事存在 → 転送。未設定時は記事の有無に関わらず 503（機能ゲート優先）。記事不在より資格情報を先に判定する点は §5.7・§9.3 の前提。06 が status 行を書く際は「記事存在を確認した後」に `pending` を upsert すること（存在しない article_id に対する行を作らない）。
- **二重登録/競合**: 単一ユーザー singleton のため、複数タブ同時 `PUT` でも `ON CONFLICT (id) DO UPDATE` で最後の書き込みが勝つだけ。問題にならない。`POST /api/read-later` の冪等化（同一記事の二重送信抑止）は **06** の `read_later_items`（article_id PK）が担う。05 単体ではリクエストの都度 Instapaper へ転送する。
