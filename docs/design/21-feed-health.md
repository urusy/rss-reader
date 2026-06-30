# 21 フィード健全性 / 死活検知

> 読み手向けメモ: このドキュメントは「リポジトリは手元にあるが、この会話の文脈を知らない別セッションの実装者」が、これだけ読めば着手・完了できる粒度で書いている。裏取りした実ファイル（このセッションで実際に開いて確認した）: `backend/src/features/feeds/{domain,repository,service,handler,mod}.rs`, `backend/src/features/feed_overview/{domain,repository,service,handler,mod}.rs`, `backend/src/features/mod.rs`, `backend/src/shared/{scheduler,config,state,error}.rs`, `backend/src/main.rs`, `backend/migrations/0001_init.sql`〜`0005_search.sql`, `frontend/src/lib/api.ts`, `frontend/src/routes/FeedManage.tsx`, `frontend/src/components/ui/badge.tsx`。

## 概要

各フィードの **取得（クロール）結果を記録**し、「**死んでいるフィード**（取得が連続失敗している）」「**古いフィード**（取得は成功しているが投稿が長期間途絶えている）」を検知して、フィード管理画面にバッジ表示する。購読の棚卸し（壊れた URL の修正・削除、放置フィードの整理）判断を助ける。

実装の中核は3点。

1. **`feeds` テーブルに健全性カラムを追記**（新マイグレーション）: `last_fetch_status` / `last_error` / `consecutive_failures` / `last_fetch_attempted_at`。これらは feeds アグリゲートの一部（同一テーブルの状態）。
2. **取得経路（fetch）の結果を記録**: 既存のフィード取得チョークポイント `feeds::service::fetch_and_store` の成否を、新スライス `feed_health` の記録関数（`record_success` / `record_failure`）で `feeds` 行へ書き戻す。定期取得（scheduler 経由）・手動再取得・フィード追加時の初回取得の **すべての取得経路**が同じ関数を通るため、1箇所のフックで全経路をカバーする。
3. **読み取り read model + 分類**: 新スライス `feed_health` が `GET /api/feeds/health` を提供。`feeds` の健全性カラムと `MAX(articles.published_at)`（最終投稿）を1クエリで読み、純粋関数 `classify()` で `healthy` / `stale` / `dead` の3値に分類して返す。`feed_overview`（機能03）と同型の読み取り専用スライス（CQRS-lite）。

> **AI 機能なし**: 本機能は LLM を一切使わない。したがって `shared/llm` の再利用・`ANTHROPIC_API_KEY` 判定・`AppError::NotEnabled` 経路は **発生しない**（テンプレート上の AI 規約は本機能には非該当）。DB キャッシュも要約結果ではなく取得結果の永続化であり、`articles.summary` 等とは無関係。

## スコープ / 非スコープ

**含む（このチケットでやる）**

- 新マイグレーション `0006_feed_health.sql`（暫定採番。**着手前に最新番号を確認**。§データモデル参照）。`feeds` に4カラム追記 + 部分インデックス1本。
- 新スライス `backend/src/features/feed_health/`（`domain.rs` / `repository.rs` / `service.rs` / `handler.rs` / `mod.rs`）。
  - `record_success` / `record_failure`: 取得結果を `feeds` 行へ書き戻す（書き込み）。
  - `list_health`: フィード別の健全性 read model を返す（読み取り）。
  - `classify()`: 純粋関数で3値分類 + その単体テスト。
- `GET /api/feeds/health` 1本。フィード別 `{ feed_id, last_fetch_status, last_error, consecutive_failures, last_fetch_attempted_at, last_fetched_at, last_published_at, health }` の配列（記事ゼロ・未取得のフィードも1行）。
- **取得経路への記録フック**: `feeds::service::fetch_and_store` を「内部関数 + 成否記録ラッパ」に最小改修（feeds は健全性カラムと同一アグリゲートのため、同一アグリゲートへの書き込み拡張として正当化。§バックエンド §「既存スライスへの最小フック」参照）。
- `features/mod.rs` に `.merge(feed_health::routes())` 1行 + `pub mod feed_health;` 1行。
- フロント: `lib/api.ts` に `FeedHealth` 型と `listFeedHealth()`。`components/ui/badge.tsx` に `stale` / `dead` バリアントを追記。`routes/FeedManage.tsx` の各フィード行に健全性バッジを差し込む（feature 01 の管理画面に同居）。
- 単体テスト（`classify` の `#[cfg(test)]`）+ 結合テスト（`scripts/test/api-feed-health.sh`、psql 決定論シードで実値 assert）。

**含まない（別チケット / 別機能）**

- 通知・アラート（メール / Push）。死活はバッジ表示まで。閾値超過時の能動通知は将来課題（§リスク）。
- per-feed リトライ/バックオフ・per-feed スケジュール。これは apalis 移行タスクの守備範囲（`shared/scheduler.rs` 差し替え。ロードマップ）。本機能は「結果の記録と分類」のみで、取得のスケジューリング自体は既存 `tokio::interval` のまま。
- 投稿頻度・未読数・経過日数の表示（機能03 `feed_overview` が担当）。本機能はそれと別エンドポイント `/api/feeds/health` を持ち、`last_published_at` のみ重複して返す（分類に必要なため）。
- フィードの自動無効化（dead を一定期間で購読停止する等）。手動削除は機能01の責務。
- `feeds` 以外のテーブル変更。`articles` は読み取りのみ。

## 既存実装の再利用

**車輪の再発明をしないため、以下を再利用する。** 以下はこのセッションで実ファイルを開いて確認済み。

