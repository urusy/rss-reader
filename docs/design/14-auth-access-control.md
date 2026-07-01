# 14 認証 / アクセス制御

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッション（effort 低め）の実装者。本書だけで着手・完了できるよう、再利用資産・設定・関数シグネチャ・ルート文字列・JSON 例・実装手順まで具体化する。
> **重要な性質**: 本機能は「横断的アクセス制御ミドルウェア（`shared/`）」＋「ログイン用の薄い縦スライス1枚（`features/auth/`）」の二本立て。`shared/llm` 以外に trait/dyn は足さない。`error.rs` は編集しない（後述 §5.6 のとおり 401 はミドルウェア/ハンドラが **生 `Response`** を返して表現する）。

---

## 1. 概要

現状この RSS リーダーは **`/api` を無認証で公開**している。家庭内 LAN 前提とはいえ、要約/翻訳は **課金される Anthropic API キー**を裏で消費するため、LAN に侵入した端末や誤って外部へポート公開した場合に **第三者がトークンを浪費できてしまう**。本機能は、**単一トークン（`AUTH_TOKEN` 環境変数）**による横断的アクセス制御を導入し、`/api` 配下を保護する。単一ユーザー前提なので、ユーザー管理・パスワードハッシュ・多要素などは持たず、「正しいトークンを提示した者だけが API を使える」という最小構成に絞る。

実体は次の3点に集約される。

1. **横断ミドルウェア `shared/auth.rs`**: すべての保護対象ルートの手前で `Authorization: Bearer <token>` を検証する。`AUTH_TOKEN` が未設定なら **素通し（=認証無効）**で、既存の LAN デプロイを壊さない「任意機能」パターン。トークン未一致は **401**。**`/api/health` 系とログイン系は除外**（保護をかけない）。
2. **薄い縦スライス `features/auth/`**: 認証ゲートのためのフロント向け補助 API を提供する。`GET /api/auth/status`（認証が必要か）と `POST /api/auth/login`（トークン検証＝フロントがトークンを保存する前の妥当性確認）。**DB は使わない**（5ファイル構成のうち `repository.rs` は省略。`health` スライスが `handler`+`mod` のみで成立している前例に倣う）。
3. **フロントのトークン保持と付与**: `lib/auth.ts`（localStorage + モジュール signal。`lib/theme.ts` と同型）でトークンを保持し、`lib/api.ts` の `http<T>()` が全リクエストに `Authorization` ヘッダを付与する。401 を受けたらトークンを破棄しログイン画面（ゲート）を表示する。

> **「セッション」について**: 起票意図は「単一トークン or セッション(AUTH_TOKEN env / ログイン)」だが、単一ユーザー・単一トークンでは **サーバ側セッションストアは不要**（トークンそのものが bearer 資格情報）。「ログイン」は UX 上の語であり、実体は「トークンを localStorage に保存し、以降のヘッダに載せる」こと。複数セッショントークンを失効管理したい将来拡張のみ DB セッションが要る（§4.2 / §11 で `0006` を暫定採番し非スコープ化）。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）
- **設定追加**: `shared/config.rs` に `auth_token: Option<String>`（`AUTH_TOKEN` 環境変数、空文字は `None` 扱い）。`.env.example` に項目追加。
- **横断ミドルウェア** `backend/src/shared/auth.rs`: `require_auth`（axum `middleware::from_fn_with_state`）＋ 定数時間比較 `constant_time_eq`（純粋関数・単体テスト対象）＋ 401 応答ヘルパ `unauthorized()`。
- **ルータ合成の変更** `backend/src/features/mod.rs`: 「公開ルータ（health + auth）」と「保護ルータ（その他全スライス）」に分け、保護ルータにだけ `require_auth` レイヤを適用して merge する。**既存スライスのコードは一切触らない**（変更点は合成ルートのみ）。
- **新スライス** `backend/src/features/auth/`（`domain` / `service` / `handler` / `mod`。`repository` は無し）。
  - `GET /api/auth/status` → `{ "required": bool }`（`AUTH_TOKEN` 設定有無。**トークン値は絶対に返さない**）。**公開**（ミドルウェア除外）。
  - `POST /api/auth/login { token }` → 一致で `200 { "ok": true }`、不一致で `401`。フロントの「保存前検証 / 起動時チェック」兼用。**公開**（ミドルウェア除外）。
- **フロント**: `lib/auth.ts`（トークンの保存/取得/破棄 + 反応的ゲート状態）、`lib/api.ts` の `http<T>()` にヘッダ付与＋401ハンドリング、ログイン UI（`components/auth/LoginGate.tsx`）を `App.tsx` の最外殻に差し込む。`lib/api.ts` に型 `AuthStatus` と2メソッド（`getAuthStatus` / `login`）。
- **テスト**: `domain.rs` の `constant_time_eq` / `AuthToken::parse` 単体テスト、`auth.rs` 内のミドルウェア結合テスト（`tower::ServiceExt::oneshot`、DB 不要）、`scripts/test/api-auth.sh`（稼働スタックへ curl）、`lib/auth.ts` の vitest（feature 04 で導入済み）。