- **`feed_overview` スライスが「読み取り read model + 純粋関数 + LEFT JOIN 集計」の前例**（`backend/src/features/feed_overview/`）。`repository::fetch_overview` が `feeds f LEFT JOIN articles a` を `query_as::<_, FeedOverviewRow>` で取得 → `service` が純粋関数（`posts_per_week`）で詰め替え → `handler` が `Json(Vec<...>)` 返却、`mod.rs` が `Router::new().route("/api/feeds/overview", get(handler::overview))`。**本スライスはこれをそっくり踏襲し、集計の代わりに健全性カラムを読み、`posts_per_week` の代わりに `classify` を使う。**
- **取得チョークポイント `feeds::service::fetch_and_store`**（`backend/src/features/feeds/service.rs`）。定期取得 `refresh_all_feeds`・手動 `refresh_one`・追加時 `create_feed` の **3経路すべて**がこの1関数を通る（grep で確認済み）。`AppResult<()>` を返すので、ここで成否を捕まえれば全経路の取得結果を1箇所で記録できる。
- **`feeds.id`（`UUID` PK）と `feeds` の既存カラム**（`0001_init.sql`）。健全性カラムは `ALTER TABLE feeds ADD COLUMN` で追記。`feeds/domain.rs::Feed` とその全クエリは **明示カラム列挙**（`SELECT id, url, title, folder_id, created_at, last_fetched_at`）で `SELECT *` を使わないため、**カラム追記で feeds スライスのクエリは壊れない**（確認済み）。
- **`idx_articles_published_at` / `idx_articles_feed_id`**（`0001_init.sql`）。`MAX(a.published_at)` と `LEFT JOIN a ON a.feed_id=f.id` に利用。
- **`features/mod.rs` の `.merge()` 合成規約**。現状 `router()` は `health/feeds/articles/stats/feed_overview/folders/instapaper/search` の8枚を `.merge()`。本スライスも1行追加で済む。
- **`shared/error.rs` の `AppError` + `AppResult`**。`sqlx::Error` は `#[from]` で `Database`(500) に自動変換。新バリアント不要。
- **`shared/scheduler.rs`** は変更不要。スケジューラは `feeds::service::refresh_all_feeds` を呼び続け、その内側の `fetch_and_store` が記録する（フックがチョークポイント側にあるため、スケジューラには手を入れない）。
- **フロント `lib/api.ts` の `http<T>()` ヘルパ**（204→undefined、非2xx は throw）と `listFeedOverview()` の GET パターン。同型の `listFeedHealth()` を1つ足すだけ。
- **`components/ui/badge.tsx`**（`cva` ベース、`default` / `unread` バリアント）。`stale` / `dead` を **追記**する（additive、既存利用箇所に影響なし）。
- **`routes/FeedManage.tsx`**（機能01。フィード行に既に `feed_overview` のバッジ/統計を表示）。本機能のバッジはこの既存行へ同居させる（新ルート不要）。
- **テスト実慣習**: 純粋ロジックは `#[cfg(test)] mod tests`（`feed_overview/domain.rs` / `feeds/domain.rs` が前例）。結合テストは `scripts/test/*.sh`（起動済みスタックへ HTTP、`api-feed-overview.sh` が前例。`backend/tests/` は不在・本 crate はバイナリ専用で library target が無いため `tests/` から内部 fn を呼べない）。

## データモデル

新マイグレーション **`backend/migrations/0006_feed_health.sql`**（暫定採番）。

> ⚠️ **採番注意（着手前に必ず確認）**: 現在の最新は `0005_search.sql`。本機能は次番として `0006` を仮置きするが、**並行して進む apalis 移行タスク**（`shared/scheduler.rs` 差し替え + ジョブテーブル）も新マイグレーションを足す可能性がある（README のマイグレーション番号レジスタ・⚠️ apalis 衝突注記を参照）。**着手直前に `backend/migrations/` の最新番号を見て、空き番号へ繰り下げること**（マイグレーションは追記のみ・既存ファイル不編集が鉄則）。番号がずれても本ファイル内の SQL 本文は不変。

```sql
-- 0006_feed_health.sql
-- Feed health / liveness tracking. Records the outcome of each crawl on the
-- feed row so the UI can flag dead (repeatedly failing) and stale (no recent
-- posts) feeds. Columns belong to the feeds aggregate; feeds queries use
-- explicit column lists (not SELECT *), so adding columns is non-breaking.

ALTER TABLE feeds ADD COLUMN IF NOT EXISTS last_fetch_status       TEXT;
ALTER TABLE feeds ADD COLUMN IF NOT EXISTS last_error              TEXT;
ALTER TABLE feeds ADD COLUMN IF NOT EXISTS consecutive_failures    INTEGER NOT NULL DEFAULT 0;
ALTER TABLE feeds ADD COLUMN IF NOT EXISTS last_fetch_attempted_at TIMESTAMPTZ;

-- Find ailing feeds quickly (badge query / future alerting). Partial index keeps
-- it tiny: only feeds currently failing are indexed.
CREATE INDEX IF NOT EXISTS idx_feeds_consecutive_failures
    ON feeds (consecutive_failures) WHERE consecutive_failures > 0;
```

カラムの意味:

| カラム | 型 | 意味 |
|--------|----|------|
| `last_fetch_status` | `TEXT` nullable | 直近の取得結果。`'ok'` / `'error'` / `NULL`（未取得）。 |
| `last_error` | `TEXT` nullable | 直近の失敗理由（成功時 `NULL`）。記録時に先頭1000文字へ切詰め。 |
| `consecutive_failures` | `INTEGER NOT NULL DEFAULT 0` | 連続失敗回数。成功で 0 リセット、失敗で +1。dead 判定の主指標。 |
| `last_fetch_attempted_at` | `TIMESTAMPTZ` nullable | 直近の取得「試行」時刻（成功・失敗とも更新）。既存 `last_fetched_at`（成功時のみ更新）と区別し「試みているが失敗し続けている」を見分ける。 |

> 既存 `feeds.last_fetched_at`（`0001_init.sql`、`touch_fetched` が成功時に `now()` をセット）は **そのまま流用**（最後に成功した取得時刻）。本機能は「試行時刻」を別カラム `last_fetch_attempted_at` で追加し、`last_fetched_at` の意味は変えない。

## バックエンド

新スライス `backend/src/features/feed_health/`。`feed_overview` と同型だが、**読み取り（`list_health`）に加えて、取得結果の書き込み（`record_success`/`record_failure`）も持つ**。書き込み先は `feeds` テーブル（健全性カラム = feeds アグリゲート）。新 trait / dyn は追加しない。

### ルート設計と衝突回避

`/api/feeds/health` は静的セグメント。`feeds` スライスは `/api/feeds`(GET/POST)・`/api/feeds/{id}`(DELETE/PATCH)・`/api/feeds/{id}/refresh`(POST) を持ち、`feed_overview` が `/api/feeds/overview`(GET) を持つ（実コードで確認済み）。`GET /api/feeds/{id}` は **存在しない**ため method+path が重複せず `.merge()` で衝突しない。axum 0.8（matchit 0.8）は静的セグメントを動的 `{id}` より優先するため、将来 `GET /api/feeds/{id}` が足されても `health`/`overview` が先にマッチする。

### `domain.rs`

```rust
use serde::Serialize;
use uuid::Uuid;

/// dead 判定の閾値。連続失敗がこの回数以上なら「死んでいる」とみなす。
pub const DEAD_FAILURE_THRESHOLD: i32 = 3;
/// stale 判定の閾値（日）。最終投稿がこれより古ければ「古い（投稿が途絶えた）」。
pub const STALE_DAYS: i64 = 30;

/// フィードの健全性3値。serde で "healthy"/"stale"/"dead" にシリアライズされ、
/// フロントの `health: "healthy" | "stale" | "dead"` と一致する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthState {
    /// 取得が安定し、最近の投稿もある。
    Healthy,
    /// 取得は失敗しきっていないが、投稿が長期間途絶えている / 一度も投稿日時を観測できていない。
    Stale,
    /// 取得が連続失敗している（URL 切れ・サーバ停止など）。
    Dead,
}

/// リポジトリが返す「素の健全性行」。分類前。feed_overview と同じく read model の
/// 相関キーは feeds の FeedId newtype を import せず素の Uuid を使う（スライス間型結合を作らない）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FeedHealthRow {
    pub feed_id: Uuid,
    pub last_fetch_status: Option<String>,
    pub last_error: Option<String>,
    pub consecutive_failures: i32,
    pub last_fetch_attempted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_published_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// API レスポンス1行（読み取り専用 read model）。素の行に `health` を足したもの。
#[derive(Debug, Clone, Serialize)]
pub struct FeedHealth {
    pub feed_id: Uuid,
    pub last_fetch_status: Option<String>,
    pub last_error: Option<String>,
    pub consecutive_failures: i32,
    pub last_fetch_attempted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub health: HealthState,
}

/// 健全性の純粋関数。now を引数注入し、now() 依存を排して決定論テスト可能にする
/// （feed_overview が経過日数を backend で計算せずフロントに委ねたのと同方針だが、
///  分類は3値の単純判定なので backend 側で行い、フロントは色分けに使うだけにする）。
///
/// 判定順（dead が最優先）:
///   1. consecutive_failures >= DEAD_FAILURE_THRESHOLD          -> Dead
///   2. last_published_at が None（投稿日時を一度も観測できない） -> Stale
///   3. 最終投稿が STALE_DAYS より古い                          -> Stale
///   4. それ以外                                                -> Healthy
pub fn classify(
    consecutive_failures: i32,
    last_published_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> HealthState {
    if consecutive_failures >= DEAD_FAILURE_THRESHOLD {
        return HealthState::Dead;
    }
    match last_published_at {
        None => HealthState::Stale,
        Some(ts) => {
            if now - ts > chrono::Duration::days(STALE_DAYS) {
                HealthState::Stale
            } else {
                HealthState::Healthy
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn ago(days: i64) -> chrono::DateTime<chrono::Utc> {
        Utc::now() - Duration::days(days)
    }

    #[test]
    fn healthy_when_recent_post_and_no_failures() {
        assert_eq!(classify(0, Some(ago(3)), Utc::now()), HealthState::Healthy);
    }

    #[test]
    fn stale_when_last_post_older_than_threshold() {
        // 40日前 > 30日 -> Stale
        assert_eq!(classify(0, Some(ago(40)), Utc::now()), HealthState::Stale);
    }

    #[test]
    fn stale_when_never_published() {
        // 投稿日時を一度も観測できていない（フィードが日時を出さない / 記事ゼロ）-> Stale
        assert_eq!(classify(0, None, Utc::now()), HealthState::Stale);
    }

    #[test]
    fn dead_when_failures_at_threshold() {
        // ちょうど閾値（3）で Dead（境界）
        assert_eq!(classify(3, Some(ago(1)), Utc::now()), HealthState::Dead);
    }

    #[test]
    fn dead_when_failures_above_threshold() {
        assert_eq!(classify(10, None, Utc::now()), HealthState::Dead);
    }

    #[test]
    fn not_dead_below_threshold() {
        // 2回失敗 + 最近の投稿あり -> まだ Healthy（dead ではない）
        assert_eq!(classify(2, Some(ago(1)), Utc::now()), HealthState::Healthy);
    }

    #[test]
    fn dead_takes_precedence_over_stale() {
        // 連続失敗が閾値超 -> 投稿が新しくても Dead が優先
        assert_eq!(classify(5, Some(ago(1)), Utc::now()), HealthState::Dead);
    }

    #[test]
    fn just_under_stale_boundary_is_healthy() {
        // 29日前 <= 30日 -> Healthy（境界の内側）
        assert_eq!(classify(0, Some(ago(29)), Utc::now()), HealthState::Healthy);
    }
}
```

> clippy 注意: `now - ts`（`DateTime` 同士の減算 → `chrono::Duration`）と `Duration::days` の比較のみで、cast やキャストによる精度損失は無い。`just lint`（`cargo clippy --all-targets -- -D warnings`、pedantic 無効）で警告は出ない。

### `repository.rs`