### 非スコープ（本機能では実装しない）
- **複数ユーザー / ユーザー管理 / 役割（RBAC）**。単一ユーザー単一トークン前提。
- **パスワードハッシュ化・ソルト**（共有シークレットは env の平文トークン。家庭内 LAN・単一ユーザー前提。§11）。
- **サーバ側セッションストア / 失効リスト / リフレッシュトークン / JWT**。将来「複数端末トークンの個別失効」が要るなら `0006_auth_sessions.sql`（§4.2 / §11）で拡張。本 MVP は **DB・マイグレーション無し**。
- **レート制限 / ブルートフォース対策のロックアウト**（トークンは十分長い乱数を前提とする運用ガイドで代替。§11）。
- **CORS / TLS の厳格化**（`features/mod.rs` の `CorsLayer::permissive()` と nginx の TLS 終端は別タスク。本機能はトークン検証のみ）。
- **ログイン以外の新ルート設計の変更**。既存 API の契約は不変（ヘッダ要求が増えるだけ）。

---

## 3. 既存実装の再利用

実ファイルを確認済み（パスは絶対）。以下を **再利用し、車輪の再発明をしない**。

| 再利用資産 | 実体（確認済みファイル） | 本機能での使い方 |
|---|---|---|
| 任意機能 = 未設定で無効化するパターン | `shared/config.rs::anthropic_api_key`（`std::env::var(...).ok().filter(\|v\| !v.is_empty())`） | `auth_token` を同型で読む。`None` のときミドルウェアは素通し（認証無効）。`ANTHROPIC_API_KEY` 流儀をそのまま踏襲 |
| `AppState { db, config, http }` | `shared/state.rs`（`#[derive(Clone)]`、`config: Arc<AppConfig>`） | ミドルウェアは `State<AppState>` から `state.config.auth_token` を読む。新フィールド・新 state は作らない |
| ルータ合成と横断レイヤ | `features/mod.rs::router()`（`.layer(TraceLayer::new_for_http())` / `.layer(CorsLayer::permissive())` を **ルータ全体に** 適用済み） | 横断レイヤを足す前例がここにある。auth レイヤは「保護対象サブルータにだけ」適用して health/login を除外する（§5.5） |
| `repository.rs` を持たないスライス | `features/health/`（`mod.rs` + `handler.rs` のみ、DB 非依存。`mod handler;` → `routes()`） | `auth` スライスも DB を持たないので `repository.rs` を作らない。health と同じ最小構成 |
| スライス構成 + `routes()` | `articles/mod.rs`・`feeds/`・`instapaper/`（`fn routes() -> Router<AppState>`、`.route("/path", get(...).post(...))`） | `auth` を `domain/service/handler/mod` で作る |
| 値オブジェクト `parse() -> Result<_, String>` | `feeds/domain.rs::FeedUrl::parse`、`instapaper/domain.rs::InstapaperCredentials::parse`（`trim` + 空チェック、`#[cfg(test)] mod tests` 付き） | `AuthToken::parse`（空トークン拒否）を同型で新設。`Err(String)` は `map_err(AppError::Validation)` |
| 純粋関数を切り出して TDD | `instapaper/domain.rs::classify_add_status`（HTTP を叩かず分類ロジックを単体テスト） | `constant_time_eq` / `verify`（一致判定）を純粋関数化し Red→Green |
| AppError と `IntoResponse` | `shared/error.rs`（6 バリアント。401 は**無い**。`IntoResponse` で `Json({"error": <Display>})`） | **error.rs は編集しない**。401 はミドルウェア/ハンドラが生 `Response` で返す（§5.6） |
| フロント API クライアント | `lib/api.ts`（`http<T>()` は 204→`undefined`、`errorStatus(e)` で先頭3桁ステータス抽出、`api` に `動詞+リソース` 命名でメソッド集約） | `http<T>()` にヘッダ付与＋401処理を追記。`errorStatus()` を 401 検知に再利用。2メソッド追加 |
| クライアント状態をモジュール signal で持つ | `lib/theme.ts`（localStorage + モジュールスコープ signal + `initTheme()`。store に依存しない） | `lib/auth.ts` を同型で新設（store を汚さない。単一ユーザーのクライアント状態は DB に持たない方針＝README §「単一ユーザ前提」に合致） |
| 自前 UI 部品 | `components/ui/input.tsx`（`type="password"` 可）・`button.tsx`・`card.tsx` | ログイン UI を組む。新規 UI 部品は作らない |
| アプリ最外殻 | `App.tsx`（`<AppProvider>` で全体を包む二ペインシェル。機能10で実装済み） | `LoginGate` を `App.tsx` の `<AppProvider>` 直下に1枚差し込む（§6.3） |
| 起動時初期化の差し込み点 | `index.tsx`（`initTheme()` を `render()` 前に呼ぶ前例） | 必要なら `initAuth()`（localStorage→signal 同期）を同じ場所に置く（§6.2） |
| フロントのテスト基盤 | feature 04 で導入済みの **vitest + jsdom**（`lib/theme.ts` をテスト） | `lib/auth.ts` の純粋ロジックを同じ流儀でテスト |
| HTTP スモークの慣習 | `scripts/test/api-stats.sh` / `api-instapaper.sh`（稼働スタックへ curl、HTTP コードと JSON キーを assert） | `scripts/test/api-auth.sh` を同型で新設（§9.3） |

> **依存追加は不要（確認済み）**: ミドルウェアは axum 0.8 の `middleware::from_fn_with_state`（axum 本体）で書ける。結合テストの `oneshot` は既存依存 `tower = "0.5"`（`tower::ServiceExt`）で足りる。定数時間比較は外部クレート（`subtle` / `constant_time_eq`）を**足さず**、ループで自前実装する（§5.1）。`backend/Cargo.toml` は変更しない見込み。