```rust
use sqlx::PgPool;
use uuid::Uuid;

use super::domain::FeedHealthRow;
use crate::shared::error::AppResult;

/// 取得成功を feeds 行へ書き戻す。連続失敗を 0 リセットし、エラーをクリアする。
/// last_fetched_at（成功時刻）は feeds スライスの touch_fetched が別途 now() にする。
pub async fn record_success(pool: &PgPool, feed_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE feeds
           SET last_fetch_status       = 'ok',
               last_error              = NULL,
               consecutive_failures    = 0,
               last_fetch_attempted_at = now()
           WHERE id = $1"#,
    )
    .bind(feed_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 取得失敗を feeds 行へ書き戻す。連続失敗を +1 し、理由を記録する（1000字に切詰め）。
pub async fn record_failure(pool: &PgPool, feed_id: Uuid, error: &str) -> AppResult<()> {
    // last_error は表示・診断用。巨大な upstream 本文が紛れても DB を汚さないよう切詰め。
    let truncated: String = error.chars().take(1000).collect();
    sqlx::query(
        r#"UPDATE feeds
           SET last_fetch_status       = 'error',
               last_error              = $2,
               consecutive_failures    = consecutive_failures + 1,
               last_fetch_attempted_at = now()
           WHERE id = $1"#,
    )
    .bind(feed_id)
    .bind(truncated)
    .execute(pool)
    .await?;
    Ok(())
}

/// フィード別の健全性行を1クエリで返す。LEFT JOIN なので記事ゼロのフィードも1行返り、
/// MAX(published_at)=NULL になる。並びは feeds の list_all / feed_overview と同じ created_at DESC。
pub async fn list_health(pool: &PgPool) -> AppResult<Vec<FeedHealthRow>> {
    let rows = sqlx::query_as::<_, FeedHealthRow>(
        r#"SELECT
             f.id                       AS feed_id,
             f.last_fetch_status        AS last_fetch_status,
             f.last_error               AS last_error,
             f.consecutive_failures     AS consecutive_failures,
             f.last_fetch_attempted_at  AS last_fetch_attempted_at,
             f.last_fetched_at          AS last_fetched_at,
             MAX(a.published_at)        AS last_published_at
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

設計ノート:
- **`query` / `query_as`（runtime クエリ）のみ。`query!` マクロ禁止**（ビルド時 DB 接続を要求するため。CLAUDE.md）。
- `GROUP BY f.id`（PK）に対し非集計列 `f.last_fetch_status` 等を SELECT・`ORDER BY f.created_at` できるのは PostgreSQL の関数従属性による（PK でグループ化時に同テーブルの列を参照可能）。`feed_overview` が同じパターンを採用済み。
- `record_*` は対象行が無ければ0行更新で no-op（`?` でエラーにしない）。取得経路では feed 行が必ず存在するため通常は1行更新。
- `consecutive_failures` は `i32`（SQL `INTEGER`）。`FromRow` で `i32` に直接マップ。

### `service.rs`

```rust
use super::domain::{classify, FeedHealth};
use super::repository;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

/// 健全性行を読み、now を基準に classify して read model に詰め替える。
pub async fn list_health(state: &AppState) -> AppResult<Vec<FeedHealth>> {
    let now = chrono::Utc::now();
    let rows = repository::list_health(&state.db).await?;
    Ok(rows
        .into_iter()
        .map(|r| FeedHealth {
            health: classify(r.consecutive_failures, r.last_published_at, now),
            feed_id: r.feed_id,
            last_fetch_status: r.last_fetch_status,
            last_error: r.last_error,
            consecutive_failures: r.consecutive_failures,
            last_fetch_attempted_at: r.last_fetch_attempted_at,
            last_fetched_at: r.last_fetched_at,
            last_published_at: r.last_published_at,
        })
        .collect())
}
```

### `handler.rs`

```rust
use axum::extract::State;
use axum::Json;

use super::domain::FeedHealth;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn health(State(state): State<AppState>) -> AppResult<Json<Vec<FeedHealth>>> {
    Ok(Json(service::list_health(&state).await?))
}
```

### `mod.rs`

```rust
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::get;
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/feeds/health", get(handler::health))
}
```

### `features/mod.rs` への合成（追加2行のみ）

```rust
pub mod feed_health;   // ← 追加（feed_overview の隣）

// ...router() 内、.merge(feed_overview::routes()) の隣に...
        .merge(feed_health::routes()) // ← 追加
```

### 既存スライスへの最小フック（feeds = 同一アグリゲート）

健全性カラムは **`feeds` テーブルの状態**であり、取得の成否を記録する以上、取得チョークポイント `feeds::service::fetch_and_store` に書き込みフックを置くのが唯一の正攻法。これはアーキ規約の「**既存スライス拡張は同一アグリゲートへの書き込みに限り正当化**」（README）に合致する同一アグリゲート（feeds）への書き込みであり、Vertical Slice の越境共通レイヤー禁止には抵触しない。フックは「内部関数 + 成否記録ラッパ」の最小改修に閉じ、**取得ロジック本体は1行も変えない**。

`backend/src/features/feeds/service.rs` の `fetch_and_store` を次のように分割する（本体を `fetch_and_store_inner` へ改名移設し、公開シグネチャ `pub async fn fetch_and_store(...) -> AppResult<()>` は不変に保つ）:

```rust
use crate::features::feed_health; // ファイル冒頭の use に追加

/// 取得チョークポイント。成否を feed_health に記録してから結果をそのまま返す。
/// scheduler 経由の定期取得・手動 refresh・追加時の初回取得の全経路がここを通る。
pub async fn fetch_and_store(state: &AppState, feed: &Feed) -> AppResult<()> {
    let result = fetch_and_store_inner(state, feed).await;
    match &result {
        Ok(()) => {
            if let Err(e) = feed_health::repository::record_success(&state.db, feed.id.0).await {
                tracing::warn!(error = %e, feed = %feed.url, "record_success failed");
            }
        }
        Err(e) => {
            if let Err(re) =
                feed_health::repository::record_failure(&state.db, feed.id.0, &e.to_string()).await
            {
                tracing::warn!(error = %re, feed = %feed.url, "record_failure failed");
            }
        }
    }
    result
}