---

## 4. データモデル

### 4.1 MVP（本スコープ）: スキーマ変更なし・マイグレーション無し

単一トークン方式では **共有シークレットは `AUTH_TOKEN` 環境変数**に置く（要約機能の `ANTHROPIC_API_KEY` と同じく **オペレーター設定**であり、エンドユーザーが UI から変える値ではないため env が適切）。サーバ側にセッション行を持たないので **新テーブル・新カラム・マイグレーションは無い**。`feeds`/`articles` 等への変更も無し。

### 4.2 将来拡張（非スコープ・着手前に最新番号を確認）

「端末ごとに別トークンを発行し個別失効したい」段階になったら、サーバ側セッションを導入する。その場合のみ新マイグレーションを **追記**する。

> **マイグレーション番号の採番ルール（必読）**: 現状の最新は **`0005_search.sql`**。本機能は MVP ではマイグレーションを足さないが、将来拡張で足すなら **`0006_*` 以降を暫定採番**する。`main.rs` の `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を呼ばないため、**適用済み最大バージョンより小さい未適用ファイルを後から足すと起動が壊れる**。よって **着手直前に必ず `ls backend/migrations/` で最新番号を確認**し、その時点の最小空き整数（`0006` 以降）を取ること。既存 `0001`〜`0005` は編集しない（追記のみ）。

将来用 SQL（**今は作らない**。`backend/migrations/0006_auth_sessions.sql` 暫定）:

```sql
-- FUTURE / OPTIONAL (NOT part of the single-token MVP).
-- Per-device session tokens so individual devices can be revoked.
-- Numbering is provisional: confirm the latest migration number before adding.
CREATE TABLE IF NOT EXISTS auth_sessions (
    token_hash   TEXT PRIMARY KEY,          -- SHA-256 of the opaque session token (never store raw)
    label        TEXT,                       -- optional human label (e.g. "iPhone")
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ                 -- NULL = no expiry
);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires_at ON auth_sessions (expires_at);
```

MVP のミドルウェア（§5.1）はこの行を一切参照しない。拡張時は `require_auth` 内のトークン照合を「env トークン一致 **または** `auth_sessions` にハッシュ一致行あり」に広げ、`features/auth` に発行/失効ハンドラと `repository.rs` を**追記**する（スライス内に閉じる）。

---

## 5. バックエンド

### 5.1 `shared/auth.rs`（横断ミドルウェア・新規ファイル）

`backend/src/shared/auth.rs` を新設し、`shared/mod.rs`（または `shared.rs` の `pub mod` 宣言箇所）に `pub mod auth;` を1行追加する。

```rust
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::shared::state::AppState;

/// 認証ミドルウェア本体。`AUTH_TOKEN` 未設定なら素通し（認証無効）。
/// 設定済みなら `Authorization: Bearer <token>` を定数時間比較し、不一致は 401。
///
/// 注意: `error.rs` は編集しない方針なので 401 は AppError ではなく生 Response で返す。
pub async fn require_auth(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // 1) 認証無効（AUTH_TOKEN 未設定）なら何もせず通す。
    let Some(expected) = state.config.auth_token.as_deref() else {
        return next.run(req).await;
    };

    // 2) Authorization: Bearer <token> を取り出す。
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));

    // 3) 定数時間比較。
    match provided {
        Some(token) if constant_time_eq(token.as_bytes(), expected.as_bytes()) => {
            next.run(req).await
        }
        _ => unauthorized(),
    }
}

/// 401 応答（WWW-Authenticate 付き）。本文形式は既存の AppError と揃える。
pub fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(json!({ "error": "unauthorized" })),
    )
        .into_response()
}

/// タイミング攻撃を避ける定数時間バイト比較（純粋関数 = 単体テスト対象）。
/// 長さが違っても早期 return せず、固定回ループして 0/非0 を畳み込む。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        // 長さ不一致でも一定の仕事をしてから false（長さは秘密ではないので許容）。
        let mut acc: u8 = 1;
        for &x in a {
            acc |= x; // 早期 return しないためのダミー累積
        }
        return acc == 0 && b.is_empty();
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
```

> **設計判断**:
> - **401 を生 Response で返す**: `AppError` には 401 相当が無く、`error.rs` は不編集が鉄則（CHEATSHEET）。`Validation`(400) に丸めるのはセマンティクス的に誤り（プロキシ/クライアントは 401 を特別扱いする）。よってミドルウェアは `AppError` を経由せず `unauthorized()` を直接返す。本文は `{"error": "..."}` と既存 `IntoResponse` の形式に合わせる。
> - **trait/struct 化しない**: 認証は差し替え予定の無い単一実装。`shared/llm` 以外に抽象境界を増やさない方針に従い、関数とミドルウェアだけで構成する。
> - **定数時間比較を自前実装**: `subtle`/`constant_time_eq` クレートを足さない（依存最小化）。純粋関数なので単体テストで正しさを担保する。

### 5.2 `features/auth/domain.rs`（値オブジェクト + 純粋ロジック）

```rust
use serde::Serialize;

/// 検証済みのトークン入力。空文字は構築時に弾く（不正状態を表現不能にする）。
/// Serialize は付けない（トークンをクライアントへ返さない）。
#[derive(Debug, Clone)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let s = raw.into();
        // トークンは前後空白も有意になり得るが、誤コピペ救済のため trim する運用とする。
        let t = s.trim();
        if t.is_empty() {
            return Err("token must not be empty".into());
        }
        Ok(Self(t.to_string()))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

/// GET /api/auth/status が返す安全な射影。required のみ公開（トークン値は出さない）。
#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    pub required: bool,
}

/// POST /api/auth/login の成功レスポンス。
#[derive(Debug, Clone, Serialize)]
pub struct LoginResponse {
    pub ok: bool,
}
```

### 5.3 `features/auth/service.rs`（`&AppState` を取り config を参照）

```rust
use super::domain::{AuthStatus, AuthToken};
use crate::shared::auth::constant_time_eq;
use crate::shared::state::AppState;

/// 認証が必要か（AUTH_TOKEN 設定有無）。トークン値は読まない/返さない。
pub fn auth_status(state: &AppState) -> AuthStatus {
    AuthStatus {
        required: state.config.auth_token.is_some(),
    }
}

/// ログイン検証結果。ハンドラが HTTP ステータスへ写像する（§5.4）。
#[derive(Debug, PartialEq, Eq)]
pub enum LoginOutcome {
    /// 認証無効（AUTH_TOKEN 未設定）。誰でも通る状態 → 200 を返してフロントにゲート不要を伝える。
    Disabled,
    /// トークン一致 → 200。
    Ok,
    /// トークン不一致 → 401。
    Invalid,
}

/// 受け取ったトークンを config の AUTH_TOKEN と定数時間比較する（純粋寄り = テスト容易）。
pub fn verify_login(state: &AppState, token: &AuthToken) -> LoginOutcome {
    match state.config.auth_token.as_deref() {
        None => LoginOutcome::Disabled,
        Some(expected) if constant_time_eq(token.as_str().as_bytes(), expected.as_bytes()) => {
            LoginOutcome::Ok
        }
        Some(_) => LoginOutcome::Invalid,
    }
}
```

> DB を使わないので `repository.rs` は無い（`health` スライスと同じ最小構成）。将来のセッション方式では `repository.rs` を追記し `verify_login` を「env 一致 or セッション行一致」に広げる（§4.2）。

### 5.4 `features/auth/handler.rs`（axum ハンドラ）

```rust
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use super::domain::{AuthStatus, AuthToken, LoginResponse};
use super::service::{self, LoginOutcome};
use crate::shared::auth::unauthorized;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// GET /api/auth/status — フロントがゲート要否を判断するための公開エンドポイント。
pub async fn status(State(state): State<AppState>) -> Json<AuthStatus> {
    Json(service::auth_status(&state))
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub token: String,
}

/// POST /api/auth/login — トークン検証（保存前検証 / 起動時チェック兼用）。
/// 401 は生 Response（unauthorized()）で返す。AppError は使わない（401 が無いため）。
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Result<Json<LoginResponse>, Response> {
    // 空トークンは 400（Validation）。AppError は IntoResponse 済みなので Response へ。
    let token = AuthToken::parse(body.token)
        .map_err(AppError::Validation)
        .map_err(IntoResponse::into_response)?;

    match service::verify_login(&state, &token) {
        // Disabled でも UX 上は「通った」と返す（フロントはゲートを出さない）。
        LoginOutcome::Ok | LoginOutcome::Disabled => Ok(Json(LoginResponse { ok: true })),
        LoginOutcome::Invalid => Err(unauthorized()),
    }
}

// AppResult は将来 DB セッション化したときの伝播用に import（MVP では未使用なら外してよい）。
#[allow(unused_imports)]
use AppResult as _AppResultMarker;
```

> 注: `AppResult` の import は MVP では使わない。`-D warnings` を通すため、未使用なら **import 行ごと削除**する（上の `#[allow]` ダミーは説明用。実装時は素直に未使用 import を消すこと）。

### 5.5 `features/auth/mod.rs`（routes）と `features/mod.rs` の合成変更

`features/auth/mod.rs`:

```rust
pub mod domain;
pub mod handler;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

/// auth スライスは「公開」ルートのみ（status / login）。保護ミドルウェアの対象外に置く。
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/status", get(handler::status))
        .route("/api/auth/login", post(handler::login))
}
```

`features/mod.rs` を **公開ルータ / 保護ルータ** に分けて合成する（既存スライスのコードは不変。変更は合成ルートのみ）:

```rust
pub mod articles;
pub mod auth;          // ← 追加
pub mod feed_overview;
pub mod feeds;
pub mod folders;
pub mod health;
pub mod instapaper;
pub mod search;
pub mod stats;

use axum::{middleware, Router};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::shared::auth::require_auth;   // ← 追加
use crate::shared::state::AppState;

pub fn router(state: AppState) -> Router {
    // 認証を要求しない公開ルート: ヘルスチェックと auth スライス（login/status）。
    let public = Router::new()
        .merge(health::routes())
        .merge(auth::routes());          // ← auth は公開側へ 1 行 merge

    // 保護対象: それ以外の全スライス。サブルータ単位で require_auth を適用する。
    let protected = Router::new()
        .merge(feeds::routes())
        .merge(articles::routes())
        .merge(stats::routes())
        .merge(feed_overview::routes())
        .merge(folders::routes())
        .merge(instapaper::routes())
        .merge(search::routes())
        .layer(middleware::from_fn_with_state(state.clone(), require_auth)); // ← 保護レイヤ

    Router::new()
        .merge(public)
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive()) // tighten before exposing beyond your LAN
        .with_state(state)
}
```