/// 旧 fetch_and_store の本体（HTTP 取得 → parse → upsert → touch_fetched）をそのまま移設。
/// 中身は一切変更しない。
async fn fetch_and_store_inner(state: &AppState, feed: &Feed) -> AppResult<()> {
    // ……既存の fetch_and_store の本体をそのまま貼る（reqwest get → feed_rs parse →
    //    articles::repository::upsert ループ → repository::touch_fetched）……
}
```

ポイント:
- 記録の失敗は取得結果に影響させない（`warn!` ログのみ）。記録は補助情報であり、取得そのものの成否は `result` をそのまま伝播する。
- フックがチョークポイント側にあるため **`shared/scheduler.rs` は変更不要**。スケジューラは従来どおり `feeds::service::refresh_all_feeds` を呼び、その内側で各 `fetch_and_store` が記録する。`refresh_all_feeds` も `refresh_one` も `create_feed` も無改修のまま記録が効く（dead code も発生しない）。
- 依存方向は feeds → feed_health の一方向（feeds が feed_health::repository を呼ぶ）。feed_health は feeds のテーブルを読むが Rust の型としては依存しない（read model は素の `Uuid`）。循環は生じない。

### AppError の使い分け

- `GET /api/feeds/health` は一覧取得につき **`NotFound` を返さない**（0件は空配列で 200）。
- DB エラーは `sqlx::Error` → `AppError::Database`(500) に `#[from]` で自動変換、`?` 伝播。
- `Validation` / `NotEnabled` / `Upstream` は本機能では発生しない（AI 無し・入力無し）。新バリアント追加なし（`shared/error.rs` 不編集）。

## フロントエンド

> 方針: 本機能の UI は「フィード行に健全性バッジ（dead/stale のみ表示、healthy は無表示）を差し込む」だけ。a11y 部品は不要。表示ホストは機能01の `routes/FeedManage.tsx`（既にフィード行で `feed_overview` のバッジ/統計を出している）に同居させる。

### `lib/api.ts`（型 + メソッド追加）

```ts
export interface FeedHealth {
  feed_id: string;
  last_fetch_status: "ok" | "error" | null;
  last_error: string | null;
  consecutive_failures: number;
  last_fetch_attempted_at: string | null;
  last_fetched_at: string | null;
  last_published_at: string | null;
  health: "healthy" | "stale" | "dead";
}

// api オブジェクト内、listFeedOverview の隣に追加（同型 GET）
listFeedHealth: () => http<FeedHealth[]>("/api/feeds/health"),
```

- 命名は既存規約「動詞 + リソース camelCase」（`listFeeds` / `listFeedOverview`）に揃え `listFeedHealth`。
- `feed_id` 相関キーで `listFeeds()` / 既存 overview と id 突合する（title/url は重複させない）。

### `components/ui/badge.tsx`（バリアント追記）

既存 `cva` の `variants.variant` に `stale` / `dead` を**追記**し、`Badge` の props 型 union も広げる（additive、既存 `default`/`unread` は不変）。

```tsx
// badge cva の variant に追加
        stale: "bg-muted text-muted-foreground ring-1 ring-border", // 古い：控えめ
        dead: "bg-destructive text-destructive-foreground",         // 死亡：強調

// Badge props の variant union も拡張
  variant?: "default" | "unread" | "stale" | "dead";
```

> `bg-destructive` / `text-destructive-foreground` は shadcn 由来 oklch トークン（`app.css`）。実装時に `frontend/src/app.css` に `--destructive` / `--destructive-foreground` が定義済みか確認すること（shadcn の標準セットには含まれる）。未定義なら `stale` と同じ控えめ表現にフォールバックするか、`app.css` にトークンを追記する（生 hex は持ち込まない）。

### `routes/FeedManage.tsx`（健全性バッジを行へ差し込み）

既存 `FeedManage` は `overview` を `createResource(() => api.listFeedOverview())` で取り、`overviewById` の `Map` で id 突合して各行に未読バッジ・統計を出している（確認済み）。これと**同じパターン**で health を足す。

1. health リソースと id マップを追加:

```tsx
import { type FeedHealth } from "@/lib/api"; // 既存 import に型を追加

// コンポーネント内、overview の隣に
const [health, { refetch: refetchHealth }] = createResource(() =>
  api.listFeedHealth(),
);
const healthById = createMemo(
  () =>
    new Map<string, FeedHealth>((health() ?? []).map((h) => [h.feed_id, h])),
);

// 再取得（再取得ボタン押下後など）に health も混ぜる
const refetchAll = async () => {
  app.refetchFeeds();
  app.refetchFolders();
  await Promise.all([refetchOverview(), refetchHealth()]);
};
```

2. フィード行（タイトル + 未読バッジの並び）に health バッジを差し込む。`healthy` は無表示。

```tsx
{/* 既存の <Show when={(o()?.unread_count ?? 0) > 0}> 未読バッジの隣に */}
{(() => {
  const h = healthById().get(feed.id);
  if (!h || h.health === "healthy") return null;
  return h.health === "dead" ? (
    <Badge variant="dead" title={h.last_error ?? "取得に連続失敗しています"}>
      取得失敗 {h.consecutive_failures}回
    </Badge>
  ) : (
    <Badge variant="stale" title="投稿が長期間途絶えています">
      更新停滞
    </Badge>
  );
})()}
```

- `title` 属性に `last_error` を出すことで、ホバーで失敗理由を確認できる（行を狭く保つ）。
- `Badge` は `title` を受けられるよう props に `title?: string` を足す（additive）か、`<span title=...>` でラップする。実装時に `badge.tsx` の props を1つ広げるのが簡潔。

### 状態管理・トークン

- 新しいグローバル状態は不要。`FeedManage` ローカルの `createResource` に閉じる（overview と同じ）。`store.tsx` は変更しない。
- 装飾は意味トークンのみ（`bg-muted` / `bg-destructive` / `text-muted-foreground` / `ring-border`）。新色・生 hex は持ち込まない（oklch トークン維持）。
- サイドバー（`Sidebar.tsx`）への dead マーカー表示は将来の拡張候補（本チケットでは管理画面のみ。§非スコープ）。