> **なぜサブルータ分割か**: axum の `.layer()` は適用先のルータ配下すべてに掛かる。`/api/health` とログイン系を除外するには「保護ルータにだけレイヤを適用 → 公開ルータと merge」が最も明快。CHEATSHEET の「新スライス = `.merge()` 1行」は満たしつつ（auth は public へ 1 行 merge）、横断レイヤは TraceLayer/CorsLayer と同じく合成ルートで足す。これは「既存スライス横断の密結合な共通レイヤーをコード内に作る」こととは異なり、合成（DI）地点の配線変更にとどまる。

### 5.6 401 の表現（`error.rs` は不編集）

| 状況 | 返し方 | HTTP | レスポンス本文 |
|---|---|---|---|
| 保護ルートにトークン無し/不一致 | `shared::auth::unauthorized()`（生 Response） | 401 | `{ "error": "unauthorized" }` ＋ `WWW-Authenticate: Bearer` |
| `POST /api/auth/login` のトークン不一致 | `unauthorized()` | 401 | 同上 |
| `POST /api/auth/login` の `token` が空 | `AppError::Validation` → `IntoResponse` | 400 | `{ "error": "invalid input: token must not be empty" }` |
| `AUTH_TOKEN` 未設定（認証無効） | ミドルウェアは素通し / login は `{ok:true}` | 200 | 通常応答 |
| `/api/health`・`/api/auth/*` | 常に到達可能（保護対象外） | — | 各エンドポイント既定 |

> **新 `AppError` バリアントは追加しない**。401 はミドルウェア/ハンドラが生 Response を返すことで表現する。`error.rs` は触らない。

---

## 6. フロントエンド

### 6.1 `lib/auth.ts`（トークン保持 + 反応的ゲート状態・新規）

`lib/theme.ts` と同型。モジュールスコープ signal + localStorage。store（`lib/store.tsx`）は汚さない。

```ts
import { createSignal } from "solid-js";

const KEY = "auth_token";

// 起動時に localStorage から復元。
const [token, setTokenSignal] = createSignal<string | null>(
  typeof localStorage !== "undefined" ? localStorage.getItem(KEY) : null,
);

export const authToken = token; // 反応的に読む（ゲートが購読）

export function setToken(t: string): void {
  localStorage.setItem(KEY, t);
  setTokenSignal(t);
}

export function clearToken(): void {
  localStorage.removeItem(KEY);
  setTokenSignal(null);
}

export function getToken(): string | null {
  return token();
}
```

### 6.2 `lib/api.ts` の変更（ヘッダ付与 + 401 処理 + 2メソッド）

`http<T>()` に `Authorization` ヘッダを付与し、401 を受けたらトークンを破棄する（ゲートが反応して再ログインを促す）。`getToken`/`clearToken` を import する。

```ts
import { getToken, clearToken } from "@/lib/auth";

// 型を追加（backend JSON をミラー）:
export interface AuthStatus {
  required: boolean;
}

async function http<T>(path: string, init?: RequestInit): Promise<T> {
  const token = getToken();
  const res = await fetch(path, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(init?.headers ?? {}),
    },
  });
  if (res.status === 401) {
    clearToken(); // 失効/誤トークン → ゲート表示へ
  }
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}: ${text}`);
  }
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}
```

`api` オブジェクトにメソッド追加（既存 `http<T>()` を再利用）:

```ts
  // ゲート要否（公開エンドポイント。トークン無しでも 200）。
  getAuthStatus: () => http<AuthStatus>("/api/auth/status"),
  // トークン検証（保存前/起動時チェック）。不一致は 401 を投げる。
  login: (token: string) =>
    http<{ ok: boolean }>("/api/auth/login", {
      method: "POST",
      body: JSON.stringify({ token }),
    }),
```

> **注意（merge 後の整合）**: 上の `http<T>` は既存実装（`init` を後ろに展開していた）を **ヘッダ合成順を保つよう書き換える**。既存呼び出し側（`method`/`body` 指定）はそのまま動く。`errorStatus(e)`（先頭3桁抽出）は 401 検知に流用可能だが、本設計では `http` 内で 401 を即 `clearToken()` する方式を主とする。

### 6.3 ログイン UI `components/auth/LoginGate.tsx`（新規）と `App.tsx` 差し込み

`LoginGate` は `App.tsx` の `<AppProvider>` 直下に置き、**未認証なら子を描画せずログインフォームを出す**。`getAuthStatus()` で「そもそも認証が要るか」を判定し、不要（`required:false`）なら素通しで子を描画。

骨子（`createResource` + `createSignal`、`Input`/`Button`/`Card` を使用）:

```tsx
import { Show, createResource, createSignal, type ParentComponent } from "solid-js";
import { api } from "@/lib/api";
import { authToken, setToken } from "@/lib/auth";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const LoginGate: ParentComponent = (props) => {
  const [status] = createResource(() => api.getAuthStatus());
  const [input, setInput] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  // 認証不要、または有効トークン保持済みなら子を描画。
  const authed = () => status()?.required === false || !!authToken();

  const submit = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.login(input());     // 401 なら throw
      setToken(input());            // 検証通過後に保存（以降のヘッダに載る）
      setInput("");
    } catch {
      setError("トークンが正しくありません");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Show when={authed()} fallback={
      <div class="flex min-h-dvh items-center justify-center bg-background p-4">
        <Card class="w-full max-w-sm p-6">
          <h1 class="mb-1 text-lg font-semibold">サインイン</h1>
          <p class="mb-4 text-xs text-muted-foreground">アクセストークンを入力してください。</p>
          <form onSubmit={submit} class="space-y-3">
            <Input
              type="password"
              autocomplete="current-password"
              placeholder="AUTH_TOKEN"
              value={input()}
              onInput={(e) => setInput(e.currentTarget.value)}
            />
            <Show when={error()}>{(m) => <p class="text-xs text-destructive">{m()}</p>}</Show>
            <Button type="submit" class="w-full" disabled={busy() || !input()}>
              {busy() ? "確認中..." : "サインイン"}
            </Button>
          </form>
        </Card>
      </div>
    }>
      {props.children}
    </Show>
  );
};

export default LoginGate;
```

`App.tsx` への差し込み（`<AppProvider>` 直下を `LoginGate` で包む）:

```tsx
import LoginGate from "@/components/auth/LoginGate";
// ...
return (
  <AppProvider>
    <LoginGate>
      {/* 既存の二ペインシェル（div.relative ... ）をそのまま LoginGate の子に */}
      <div class="relative min-h-dvh bg-background text-foreground lg:grid ...">
        {/* ...既存のまま... */}
      </div>
    </LoginGate>
  </AppProvider>
);
```

> **設計判断**: ゲートを `App.tsx` 最外殻に1枚置くだけで全ルートを保護できる（`index.tsx` のルーティングは不変）。`status()` 取得中（resource pending）は `authed()` が `undefined→false` 評価でフォールバック表示になり得るが、`getAuthStatus` は軽量で一瞬。ちらつきが気になるなら `Show` の `when` を `status.loading ? <spinner/> : ...` に分岐してよい（任意）。
> `getAuthStatus` 自体は公開エンドポイントなのでトークン無しでも 200 を返す（ゲート判定が鶏卵にならない）。

### 6.4 Ark UI について

ログイン UI は `input`/`button`/`card` のみで自前 Tailwind で賄える。**Ark UI 部品は本機能では不要**。

---

## 7. API 契約

> すべて `/api` プレフィックス。`/api/auth/*` と `/api/health*` は **保護対象外**（トークン不要）。それ以外は `AUTH_TOKEN` 設定時に `Authorization: Bearer <token>` 必須。

### 7.1 `GET /api/auth/status` — ゲート要否（公開）
レスポンス（200）:
```json
{ "required": true }
```
`AUTH_TOKEN` 未設定なら `{ "required": false }`。**トークン値は返さない。**

### 7.2 `POST /api/auth/login` — トークン検証（公開）
リクエスト:
```json
{ "token": "s3cr3t-long-random-token" }
```
レスポンス（200、一致 or 認証無効）:
```json
{ "ok": true }
```
エラー:
- 401 `{ "error": "unauthorized" }`（トークン不一致。`WWW-Authenticate: Bearer` 付き）
- 400 `{ "error": "invalid input: token must not be empty" }`（空トークン）

### 7.3 保護対象ルート（既存全 API）
`AUTH_TOKEN` 設定時、`Authorization` ヘッダが無い/誤りなら:
```json
{ "error": "unauthorized" }
```
（HTTP 401、`WWW-Authenticate: Bearer` 付き）。ヘッダが正しければ **既存契約どおり**に応答（契約変更なし）。

例（正常アクセス）:
```
GET /api/feeds
Authorization: Bearer s3cr3t-long-random-token
→ 200 [ ... ]
```

---

## 8. 依存関係

- **ブロックする機能（本機能に依存する）**: 無し。本機能は横断ゲートであり、他機能の API 契約は変えない（ヘッダ要求が増えるだけ）。既存フロントは `http<T>()` 経由で API を叩くため、ヘッダ付与は一箇所の変更で全機能に波及する。
- **本機能が依存する機能（ハード依存）**: 無し。`shared/config`・`shared/state`・`features/mod.rs` の合成・`App.tsx` 最外殻という **既に存在する土台**の上に乗る。
- **ソフトな協調**:
  - 機能10（二ペインシェル `App.tsx`／`AppProvider`）: ゲートを最外殻へ差し込むため、`App.tsx` の構造（実装済み）を前提にする。
  - 機能04（`lib/theme.ts` / vitest）: `lib/auth.ts` を同型で書き、同じテスト基盤を使う。
  - 機能05（`/settings`）: 将来「トークン変更 UI」を `/settings` に置くなら同居（本 MVP では env 設定のみなので不要）。
- 既存スライス（feeds/articles/folders/stats/feed_overview/instapaper/search/health）への変更は無し。接触点は **`features/mod.rs` の合成変更**・**`shared/config.rs` への1フィールド**・**`shared/mod.rs` の `pub mod auth;`**・**`lib/api.ts` の `http` 改修**・**`App.tsx` の最外殻**のみ。

---

## 9. テスト計画（TDD）

> 方針: ロジックは純粋関数に寄せて Red→Green（MEMORY「バグ修正もテスト先行・書いたら必ず実行」）。本クレートは **binary crate（`lib.rs` 無し）**なので `backend/tests/` から内部を呼べない → 単体/結合テストは各モジュール内 `#[cfg(test)] mod tests` に置く。HTTP 表面は shell スクリプトで検証（既存前例）。

### 9.1 単体テスト（`#[cfg(test)] mod tests`、外部 I/O 不要）

`shared/auth.rs` 末尾（`constant_time_eq`）:

| テスト | 意図 |
|---|---|
| `cteq_equal_returns_true` | 同一バイト列で `true` |
| `cteq_differs_returns_false` | 1バイト違いで `false` |
| `cteq_different_length_returns_false` | 長さ違いで `false`（早期 return しても結果は false） |
| `cteq_empty_vs_empty_true` | 空 vs 空は `true`（運用上は `AuthToken::parse` で空は弾くが関数単体の性質を固定） |

`features/auth/domain.rs` 末尾（`AuthToken::parse`）:

| テスト | 意図 |
|---|---|
| `parse_rejects_empty` | 空文字を `Err` |
| `parse_rejects_whitespace_only` | 空白のみを `Err`（trim 後空） |
| `parse_trims_and_keeps_value` | 前後空白除去後の値を保持 |

### 9.2 ミドルウェア結合テスト（`#[cfg(test)] mod tests` in `shared/auth.rs`、DB 不要・`tower::ServiceExt::oneshot`）

`AppState` を最小構成で作る（`db` は `PgPool` 必須なので、**接続せずプールハンドルだけ作る** `PgPoolOptions::new().connect_lazy(...)` を使い、保護ルートは実行されず 401 で弾かれる経路だけ検証する。通過経路は health/login で確認）。雛形:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // oneshot

    fn test_state(token: Option<&str>) -> AppState {
        // connect_lazy は接続を張らない（保護ルートは叩かないので OK）。
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://invalid/invalid")
            .unwrap();
        let mut cfg = /* AppConfig の最小値を組む。auth_token のみ可変 */;
        cfg.auth_token = token.map(|s| s.to_string());
        AppState { db, config: std::sync::Arc::new(cfg), http: reqwest::Client::new() }
    }

    fn protected_app(state: AppState) -> Router {
        Router::new()
            .route("/api/x", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(state.clone(), require_auth))
            .with_state(state)
    }
}
```

| テスト | 期待 |
|---|---|
| `disabled_passes_without_header` | `auth_token=None` のとき `/api/x` ヘッダ無し → 200 |
| `enabled_rejects_missing_header` | `auth_token=Some` でヘッダ無し → 401 |
| `enabled_rejects_wrong_token` | `Authorization: Bearer wrong` → 401 |
| `enabled_accepts_correct_token` | `Authorization: Bearer <token>` → 200 |
| `enabled_rejects_non_bearer_scheme` | `Authorization: Basic ...` → 401 |

> `AppConfig` の最小構築が面倒なら、`AppConfig` に `#[cfg(test)]` のヘルパ `pub fn for_test(auth_token: Option<String>) -> Self` を足してよい（テスト専用・本番非影響）。connect_lazy が使えない場合は `protected` 経路を叩かず、health/login の oneshot だけで代替する。