## API 契約

### `GET /api/feeds/health`

- リクエスト: クエリ・ボディなし。認証/有効化フラグなし（常に有効）。
- レスポンス `200 OK`: `FeedHealth` の配列（フィード作成日時の降順、記事ゼロ・未取得フィードも含む）。

```json
[
  {
    "feed_id": "7b1c0d2e-2a3b-4c5d-8e9f-0a1b2c3d4e5f",
    "last_fetch_status": "error",
    "last_error": "upstream request failed: error sending request for url (https://example.test/dead.xml)",
    "consecutive_failures": 5,
    "last_fetch_attempted_at": "2026-06-30T09:00:00Z",
    "last_fetched_at": "2026-06-20T09:00:00Z",
    "last_published_at": "2026-06-19T22:14:00Z",
    "health": "dead"
  },
  {
    "feed_id": "9f8e7d6c-5b4a-3c2d-1e0f-aabbccddeeff",
    "last_fetch_status": "ok",
    "last_error": null,
    "consecutive_failures": 0,
    "last_fetch_attempted_at": "2026-06-30T09:00:00Z",
    "last_fetched_at": "2026-06-30T09:00:00Z",
    "last_published_at": "2026-04-01T10:00:00Z",
    "health": "stale"
  },
  {
    "feed_id": "11112222-3333-4444-5555-666677778888",
    "last_fetch_status": "ok",
    "last_error": null,
    "consecutive_failures": 0,
    "last_fetch_attempted_at": "2026-06-30T09:00:00Z",
    "last_fetched_at": "2026-06-30T09:00:00Z",
    "last_published_at": "2026-06-29T08:00:00Z",
    "health": "healthy"
  }
]
```

フィールド意味:
- `feed_id`: `feeds.id`（UUID 文字列）。フロントは `listFeeds()` と id 突合。
- `last_fetch_status`: `'ok'` / `'error'` / `null`（未取得）。
- `last_error`: 直近失敗理由（成功時 `null`、1000字切詰め）。
- `consecutive_failures`: 連続失敗回数（`i32`、成功で 0）。`>= 3` で `health="dead"`。
- `last_fetch_attempted_at`: 直近の取得試行時刻（成功・失敗とも）。
- `last_fetched_at`: 最後に成功した取得時刻（既存カラム流用）。
- `last_published_at`: `MAX(articles.published_at)`。記事ゼロ or 全件 NULL のとき `null`。
- `health`: `"healthy"` / `"stale"` / `"dead"`。`classify()` の結果。dead 最優先 → 失敗少 & 投稿停滞/未観測なら stale → それ以外 healthy。
- エラー: DB 障害時 `500 {"error":"internal error"}`（`AppError::Database`）。それ以外の異常系は無し。

## 依存関係

- **依存する機能（ハード依存）: なし。`dependsOn` は実質空。** バックエンドは既存 `feeds`/`articles` + 新カラムのみで完結。
- **ソフト依存（表示ホスト）: 機能01（feed-management / `FeedManage.tsx`）**。本機能のバッジは機能01の管理画面のフィード行に同居する。機能01は既に main にマージ済み（`routes/FeedManage.tsx` 実在）なので、ホストは存在する。万一 `FeedManage` が無い段階で着手する場合は、機能03 の `FeedStatsList` 同様の自己完結コンポーネントを1枚作って暫定マウントできる（feeds + health を自前取得して id 突合）。
- **関係するが本機能では触れない**:
  - 機能03（feed_overview）: 別エンドポイント。`last_published_at` のみ重複して返すが、未読数/投稿頻度は overview の責務。UI 上は同じ行に両者のバッジが並ぶ。
  - apalis 移行（ロードマップ）: 取得スケジューリングの差し替え。`fetch_and_store` を通る限り記録フックは効き続ける（apalis に移っても `fetch_and_store` を呼ぶ設計なら無改修）。マイグレーション番号の衝突のみ注意（§データモデル）。

## テスト計画（TDD）

**Red → 理解 → Green の順。書いたら必ず実行する。**

> 配置方針: 純粋ロジック（`classify`）は `#[cfg(test)] mod tests`（DB 不要）。記録ロジック + 分類の結合検証は `scripts/test/api-feed-health.sh`（起動済みスタックへ HTTP、psql 決定論シードで実値 assert）。本 crate はバイナリ専用で library target が無く `backend/tests/` から内部 fn を呼べないため、`feed_overview` と同じ2方式に従う。

### 単体テスト（`#[cfg(test)] mod tests`、§domain.rs に同梱・DB 不要）

`backend/src/features/feed_health/domain.rs` に `classify` のテストを**先に**書く（Red）。

| テスト | 意図 |
|--------|------|
| `healthy_when_recent_post_and_no_failures` | 失敗0 + 最近投稿 → Healthy（基本ケース） |
| `stale_when_last_post_older_than_threshold` | 失敗0 + 最終投稿40日前 → Stale（古いフィード検知の核） |
| `stale_when_never_published` | 失敗0 + `last_published_at=None` → Stale（日時未観測の扱い） |
| `dead_when_failures_at_threshold` | 失敗=3（閾値ちょうど）→ Dead（境界） |
| `dead_when_failures_above_threshold` | 失敗10 → Dead |
| `not_dead_below_threshold` | 失敗2 + 最近投稿 → Healthy（dead 誤判定しない） |
| `dead_takes_precedence_over_stale` | 失敗5 + 最近投稿 → Dead（判定順の優先） |
| `just_under_stale_boundary_is_healthy` | 最終投稿29日前 → Healthy（stale 境界の内側） |

実行: `cd backend && cargo test feed_health`（DB 不要）。`just lint`（clippy `-D warnings` + `pnpm typecheck`）も通す。

### 結合テスト（`scripts/test/api-feed-health.sh`、新規・実値を assert）