### 9.3 HTTP スモークテスト（稼働スタックへ curl）

`scripts/test/api-auth.sh` を新設（`scripts/test/api-stats.sh` と同型、nginx 経由）。**`AUTH_TOKEN` を設定した状態のスタック**で実行する前提（スクリプト冒頭に注記）:

| 手順 / アサーション | 意図 |
|---|---|
| `GET /api/health` ヘッダ無し → 200 | health は保護対象外 |
| `GET /api/auth/status` ヘッダ無し → 200 かつ `required:true` | 公開・ゲート要否 |
| `GET /api/feeds` ヘッダ無し → 401 | 保護が効いている |
| `GET /api/feeds` `-H "Authorization: Bearer $AUTH_TOKEN"` → 200 | 正トークンで通過 |
| `GET /api/feeds` `-H "Authorization: Bearer wrong"` → 401 | 誤トークン拒否 |
| `POST /api/auth/login {"token":"wrong"}` → 401 | login 不一致 |
| `POST /api/auth/login {"token":"$AUTH_TOKEN"}` → 200 `ok:true` | login 一致 |
| （`AUTH_TOKEN` 未設定の別起動で）`GET /api/feeds` ヘッダ無し → 200 | 認証無効時の素通し |

### 9.4 フロント（vitest + 手動）
- vitest（feature 04 導入済み）: `lib/auth.ts` の `setToken`/`getToken`/`clearToken` が localStorage と signal を同期すること（`localStorage` は jsdom で利用可）。
- `tsc`（`just lint`）: `api.ts` / `LoginGate.tsx` / `auth.ts` の型整合。
- 手動: `AUTH_TOKEN` を設定して起動 → 未ログインでゲート表示 → 誤トークンでエラー → 正トークンでアプリ表示 → リロードで保持 → （トークンを変えてリロード or 401 誘発で）ゲート再表示。`AUTH_TOKEN` 未設定で起動 → ゲートなしで即利用可。

---

## 10. 実装手順（順序付きチェックリスト）

1. **設定追加**: `shared/config.rs` の `AppConfig` に `pub auth_token: Option<String>` を足し、`from_env()` で `std::env::var("AUTH_TOKEN").ok().filter(|v| !v.is_empty())` を読む（`anthropic_api_key` と同型）。`.env.example` に `# AUTH_TOKEN=` 行（コメント例＋「未設定なら認証無効」注記）を追加。
2. **ミドルウェア（Red 先行）**: `shared/auth.rs` を §5.1 で新規作成。`shared/mod.rs`（または `shared` のモジュール宣言箇所）に `pub mod auth;` を1行追加。`constant_time_eq` の `#[cfg(test)] mod tests`（§9.1）を先に書き、落ちる→実装で Green。
3. **auth スライス**: `features/auth/{domain,service,handler,mod}.rs` を §5.2〜5.5 で作成（`repository.rs` は作らない）。`domain.rs` の `AuthToken::parse` テスト（§9.1）を先に。未使用 import（`AppResult` 等）は残さない（`-D warnings`）。
4. **合成変更**: `features/mod.rs` を §5.5 のとおり「public（health + auth）/ protected（その他＋`require_auth`）」へ書き換え、`pub mod auth;` と `use crate::shared::auth::require_auth;` を追加。**既存スライスのファイルは触らない。**
5. **ミドルウェア結合テスト**: `shared/auth.rs` に §9.2 の `#[cfg(test)] mod tests` を追加（`tower::ServiceExt::oneshot`、`connect_lazy`、必要なら `AppConfig::for_test`）。`cargo test` で Green。
6. **ビルド & lint**: `just lint`（clippy `-D warnings` / tsc）。依存追加が不要なこと（axum/tower で足りる）を確認。
7. **HTTP スモーク**: `scripts/test/api-auth.sh` を §9.3 で作成・`chmod +x`。`AUTH_TOKEN` 設定スタックで実行し全アサート緑。`AUTH_TOKEN` 未設定起動での素通しも確認。
8. **フロント — auth.ts**: `frontend/src/lib/auth.ts`（§6.1）を作成。vitest（§9.4）を Red→Green。
9. **フロント — api.ts**: `http<T>()` にヘッダ付与＋401処理、型 `AuthStatus` と `getAuthStatus`/`login` を追加（§6.2）。`getToken`/`clearToken` を import。既存呼び出し側が壊れないことを `tsc` で確認。
10. **フロント — ゲート UI**: `components/auth/LoginGate.tsx`（§6.3）作成、`App.tsx` の `<AppProvider>` 直下に差し込み。`just lint` の tsc を通す。
11. **手動 E2E**: §9.4 の手動シナリオ（設定あり/なし両方）を確認。
12. **ドキュメント/運用メモ**: `.env.example` と README（任意）に「`AUTH_TOKEN` は十分長い乱数（例 `openssl rand -base64 32`）を使う」「未設定＝認証無効」を明記。
13. **コミット**: 設定・ミドルウェア・スライス・合成・フロント・スクリプトをまとめて。`.env` やトークンはコミットしない。

---

## 11. リスク・未決事項・代替案

| 項目 | リスク / 内容 | 対処・緩和 |
|---|---|---|
| **平文の共有トークン** | `AUTH_TOKEN` は env 平文。漏洩すれば全 API を取られる | 家庭内 LAN・単一ユーザーの MVP 判断。十分長い乱数を運用ガイドで強制（`openssl rand -base64 32`）。`/api/auth/status` は `required` のみ返しトークン値は一切返さない。将来は §4.2 のセッション化で per-device 失効 |
| **TLS 無しでのトークン送信** | LAN 内でも平文 HTTP だと `Authorization` ヘッダが盗聴され得る | 本機能のスコープ外。nginx での TLS 終端を別タスクで推奨（README に注記）。トークン方式自体は TLS 前提で安全 |
| **ブルートフォース** | `POST /api/auth/login` / 保護ルートへの総当たり | 高エントロピートークンで実質防御。レート制限は非スコープ。必要なら nginx `limit_req` or tower の rate-limit を将来追加 |
| **`error.rs` を編集しない制約と 401** | `AppError` に 401 が無い | ミドルウェア/ハンドラが生 `Response`（`unauthorized()`）を返す設計で回避（§5.6）。`error.rs` は不編集 |
| **CORS と Authorization ヘッダ** | `CorsLayer::permissive()` はプリフライトで `Authorization` を許可するか | `permissive()` は許可ヘッダを反映するため LAN・同一オリジン（nginx 同居）では問題なし。外部公開時に CORS を絞る際は `Authorization` を許可リストへ明示（別タスク） |
| **定数時間比較の自前実装** | 実装ミスで timing 漏れ/誤判定 | 純粋関数化し §9.1 で網羅。長さは秘密でない前提（トークン長は固定運用）。不安なら将来 `subtle` クレートへ差し替え（関数差し替えのみ） |
| **`getAuthStatus` 取得中のちらつき** | resource pending 中に一瞬ゲートが見える可能性 | `getAuthStatus` は軽量。気になれば `status.loading` 分岐でスピナー表示（§6.3 任意） |
| **ミドルウェア結合テストの AppState 構築** | `db: PgPool` 必須でテストが面倒 | `connect_lazy`（接続を張らない）＋ 保護ルートを叩かず 401 経路を検証。`AppConfig::for_test` ヘルパ（`#[cfg(test)]`）で最小構築 |
| **将来のセッション方式とマイグレーション番号** | per-device 失効に DB が要る | §4.2 の `0006_auth_sessions.sql` を暫定採番。**着手前に必ず `ls backend/migrations/` で最新番号を確認**（`set_ignore_missing` 未使用のため out-of-order は起動破壊） |
| **既存 `http<T>` 改修の波及** | ヘッダ合成順の変更で既存呼び出しが壊れないか | `init.headers` を最後に展開して上書き可能に保つ。`tsc` と既存 API スモークで回帰確認 |
</content>
</invoke>