`api-feed-overview.sh` を雛形に、**psql で健全性カラムを決定論シード**してから `GET /api/feeds/health` を叩き、`health` 分類と各フィールドの**実値**を assert する。内部 DB へは `docker compose exec -T db psql` で到達（compose の DB はホスト非公開）。

シード（決定論）:
- **feed DEAD**（`id=…00dd`）: `consecutive_failures=5, last_fetch_status='error', last_error='boom'`、記事1本 `published_at=now()-interval '1 day'`。期待 `health="dead"`（失敗が最近投稿より優先）, `consecutive_failures==5`, `last_fetch_status=="error"`。
- **feed STALE**（`id=…0055`）: `consecutive_failures=0, last_fetch_status='ok'`、記事1本 `published_at=now()-interval '40 days'`。期待 `health="stale"`, `consecutive_failures==0`, `last_published_at!=null`。
- **feed HEALTHY**（`id=…00hh`→ UUID は `0000…00ab`）: `consecutive_failures=0, last_fetch_status='ok'`、記事1本 `published_at=now()-interval '2 days'`。期待 `health="healthy"`。
- **feed STALE-NEVER**（`id=…00ee`）: `consecutive_failures=0, last_fetch_status='ok'`、記事ゼロ（`last_published_at=null`）。期待 `health="stale"`（未観測）。

```bash
#!/usr/bin/env bash
# Integration test for feed_health: seeds health columns via psql, then asserts
# the COMPUTED health classification of GET /api/feeds/health.
# Requires: running stack (nginx :8081), docker compose, jq.
set -uo pipefail

URL="${URL:-http://localhost:8081/api/feeds/health}"
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rssreader}"
PGDB="${POSTGRES_DB:-rssreader}"
DEAD="00000000-0000-0000-0000-0000000000dd"
STALE="00000000-0000-0000-0000-000000000055"
HEALTHY="00000000-0000-0000-0000-0000000000ab"
NEVER="00000000-0000-0000-0000-0000000000ee"

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() { psql -q -c "DELETE FROM feeds WHERE id IN ('$DEAD','$STALE','$HEALTHY','$NEVER');" >/dev/null 2>&1 || true; }
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

# --- seed (idempotent) ---
psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id IN ('$DEAD','$STALE','$HEALTHY','$NEVER');
INSERT INTO feeds (id, url, title, last_fetch_status, last_error, consecutive_failures, last_fetch_attempted_at) VALUES
  ('$DEAD',    'https://example.test/dead.xml',    'dead',    'error', 'boom', 5, now()),
  ('$STALE',   'https://example.test/stale.xml',   'stale',   'ok',    NULL,   0, now()),
  ('$HEALTHY', 'https://example.test/healthy.xml', 'healthy', 'ok',    NULL,   0, now()),
  ('$NEVER',   'https://example.test/never.xml',   'never',   'ok',    NULL,   0, now());
INSERT INTO articles (id, feed_id, url, title, published_at, is_read) VALUES
  (gen_random_uuid(), '$DEAD',    'https://example.test/d1', 'd1', now() - interval '1 day',  false),
  (gen_random_uuid(), '$STALE',   'https://example.test/s1', 's1', now() - interval '40 days', false),
  (gen_random_uuid(), '$HEALTHY', 'https://example.test/h1', 'h1', now() - interval '2 days',  false);
-- feed NEVER intentionally has zero articles (last_published_at = null).
SQL

# --- fetch ---
body="$(curl -s -m 5 -w '\n%{http_code}' "$URL")"
code="${body##*$'\n'}"; json="${body%$'\n'*}"
[ "$code" = "200" ] || fail "expected 200, got $code ($json)"
case "$json" in "["*) : ;; *) fail "not a JSON array: $json";; esac

assert_health() { # $1=id $2=expected_health
  echo "$json" | jq -e --arg id "$1" --arg h "$2" '
    (map(select(.feed_id==$id)) | first) as $r
    | $r != null and $r.health == $h
  ' >/dev/null || fail "feed $1 expected health=$2, got $(echo "$json" | jq -c --arg id "$1" 'map(select(.feed_id==$id)) | first')"
}

assert_health "$DEAD" "dead"
assert_health "$STALE" "stale"
assert_health "$HEALTHY" "healthy"
assert_health "$NEVER" "stale"

# --- spot-check raw fields on the dead feed ---
echo "$json" | jq -e --arg id "$DEAD" '
  (map(select(.feed_id==$id)) | first) as $r
  | $r.consecutive_failures == 5
    and $r.last_fetch_status == "error"
    and $r.last_error == "boom"
' >/dev/null || fail "dead feed raw fields wrong: $(echo "$json" | jq -c --arg id "$DEAD" 'map(select(.feed_id==$id)) | first')"

echo "PASS: /api/feeds/health classification (dead/stale/healthy/stale-never) + raw fields"
```

- **Red**: 実装前は `/api/feeds/health` が 404 → 「expected 200」で落ちる。実装後 Green。
- 環境変数 `URL` / `DB_SVC` / `POSTGRES_USER` / `POSTGRES_DB` で上書き可能。`jq` 必須。

### 記録ロジックの検証（手動 or 任意のスクリプト追加）

`record_success`/`record_failure` は取得経路でのみ発火する。最小確認:
1. 到達不能 URL のフィードを追加（`POST /api/feeds` に `https://example.invalid/feed.xml` 等）。`create_feed` の初回取得が DNS/接続失敗 → `record_failure` 発火。
2. `GET /api/feeds/health` で当該フィードの `consecutive_failures >= 1`・`last_fetch_status=="error"`・`last_error != null` を確認。
3. 正常フィードを `POST /api/feeds/{id}/refresh` → `consecutive_failures==0`・`last_fetch_status=="ok"` を確認。

ネットワーク依存のため必須スクリプト化はしない（CI 不安定要因）。手動確認 + 上記シードベースの分類テストで品質を担保する。

### フロント（手動 / 型）

- `tsc` 型チェック（`just lint` の `pnpm typecheck`）で `FeedHealth` 型・`listFeedHealth()`・`badge.tsx` バリアント・`FeedManage` の整合を確認。
- 手動: dead / stale / healthy の各フィードで、管理画面の行に「取得失敗 N回（赤）」「更新停滞（控えめ）」が出る／healthy は無表示、`title` ホバーで `last_error` が見えることを目視。

## 実装手順

1. ブランチを切る（例 `feat/feed-health`）。`main` 直コミットしない。
2. **マイグレーション番号を確認**: `backend/migrations/` の最新を見る（現状 `0005`）。空き番号で `0006_feed_health.sql` を作成（§データモデルの SQL）。apalis 等が先に番号を取っていたら繰り下げる。
3. 新スライス `backend/src/features/feed_health/` を作成し5ファイルを置く:
   - `domain.rs`（§domain。**まず `#[cfg(test)] mod tests` を書いて Red**、`HealthState` / `FeedHealthRow` / `FeedHealth` / `classify` / 定数）。
   - `repository.rs`（§repository。`record_success` / `record_failure` / `list_health`）。
   - `service.rs`（§service。`list_health`）。
   - `handler.rs`（§handler）。
   - `mod.rs`（§mod、`routes()`）。
4. `cd backend && cargo test feed_health` で単体テストを Green にする。
5. `backend/src/features/mod.rs` に `pub mod feed_health;` と `.merge(feed_health::routes())` を1行ずつ追加。
6. **feeds への記録フック**: `feeds/service.rs` の `fetch_and_store` を `fetch_and_store_inner`（本体そのまま）+ 記録ラッパに分割し、`use crate::features::feed_health;` を追加（§「既存スライスへの最小フック」）。公開シグネチャは不変。取得ロジック本体は変更しない。
7. `cargo build` → `just lint`（`clippy -D warnings` + `pnpm typecheck`）→ `cargo fmt`。
8. スタックを起動（`just up`、または `just dev-db` + `just back`）。マイグレーション 0006 が適用されることを確認（`feeds` に新カラム）。
9. `scripts/test/api-feed-health.sh` を追加、実行 → dead/stale/healthy/stale-never の分類と dead の生フィールドを Green で確認。手で `curl http://localhost:8081/api/feeds/health | jq` も見る。記録ロジックは §記録ロジックの検証 を手動確認。
10. フロント: `lib/api.ts` に `FeedHealth` 型 + `listFeedHealth()` を追加。`components/ui/badge.tsx` に `stale`/`dead` バリアント（+ 必要なら `title` prop）を追加。`routes/FeedManage.tsx` に health リソース + id マップ + 行バッジを追加。
11. `just lint`（tsc）を通し、dead/stale/healthy の各フィードで管理画面の表示を目視確認。
12. ユーザーが望むタイミングでコミット（メッセージ末尾に `Co-Authored-By` 行）。新規マイグレーションが 0006（または確定番号）1枚で、既存マイグレーションを編集していないことを最終確認。

## リスク

| 項目 | リスク | 対処 |
|------|--------|------|
| **マイグレーション番号衝突** | apalis 移行が同じ 0006 を取りうる（README ⚠️） | 着手直前に `backend/migrations/` の最新番号を確認し、空き番号へ繰り下げる。SQL 本文は不変 |
| **dead 閾値（採用＝連続3回）** | 一時的なネットワーク不調で誤って dead 表示 | 「連続」失敗なので1回成功すれば 0 リセット。閾値はドメイン定数 `DEAD_FAILURE_THRESHOLD` 1箇所で調整可。実データで調整 |
| **stale 閾値（採用＝30日）** | 月刊フィードや不定期フィードを誤って stale 表示 | `STALE_DAYS` 定数1箇所で調整可。将来は per-feed の投稿頻度（機能03 `posts_per_week`）から動的に閾値を決める案もある（本チケットは固定30日） |
| **`last_published_at=None` を stale 扱い** | 日時を出さないフィード / 追加直後のフィードを stale 誤表示 | 意図的選択（「古いフィード検知」の趣旨）。誤報が多ければ `classify` の None 分岐を Healthy に変える（純関数1箇所 + テスト）。代替: `created_at` が新しいうちは猶予する（行に created_at を読み込む拡張が必要） |
| **手動 refresh の 502 と記録の二重性** | `refresh_one` は失敗時 `fetch_and_store` が記録した後に `?` で 502 を返す。記録済みなのにユーザにはエラー | 仕様として正しい（記録は副作用、HTTP は取得結果）。UI は 502 を握って health 再取得すれば最新失敗回数が見える |
| **`fetch_and_store` 改修の影響範囲** | feeds スライスのチョークポイントを触るため回帰リスク | 本体を `_inner` に丸ごと移設し1行も変えない。ラッパは成否で記録を分岐するだけ。既存の `create_feed`/`refresh_one`/`refresh_all_feeds` は無改修。`cargo test` + 取得の手動確認で担保 |
| **記録の失敗** | `record_*` の UPDATE が失敗すると健全性が古いまま | `warn!` ログのみで取得結果には影響させない（記録は補助）。DB 障害時は取得自体も失敗するため整合は崩れにくい |
| **集計クエリ性能** | `list_health` は `feeds LEFT JOIN articles` の全件 `GROUP BY`。`MAX(published_at)` は全フィード横断 GROUP BY のためインデックスが効きにくい | 単一ユーザ・家庭内規模では問題なし。記事増大時は機能03同様マテリアライズ昇格（将来・新マイグレーション） |
| **`bg-destructive` トークン未定義** | `app.css` に `--destructive` が無いとバッジが透明 | 実装時に `frontend/src/app.css` を確認。無ければ `stale` と同等の控えめ表現にフォールバック or トークン追記（生 hex は不可） |
| **タイムゾーン** | `classify` の now は `chrono::Utc::now()`、`published_at` も timestamptz。境界（30日）付近で1日ずれうる | 表示用メトリクスとして許容（家庭内・単一ユーザ）。厳密化が要れば日数差をサーバ計算に寄せる |
