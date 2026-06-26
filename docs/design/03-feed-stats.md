# 03 最終投稿経過日数と投稿頻度の表示

> 読み手向けメモ: このドキュメントは「リポジトリは手元にあるが、この会話の文脈を知らない別セッションの実装者」が、これだけ読めば着手できる粒度で書いている。裏取りした実ファイル（このセッションで実際に開いて確認した）: `backend/src/features/stats/{domain,repository,service,handler,mod}.rs`, `backend/src/features/feeds/{repository,domain,mod}.rs`, `backend/src/features/mod.rs`, `backend/src/shared/{state,db}.rs`, `backend/migrations/0001_init.sql`, `backend/Cargo.toml`(crate 名 `rss-reader-backend` / `sqlx` は `uuid`+`chrono` feature 有効), `frontend/src/lib/api.ts`, `frontend/src/routes/FeedList.tsx`, `frontend/src/index.tsx`, `scripts/test/api-stats.sh`, `justfile`(`test:` = `cd backend && cargo test` / `lint:` = `cargo clippy --all-targets -- -D warnings`)。

## 1. 概要

フィードごとに「**最終投稿からの経過日数**」と「**投稿頻度（週あたり本数）**」を表示する。どのフィードが活発で、どれが止まっているかを一目で把握でき、購読の棚卸し（削除・整理）判断を助ける。

実装は `articles.published_at` を**読み取り時に集計**する。実体カラムは増やさず（マイグレーション不要）、`feeds` を `LEFT JOIN articles` した runtime 集計クエリ1本で「フィードごとの最大 `published_at`（最終投稿）」と「直近30日の投稿本数（→ 週あたり本数へ換算）」を返す。

この集計は単一機能の枝葉ではなく、**フィード別の read model（CQRS-lite）**として新スライス `feed_overview` に置く。同じ1クエリで `unread_count` / `total_count` も返すため、機能01（フィード管理の未読バッジ）・機能09（既読管理の未読数）が**この同じエンドポイントを再利用**する。つまり本機能はそれらの土台でもある。既存 `stats` スライス（グローバル集計）の「フィード粒度版」であり、構成をそっくり踏襲する。

## 2. スコープ / 非スコープ

**含む（このチケットでやる）**
- 新スライス `backend/src/features/feed_overview/`（`domain.rs` / `repository.rs` / `service.rs` / `handler.rs` / `mod.rs`）。
- `GET /api/feeds/overview` 1本。フィード別 `{ feed_id, total_count, unread_count, last_published_at, posts_per_week }` の配列を返す（記事ゼロのフィードも1行返す）。
- `posts_per_week`（投稿頻度。1桁丸めまで含む）を導出する純粋関数 + その単体テスト。
- 値を検証する結合テスト（`scripts/test/api-feed-overview.sh`。DB を psql で決定論的にシードし、計算結果の**実値**を assert する）。
- フロント: `lib/api.ts` に `listFeedOverview()` と `FeedOverview` 型を追加。`lib/format.ts` に「N日前」「週Y件」整形ヘルパ。**自己完結の per-feed 表示コンポーネント** `components/feed/FeedStatsList.tsx`（自前 `listFeeds()`+`listFeedOverview()` を id 突合して各フィード行に2指標を出す）を新設し、`/manage` 不在でも表示確認できるようにする。

**含まない（別チケット / 別機能）**
- フィードのリネーム / フォルダ割当 / per-feed refresh（機能01・`feeds` スライス拡張）。
- フォルダ機能・`feeds.folder_id`（機能02、`0002_folders.sql`）。本スライスは folder を一切参照しない。
- 一括既読・未読フィルタトグル（機能09 / 11）。`unread_count` を**返す**だけで、消し込み操作はしない。
- 集計結果のマテリアライズ（実体カラム/集計テーブル化）。記事数が増えて遅くなったときの将来課題（§11）。
- `/manage` ルートそのものの新設・二ペインシェル化（機能01 / 10）。本機能は「自己完結コンポーネントを1つ作り、行に2指標を出す」差分に閉じ、ホスト画面が無い段階でも単体で表示確認できる。

## 3. 既存実装の調査と再利用

**車輪の再発明をしないため、以下を再利用する。** 以下はこのセッションで実ファイルを開いて確認済み。

- **`stats` スライスが読み取り集計の前例**（`backend/src/features/stats/`）。`repository::fetch` は `sqlx::query_as::<_, (i64,i64,i64)>(SELECT (SELECT COUNT(*) ...), ...)` を `fetch_one` で取得 → `service::get_stats` が素通し → `handler::get` が `Json(Stats)` 返却、`mod.rs` が `Router::new().route("/api/stats", get(handler::get))`。本スライスはこれを「グローバル集計」から「フィード粒度の集計」へ広げた版で、構成をそっくり踏襲する。
- **`articles.published_at`（`TIMESTAMPTZ` nullable）と `idx_articles_published_at (published_at DESC NULLS LAST)`**（`0001_init.sql`）が既にある。最終投稿 = `MAX(published_at)`、直近30日 = `published_at >= now() - interval '30 days'` をこの列で計算でき、**新カラム不要**。
- **`articles.is_read`（`NOT NULL DEFAULT false`）と部分インデックス `idx_articles_is_read WHERE is_read=false`**（`0001_init.sql`）。未読数 = `COUNT(*) FILTER (WHERE is_read=false)`。`stats` の `unread`（`COUNT(*) ... WHERE is_read=false`）が同型クエリを既に使っている。
- **`idx_articles_feed_id`**（`0001_init.sql`）。`feeds f LEFT JOIN articles a ON a.feed_id=f.id` の結合キー。
- **`features/mod.rs` の `.merge()` 合成規約**。現状 `router()` は `health/feeds/articles/stats` の4枚を `.merge()` で合成している。本スライスも同じ1行追加で済む。
- **`shared/error.rs` の `AppError` + `AppResult`**。`sqlx::Error` は `#[from]` で `Database`(500) に自動変換され、ハンドラは `?` 伝播するだけ。新バリアント不要。
- **`feeds/repository.rs::list_all` の並び順**（`ORDER BY created_at DESC`）。本スライスの一覧順もこれに揃え、フロントの `listFeeds()` 順と一致させる。
- **フロント `lib/api.ts` の `http<T>()` ヘルパ**（204→undefined 畳み込み、非2xx は throw）。`listFeeds()` と同型の GET メソッドを1つ足すだけ。既存 `Feed` 型はそのまま使い、id 突合で結合する（overview 側に title/url を重複させない）。
- **プロジェクトのテスト実慣習**: 純粋ロジックは `#[cfg(test)] mod tests`（`feeds/domain.rs` の `FeedUrl::parse` 群が前例）。「結合テスト」は **`scripts/test/*.sh` で起動済みスタックに HTTP を投げる**方式（`scripts/test/api-stats.sh` が前例）。`backend/tests/` ディレクトリは**現状存在しない**。本機能もこの2方式に従う（後述 §9・§11 で土台設計の記述ズレを是正する旨を明記）。

## 4. データモデルとマイグレーション

**DB 変更なし（マイグレーション追加なし）。**

理由: 必要な情報（`articles.published_at`, `articles.is_read`, `articles.feed_id`, `feeds.id`, `feeds.created_at`）はすべて `0001_init.sql` に既存。最終投稿・投稿頻度・未読数はいずれも**読み取り時に集計**でき、実体カラムを持たせる必要がない。土台設計（00-foundation-backend）でも 03/09/01 の集計系は「読み取り時計算でマイグレーション不要」と明記されており、本書はそれに従う。

> 補足: 土台設計が予約するマイグレーション番号は `0002_folders` / `0003_instapaper` / `0004_read_later`。本機能はこのいずれも使わない。将来、記事数増大で集計が重くなった場合は新しい空き番号で集計列/マテビューを追加する（§11）。既存 `0001_init.sql` は編集しない。

## 5. バックエンド設計

新スライス `backend/src/features/feed_overview/`。**書き込みなし・読み取り専用**。他テーブル（`feeds`/`articles`）を自前 SQL で読むが、これは禁止される「越境共通レイヤー」ではなく、`stats` と同型の独立した読み取りスライス（CQRS-lite）である。新 trait / dyn は追加しない。

> **命名の確定（重要・他ドキュメントの修正が必要）**: バックエンド土台設計はこのスライスを **`feed_overview` / `GET /api/feeds/overview`** と名付けている。フロント土台設計（§4.3/§4.5）と機能01・09 の旧ドラフトは同じ契約を `feed_stats` / `GET /api/feeds/stats` / `listFeedStats()` / `FeedStat` と呼んでいる。**本書では `feed_overview` / `/api/feeds/overview` / `FeedOverview` / `listFeedOverview()` を唯一の正とする。** これは「相互確認してね」では不十分で、放置すると 01/09 が旧名でエンドポイントを二重実装するリスクがある。実装着手前に**フロント土台設計と 01/09 の設計書を canonical 名へ編集**すること（具体的な置換指示は §11 末尾）。

### 5.1 ルート設計と衝突回避

`/api/feeds/overview` は静的セグメント。`feeds` スライスは実コードで `/api/feeds`（GET/POST）、`/api/feeds/{id}`（DELETE）、`/api/feeds/{id}/refresh`（POST）を持つ（`feeds/mod.rs` で確認済み）。**`GET /api/feeds/{id}` は存在しない**ため、別スライスの `GET /api/feeds/overview` と method+path が重複せず、`.merge()` で衝突しない。axum 0.8（matchit 0.8）は静的セグメントを動的 `{id}` より優先するため、仮に将来 `GET /api/feeds/{id}` が足されても `overview` が先にマッチする。複数スライスが同一プレフィックス `/api/feeds` に merge するのは結合ではない（土台設計 §2.1）。

### 5.2 `domain.rs`

```rust
use serde::Serialize;
use uuid::Uuid;

/// リポジトリが返す「素の集計行」。週あたり本数は持たず、直近30日の本数だけを持つ。
///
/// feed_id は read model の相関キーなので、あえて feeds スライスの `FeedId` newtype を
/// import せず `Uuid` を直接使う。これはスライス間の型結合を作らないための意図的な選択で、
/// グローバル集計の前例 `stats` が `feeds`/`articles` のキーを一切持たず素の `i64` を返すのと
/// 同じ方針（読み取り read model はドメイン newtype を跨いで持ち込まない）。serde で UUID 文字列に
/// シリアライズされ、フロントの `feed_id: string` と一致する。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FeedOverviewRow {
    pub feed_id: Uuid,
    pub total_count: i64,
    pub unread_count: i64,
    pub last_published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub recent_count_30d: i64,
}

/// API レスポンス1行（読み取り専用 read model）。
#[derive(Debug, Clone, Serialize)]
pub struct FeedOverview {
    pub feed_id: Uuid,
    pub total_count: i64,
    pub unread_count: i64,
    /// 最終投稿時刻。記事ゼロ or published_at が全て NULL なら None。
    pub last_published_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 週あたり投稿本数（直近30日の本数から換算、小数1桁に丸め済み）。
    pub posts_per_week: f64,
}

/// 投稿頻度の純粋関数。直近30日 = 30/7 週なので per_week = count * 7 / 30。
/// **小数第1位に丸めて返す**（payload を綺麗に保ち、結合テストの値 assert も簡潔にするため）。
/// この丸めは表示用メトリクスとして意図的で、家庭内・単一ユーザ規模では情報欠落は問題にならない。
///
/// 「最終投稿経過日数」は now() 依存を避けるため backend では計算せず、
/// last_published_at をそのまま返してフロントで「N日前」に整形する（土台設計 §3）。
pub fn posts_per_week(recent_count_30d: i64) -> f64 {
    let raw = (recent_count_30d as f64) * 7.0 / 30.0;
    (raw * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn zero_recent_posts_is_zero_per_week() {
        assert!(approx(posts_per_week(0), 0.0));
    }

    #[test]
    fn thirty_in_thirty_days_is_seven_per_week() {
        assert!(approx(posts_per_week(30), 7.0));
    }

    #[test]
    fn fifteen_in_thirty_days_is_three_and_half_per_week() {
        assert!(approx(posts_per_week(15), 3.5));
    }

    #[test]
    fn two_in_thirty_days_rounds_to_point_five() {
        // raw = 0.4666… → 1桁丸めで 0.5。結合テストの feed A が踏むケース。
        assert!(approx(posts_per_week(2), 0.5));
    }

    #[test]
    fn ten_in_thirty_days_rounds_to_two_point_three() {
        // raw = 2.3333… → 2.3。丸めが切り捨て側に倒れることの確認。
        assert!(approx(posts_per_week(10), 2.3));
    }

    #[test]
    fn result_is_non_negative_and_increases_with_count() {
        assert!(posts_per_week(30) > posts_per_week(0));
        assert!(posts_per_week(100) >= 0.0);
    }
}
```

> clippy 注意: `recent_count_30d as f64` は `clippy::cast_precision_loss`（pedantic グループ）に該当しうるが、`just lint` は `cargo clippy --all-targets -- -D warnings`（既定 lint への `-D warnings`）であり pedantic は有効化していないため警告にならない。万一 pedantic を入れる方針になったら関数頭に `#[allow(clippy::cast_precision_loss)]` を付ける。

### 5.3 `repository.rs`

```rust
use sqlx::PgPool;

use super::domain::FeedOverviewRow;
use crate::shared::error::AppResult;

/// feeds を起点に articles を LEFT JOIN し、フィード別の集計を1クエリで返す。
/// LEFT JOIN なので記事ゼロのフィードも1行返り、COUNT(a.id)=0 / MAX=NULL になる。
pub async fn fetch_overview(pool: &PgPool) -> AppResult<Vec<FeedOverviewRow>> {
    let rows = sqlx::query_as::<_, FeedOverviewRow>(
        r#"SELECT
             f.id AS feed_id,
             COUNT(a.id)                                                   AS total_count,
             COUNT(a.id) FILTER (WHERE a.is_read = false)                  AS unread_count,
             MAX(a.published_at)                                           AS last_published_at,
             COUNT(a.id) FILTER (
               WHERE a.published_at >= now() - interval '30 days'
             )                                                             AS recent_count_30d
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
- **`query_as`（runtime クエリ）のみ。`query!` マクロは使わない**（ビルド時 DB 接続を要求するため禁止、CLAUDE.md / 土台設計）。
- `COUNT(a.id)` は `BIGINT`(`i64`)、LEFT JOIN で未マッチ時は `a.id` が NULL なので COUNT が無視し 0 になる。`FILTER` 付き COUNT も同様に 0。`a.published_at` が NULL の記事は `recent_count_30d` の述語が NULL→不成立で数えられない。`MAX(published_at)` は NULL を無視し、全件 NULL/0件なら NULL → `Option<DateTime<Utc>>`。
- `GROUP BY f.id`（主キー）に対し `ORDER BY f.created_at DESC` は PostgreSQL の関数従属性により合法（`f.id` が PK なので `f.created_at` を集計せず参照・整列できる）。`feeds/repository.rs::list_all` と同じ `created_at DESC` に揃える。
- 取得行が無い（フィード0件）場合は空 Vec を返す。エラーにしない。
- `sqlx::FromRow` の derive はカラム名でマッピングするので、`f.id AS feed_id` のエイリアスが `FeedOverviewRow.feed_id` に対応する。`sqlx` は `Cargo.toml` で `uuid`+`chrono` feature が有効なので `Uuid` / `DateTime<Utc>` をそのまま `FromRow` で受けられる（確認済み）。

### 5.4 `service.rs`

```rust
use super::domain::{self, FeedOverview};
use super::repository;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

/// 集計行を読み、週あたり本数を導出して read model に詰め替える。
pub async fn list_overview(state: &AppState) -> AppResult<Vec<FeedOverview>> {
    let rows = repository::fetch_overview(&state.db).await?;
    Ok(rows
        .into_iter()
        .map(|r| FeedOverview {
            feed_id: r.feed_id,
            total_count: r.total_count,
            unread_count: r.unread_count,
            last_published_at: r.last_published_at,
            posts_per_week: domain::posts_per_week(r.recent_count_30d),
        })
        .collect())
}
```

### 5.5 `handler.rs`

```rust
use axum::extract::State;
use axum::Json;

use super::domain::FeedOverview;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn overview(State(state): State<AppState>) -> AppResult<Json<Vec<FeedOverview>>> {
    Ok(Json(service::list_overview(&state).await?))
}
```

### 5.6 `mod.rs`

```rust
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::get;
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/feeds/overview", get(handler::overview))
}
```

### 5.7 `features/mod.rs` への合成（追加2行のみ）

`backend/src/features/mod.rs` は現在以下（抜粋）。**`stats` の隣に2行足すだけ**。

```rust
pub mod articles;
pub mod feeds;
pub mod feed_overview; // ← 追加（アルファベット順で feeds の前後どちらでも可）
pub mod health;
pub mod stats;

// ...router() 内...
        .merge(stats::routes())
        .merge(feed_overview::routes()) // ← 追加
```

既存スライス（feeds / articles / stats / health）には一切手を入れない。

### 5.8 AppError の使い分け

- 一覧取得につき **`NotFound` は返さない**（0件は空配列で 200）。
- DB エラーは `sqlx::Error` → `AppError::Database`(500) に `#[from]` で自動変換、`?` 伝播。
- `Validation` / `NotEnabled` / `Upstream` は本機能では発生しない。新バリアント追加なし（`shared/error.rs` 不編集）。

## 6. フロントエンド設計

> 方針: 本機能の UI は「各フィード行に2つの数値ラベルを差し込む」だけの純表示。Ark UI のような a11y 部品は不要（自前 Tailwind のみ）。整形（「N日前」「週Y件」）はフロントで行う（土台設計 §3）。**本機能は機能01（`/manage`/`FeedManage.tsx`）にハード依存しない** —— 自前で feeds を取得して描画する独立コンポーネントを1枚持つことで自己完結させる（後述 §6.3）。

### 6.1 `lib/api.ts`（型 + メソッド追加）

```ts
export interface FeedOverview {
  feed_id: string;
  total_count: number;
  unread_count: number;
  last_published_at: string | null;
  posts_per_week: number;
}

// api オブジェクト内に追加（listFeeds と同型の GET）
listFeedOverview: () => http<FeedOverview[]>("/api/feeds/overview"),
```

- 命名は既存規約「動詞 + リソース camelCase」（`listFeeds` 等）に揃え `listFeedOverview`。
- 既存 `Feed` 型は変更しない。overview は `feed_id` 相関キーのみ持ち、`title`/`url` は `listFeeds()` 側から取って**id 突合**で結合する（重複を持たせない）。

### 6.2 整形ヘルパ `lib/format.ts`（新規）

```ts
/** ISO日時 → 「投稿なし / 今日 / 昨日 / N日前」 */
export function lastPostLabel(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "投稿なし";
  const then = new Date(iso).getTime();
  const days = Math.floor((now.getTime() - then) / 86_400_000);
  if (days <= 0) return "今日";
  if (days === 1) return "昨日";
  return `${days}日前`;
}

/** 週あたり本数 → 「投稿なし / 週Y件」（小数1桁、末尾.0は省く） */
export function postsPerWeekLabel(n: number): string {
  if (!n || n <= 0) return "投稿なし";
  const rounded = Math.round(n * 10) / 10; // backend で丸め済みだが念のため冪等に
  const text = Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1);
  return `週${text}件`;
}
```

`now` を引数注入しているのは、後でユニット/型確認するときに固定時刻で検証しやすくするため（フロントは手動/型確認方針だが、純関数なので簡単に検証できる形にしておく）。`posts_per_week` は backend が既に1桁丸めを返すため（§5.2）、`postsPerWeekLabel` の丸めは保険であり挙動を変えない。

### 6.3 表示コンポーネント `components/feed/FeedStatsList.tsx`（新規・自己完結）

レビュー指摘の核心: 現行 `routes/FeedList.tsx` は**記事一覧**（`createResource(() => api.listArticles())`）を描画しており、`listArticles()` の戻りはフィードの title/url を持たない。よって「既存フィード表示の近く」という差し込み先は**実在しない**。これを解消するため、本機能は**自前でフィード一覧を取得して描画する独立コンポーネント**を1枚持つ。これがこの機能の正式な表示ホストであり、機能01 の `/manage` 着地を待たずに動く。

```tsx
import { createMemo, createResource, For, Show } from "solid-js";
import { api, type FeedOverview } from "@/lib/api";
import { lastPostLabel, postsPerWeekLabel } from "@/lib/format";

/**
 * フィード別の「最終投稿 / 投稿頻度」一覧。
 * 自前で listFeeds() と listFeedOverview() を取得し feed.id で突合するため、
 * 記事一覧画面や機能01の /manage に一切依存しない（自己完結）。
 * 機能01 着地後は、この行レンダリングを FeedManage の各行へ移設して再利用する。
 */
export default function FeedStatsList() {
  const [feeds] = createResource(() => api.listFeeds());
  const [overview] = createResource(() => api.listFeedOverview());

  const byId = createMemo(
    () =>
      new Map<string, FeedOverview>(
        (overview() ?? []).map((o) => [o.feed_id, o] as const),
      ),
  );

  return (
    <Show
      when={!feeds.loading}
      fallback={<p class="text-sm text-muted-foreground">読み込み中…</p>}
    >
      <Show
        when={(feeds()?.length ?? 0) > 0}
        fallback={
          <p class="text-sm text-muted-foreground">フィードがありません。</p>
        }
      >
        <ul class="divide-y divide-border">
          <For each={feeds()}>
            {(feed) => {
              const o = () => byId().get(feed.id);
              return (
                <li class="flex items-center justify-between gap-3 py-3">
                  <span class="text-sm font-medium min-w-0 truncate">
                    {feed.title ?? feed.url}
                  </span>
                  <span class="text-xs text-muted-foreground whitespace-nowrap">
                    {lastPostLabel(o()?.last_published_at ?? null)} ・{" "}
                    {postsPerWeekLabel(o()?.posts_per_week ?? 0)}
                  </span>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>
    </Show>
  );
}
```

ポイント:
- `import { createResource, createMemo, For, Show } from "solid-js"` を必ず付ける（コピペ即実装のため明記）。`@/` エイリアスと既存 `api` を使う。
- **2つの `createResource` を別々に張り、`createMemo` で `Map<feed_id, FeedOverview>` を作って id 突合**。`overview` がまだ無くても `o()` は `undefined` で「投稿なし」表示にフォールバックする。
- これは記事一覧（`FeedList`）の `createResource(() => api.listArticles())` を一切触らない。article-list 画面へのフックではなく、独立した per-feed リストである。

### 6.4 表示場所（着地戦略）

- **最終形（機能01着地後）**: `routes/FeedManage.tsx`（`/manage`）の各フィード行メタに、上記コンポーネントの行レンダリング部を移設して再利用。指標は `text-xs text-muted-foreground`。
- **機能01より先に本機能が着地する場合の暫定マウント**: `routes/FeedList.tsx` の JSX 先頭に**1行だけ** `<FeedStatsList />` を差し込む（import 1行 + JSX 1行）。`FeedStatsList` は自前でデータを取るので `FeedList` のロジック（記事一覧）には触れない。機能01 が `FeedManage` を新設したら、この暫定マウント1行を削除し、最終形へ移す。`lib/format.ts` と `listFeedOverview()` はそのまま流用。

> この戦略により、§8 の通り**機能01 へのハード依存は無い**（コンポーネントが自己完結し、暫定マウント先も既存ルート1箇所への最小差し込みで足りる）。

### 6.5 状態管理・トークン

- 新しいグローバル状態は不要（純表示）。各 `createResource` のローカルに閉じる。
- 装飾は意味トークンのみ: メタは `text-xs text-muted-foreground`、行区切りは `divide-y divide-border`、行は `py-3`。新色・生 hex は持ち込まない（oklch トークン維持、土台設計 §5）。長いタイトルは `min-w-0 truncate` でグリッド破綻を防ぐ。
- `unread_count` をバッジ表示するのは機能01/09 の責務。本機能はレスポンスに含めるだけで、UI バッジ化はしない（重複実装を避ける）。

## 7. API 契約

### `GET /api/feeds/overview`

- リクエスト: クエリ・ボディなし。
- 認証/有効化フラグ: なし（常に有効）。
- レスポンス `200 OK`: `FeedOverview` の配列（フィード作成日時の降順、記事ゼロのフィードも含む）。

```json
[
  {
    "feed_id": "7b1c0d2e-2a3b-4c5d-8e9f-0a1b2c3d4e5f",
    "total_count": 128,
    "unread_count": 12,
    "last_published_at": "2026-06-25T22:14:00Z",
    "posts_per_week": 4.9
  },
  {
    "feed_id": "9f8e7d6c-5b4a-3c2d-1e0f-aabbccddeeff",
    "total_count": 0,
    "unread_count": 0,
    "last_published_at": null,
    "posts_per_week": 0.0
  }
]
```

- フィールド意味:
  - `feed_id`: `feeds.id`（UUID 文字列）。フロントは `listFeeds()` と id 突合。
  - `total_count` / `unread_count`: そのフィードの総記事数 / 未読数（`i64`）。
  - `last_published_at`: `MAX(articles.published_at)`。記事ゼロ or 全件 `published_at` が NULL のとき `null`。経過日数の整形はフロント。
  - `posts_per_week`: 直近30日の投稿本数 × 7 / 30 を**小数第1位に丸めた値**（`f64`）。0.0 は「直近30日投稿なし」。
- エラー: DB 障害時 `500 {"error":"internal error"}`（`AppError::Database`）。それ以外の異常系は無し。

## 8. 依存関係

- **依存する機能（このチケットが必要とするもの）: なし。`dependsOn` は空。** バックエンドは既存 `feeds`/`articles` テーブルのみで完結。フロントも自己完結コンポーネント `FeedStatsList.tsx`（§6.3）が `listFeeds()`+`listFeedOverview()` を自前取得するため、**機能01 の `/manage`/`FeedManage.tsx` を待たない**（レビュー指摘の「存在しないホスト」問題を解消）。
- **このチケットがブロックする / 土台になる機能**:
  - 機能01（feed-management）: フィード別 `unread_count` バッジと管理行メタを本エンドポイントから取得（土台設計マトリクス「01 = feed_overview(未読数)」）。最終的に本機能の per-feed 行レンダリングを `FeedManage` 行へ移設して同居。
  - 機能09（read-management）: サイドバー未読数を本エンドポイント（または将来の派生）から取得。
- 関係するが本機能では触れないもの: 機能02（フォルダ）。本スライスは `folder_id` を一切参照しない。フォルダ別集計が要るなら配下フィード行をフロントで合算する（専用エンドポイントは作らない、土台設計 §3）。

## 9. テスト計画（TDD）

**Red → 理解 → Green の順。書いたら必ず実行する。**

> テスト配置についての前提是正: 土台設計（00-foundation-backend §5）は「`stats` の前例 = `backend/tests/`（MEMORY first-api-stats）」と書くが、**これは事実誤認**。実リポジトリに `backend/tests/` は存在せず、`stats` の結合テストは `scripts/test/api-stats.sh`（起動済みスタックへ HTTP）である。加えて本 crate は**バイナリ専用（`src/lib.rs` も `[lib]` セクションも無い）**ため、`backend/tests/*.rs` から内部モジュール（`repository::fetch_overview` 等）を `use` できない（library target が無い）。library target の新設は crate ルート（`main.rs`/`Cargo.toml`）に手を入れる横断変更で、並行開発中の他スライスと衝突しうるため本チケットの範囲外とする。**よって本機能の値検証は、実慣習どおり `scripts/test/*.sh` 側で、psql による決定論シードを使って実値を assert する**（純粋ロジックは `#[cfg(test)]`）。この是正は土台設計へフィードバックすること（§11）。

### 9.1 単体テスト（`#[cfg(test)] mod tests`、§5.2 に同梱・DB 不要）

`backend/src/features/feed_overview/domain.rs` に純粋関数 `posts_per_week` のテストを**先に**書く（Red）。

| テスト | 意図 |
|--------|------|
| `zero_recent_posts_is_zero_per_week` | 直近30日ゼロ → 0.0。境界（dormant フィード） |
| `thirty_in_thirty_days_is_seven_per_week` | 30本/30日 → 7.0。換算式 ×7/30 の正しさ |
| `fifteen_in_thirty_days_is_three_and_half_per_week` | 15本 → 3.5。小数になるケース |
| `two_in_thirty_days_rounds_to_point_five` | 2本 → 0.5（raw 0.4667 の1桁丸め）。結合テスト feed A が踏む値 |
| `ten_in_thirty_days_rounds_to_two_point_three` | 10本 → 2.3（raw 2.3333 の丸め方向） |
| `result_is_non_negative_and_increases_with_count` | 非負・件数増で増加（符号と単調性の不変条件） |

実行: `cd backend && cargo test feed_overview`（DB 不要）。`just lint`（clippy `-D warnings`）も通す。

### 9.2 結合テスト（`scripts/test/api-feed-overview.sh`、新規・**実値を assert**）

`api-stats.sh` を雛形に、**psql で決定論的にデータをシード**してから HTTP を叩き、計算結果の**実値**を assert する（レビュー指摘②の解消：キー存在ではなく値を検証）。内部 DB へは `docker compose exec -T db psql` で到達する（compose の DB はホスト非公開のため。`scripts/test/api-stats.sh` は HTTP のみだが、本テストは値検証のため DB シードが要る）。

シード（決定論）:
- **feed A**（`id = 00000000-0000-0000-0000-0000000000aa`）に記事4本:
  - a1: `published_at = now() - interval '1 day'`, `is_read=false`（直近30日・未読）
  - a2: `published_at = now() - interval '5 days'`, `is_read=false`（直近30日・未読）
  - a3: `published_at = now() - interval '40 days'`, `is_read=true`（30日外・既読）
  - a4: `published_at = NULL`, `is_read=true`（日時なし・既読）
  - 期待: `total_count=4`, `unread_count=2`, `last_published_at != null`（= a1、NULL を無視した MAX）, `recent_count_30d=2` → `posts_per_week = round(2*7/30,1) = 0.5`
- **feed B**（`id = 00000000-0000-0000-0000-0000000000bb`）は記事0本:
  - 期待: LEFT JOIN により行は返るが `total_count=0`, `unread_count=0`, `last_published_at=null`, `posts_per_week=0`

assert はバージョン差（jq 1.6/1.7 の数値リテラル正規化差）を避けるため `jq -e` の述語内で行い、`posts_per_week` は `(.posts_per_week*10|round)` で整数化して比較する。

```bash
#!/usr/bin/env bash
# Integration test for feed_overview: seeds deterministic rows via psql, then
# asserts the COMPUTED VALUES of GET /api/feeds/overview (not just key presence).
# Requires: running stack (nginx :8081), docker compose, jq.
set -uo pipefail

URL="${URL:-http://localhost:8081/api/feeds/overview}"
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rssreader}"
PGDB="${POSTGRES_DB:-rssreader}"
A="00000000-0000-0000-0000-0000000000aa"
B="00000000-0000-0000-0000-0000000000bb"

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() { psql -q -c "DELETE FROM feeds WHERE id IN ('$A','$B');" >/dev/null 2>&1 || true; }
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

# --- seed (idempotent) ---
psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id IN ('$A','$B');
INSERT INTO feeds (id, url, title) VALUES
  ('$A', 'https://example.test/feed-a.xml', 'feed A'),
  ('$B', 'https://example.test/feed-b.xml', 'feed B');
INSERT INTO articles (id, feed_id, url, title, published_at, is_read) VALUES
  (gen_random_uuid(), '$A', 'https://example.test/a1', 'a1', now() - interval '1 day',  false),
  (gen_random_uuid(), '$A', 'https://example.test/a2', 'a2', now() - interval '5 days', false),
  (gen_random_uuid(), '$A', 'https://example.test/a3', 'a3', now() - interval '40 days', true),
  (gen_random_uuid(), '$A', 'https://example.test/a4', 'a4', NULL,                       true);
SQL

# --- fetch ---
body="$(curl -s -m 5 -w '\n%{http_code}' "$URL")"
code="${body##*$'\n'}"; json="${body%$'\n'*}"
[ "$code" = "200" ] || fail "expected 200, got $code ($json)"
case "$json" in "["*) : ;; *) fail "not a JSON array: $json";; esac

# --- assert feed A computed values ---
echo "$json" | jq -e --arg id "$A" '
  (map(select(.feed_id==$id)) | first) as $a
  | $a != null
    and $a.total_count == 4
    and $a.unread_count == 2
    and $a.last_published_at != null
    and ((($a.posts_per_week * 10) | round) == 5)   # 0.5
' >/dev/null || fail "feed A aggregates wrong: $(echo "$json" | jq -c --arg id "$A" 'map(select(.feed_id==$id))')"

# --- assert feed B (zero-article feed STILL returns a row) ---
echo "$json" | jq -e --arg id "$B" '
  (map(select(.feed_id==$id)) | first) as $b
  | $b != null
    and $b.total_count == 0
    and $b.unread_count == 0
    and $b.last_published_at == null
    and ((($b.posts_per_week * 10) | round) == 0)   # 0.0
' >/dev/null || fail "feed B zero-row wrong: $(echo "$json" | jq -c --arg id "$B" 'map(select(.feed_id==$id))')"

echo "PASS: /api/feeds/overview computed values (feed A 4/2/0.5, feed B 0/0/null/0)"
```

- **Red**: 実装前は `/api/feeds/overview` が 404 → スクリプトが「expected 200」で落ちる。実装後 Green。
- 環境変数 `URL` / `DB_SVC` / `POSTGRES_USER` / `POSTGRES_DB` で接続先を上書き可能。`jq` 必須（macOS は `brew install jq`、compose イメージは Debian なので `apt`）。`docker compose exec` で内部 DB に到達するため、ホストへ DB ポートを公開していなくても動く。
- これにより、レビュー指摘②「数値表示機能なのに値の正しさを検証していない」を解消する（LEFT JOIN の zero-row、`COUNT FILTER` の未読/直近30日カウント、`MAX` の NULL 無視、`posts_per_week` 換算をすべて実値で確認）。

### 9.3 フロント（手動 / 型）

- `tsc` 型チェック（`just lint` の `pnpm typecheck`）で `FeedOverview` 型・`listFeedOverview()`・`FeedStatsList.tsx` の整合を確認。
- 手動: 活発なフィード（直近投稿あり）と止まったフィード（30日以上投稿なし or 記事ゼロ）の両方で「N日前」「週Y件」「投稿なし」が正しく出ることを目視。`lastPostLabel` の境界（今日/昨日/N日前/null）を確認。

## 10. 実装手順（順序付きチェックリスト）

1. ブランチを切る（例 `feat/feed-overview-stats`）。`main` 直コミットしない。
2. `backend/src/features/feed_overview/` を作成し5ファイルを置く:
   - `domain.rs`（§5.2。**まず `#[cfg(test)] mod tests` を書いて Red**、`FeedOverviewRow` / `FeedOverview` / `posts_per_week`）。
   - `repository.rs`（§5.3 の集計クエリ）。
   - `service.rs`（§5.4）。
   - `handler.rs`（§5.5）。
   - `mod.rs`（§5.6、`routes()`）。
3. `cd backend && cargo test feed_overview` で単体テストを Green にする。
4. `backend/src/features/mod.rs` に `pub mod feed_overview;` と `.merge(feed_overview::routes())` を1行ずつ追加（§5.7）。他スライスは触らない。
5. `cargo build` → `just lint`（`clippy -D warnings` + `pnpm typecheck`）を通す。`cargo fmt`。
6. スタックを起動（`just up`、または `just dev-db` + `just back`）。
7. `scripts/test/api-feed-overview.sh` を追加（§9.2）、実行 → feed A=4/2/0.5・feed B=0/0/null/0 を Green で確認。手で `curl http://localhost:8081/api/feeds/overview | jq` も見る。
8. フロント: `lib/api.ts` に `FeedOverview` 型と `listFeedOverview()` を追加（§6.1）。`lib/format.ts` に `lastPostLabel` / `postsPerWeekLabel` を追加（§6.2）。
9. `components/feed/FeedStatsList.tsx` を新設（§6.3）。`/manage`（機能01）が既にあればそこへ行レンダリングを移設、無ければ `routes/FeedList.tsx` の JSX 先頭に `<FeedStatsList />` を暫定マウント（§6.4）。
10. `just lint`（tsc）を通し、活発/休止の両フィードで表示を目視確認。
11. **他ドキュメントの命名を canonical 名へ編集**（§11 末尾の置換指示）。フロント土台設計 §4.3/§4.5 と機能01・09 の設計書を `feed_overview` / `/api/feeds/overview` / `listFeedOverview()` / `FeedOverview` に統一。
12. ユーザーが望むタイミングでコミット（メッセージ末尾に `Co-Authored-By` 行）。新規マイグレーションが無いことを最終確認。

## 11. リスク・未決事項・代替案

- **投稿頻度の定義（採用＝30日窓カウント）**: 「直近30日の本数 ×7/30、小数1桁丸め」を採用。利点=SQL が単純・バースト耐性・「30日無投稿=0」が休止判定として自然。欠点=作成3日目の新フィードは過小評価。**代替案**: 「直近N件（例10件）の `published_at` の平均間隔から週換算」。フィード固有リズムに追従し低頻度に強いが、SQL が `LATERAL`/窓関数で複雑化し純関数テストの入力も増える。**判断保留**: まず30日窓で出し、実データで違和感が出たら平均間隔方式へ差し替え（`repository.rs` の SQL と `domain::posts_per_week` のシグネチャ変更に閉じる）。窓日数（30）も将来パラメタ化候補。
- **`posts_per_week` の丸め位置（採用＝backend で1桁丸め）**: API payload を綺麗に保ち（`2.3333333333335` のような値を出さない）、結合テストの値 assert も簡潔にするため backend 側で `round(x,1)` する。情報量は落ちるが表示専用メトリクスとして意図的。生精度が必要になれば丸めを外しフロント表示側のみで丸める方式へ戻せる（純関数1箇所の変更）。
- **パフォーマンス**: `recent_count_30d` の `COUNT(*) FILTER (... published_at >= now()-interval '30 days')` と全件 `GROUP BY f.id` は、`articles` 全体を seq-scan して集計する。`idx_articles_published_at` は範囲述語が集計内 FILTER であり全フィード横断の GROUP BY のため**効かない**（インデックスの恩恵を受けない）。単一ユーザ・家庭内 LAN 規模では当面問題なし。閾値を超えたら**新マイグレーション（空き番号）**で集計列 or マテリアライズドビューへ昇格し、`repository.rs` のクエリを差し替える（読み取りスライス内に閉じる、§4 補足）。
- **`feed_id` を newtype にしない件**: 読み取り read model は `feeds` の `FeedId` newtype を import せず素の `Uuid` を持つ。これはスライス間の型結合を避けるための意図的選択で、グローバル集計の前例 `stats` がキーに素の `i64` を返すのと同じ方針（土台設計「PK=newtype」は書き込み側ドメインの話で、CQRS 読み取り read model には適用しない）。レビュー時にこの一文を根拠とすること。
- **`GROUP BY f.id` + `ORDER BY f.created_at` の関数従属性**: PostgreSQL は主キーグループ化時に許可するが、念のため実装時に `psql` で1度実行確認する。万一エラーになる環境があれば `GROUP BY f.id, f.created_at` に変更（結果は不変）。
- **タイムゾーン**: `published_at`/`now()` とも timestamptz。経過日数はフロントでクライアントのローカル日付差として計算するため、サーバ/クライアントの TZ 差で「今日/昨日」の境界が1日ずれる可能性。家庭内・単一ユーザ前提で許容。厳密化が要れば backend で `days_since` を返す案もあるが now() 依存をレスポンスに持ち込むため現状は採らない。
- **`published_at` が NULL の記事**: フィードが日時を提供しないと NULL になり、`MAX` も直近30日カウントも NULL を無視する。結果そのフィードは記事があっても `last_published_at: null`・`recent` 不算入になりうる。`created_at` を代替に使う案は将来検討（本チケットでは published_at のみ）。結合テスト feed A はこの「NULL 記事が total には数えられるが MAX/recent には効かない」挙動を実値で押さえている。
- **テスト配置の土台設計ズレ（フィードバック必須）**: §9 冒頭の通り、土台設計の「`stats` の前例 = `backend/tests/`（MEMORY first-api-stats）」は事実誤認（`backend/tests/` は不在、本 crate はバイナリ専用で library target が無く `tests/` から内部 fn を呼べない、実慣習は `scripts/test/*.sh`）。**他スライスが `backend/tests/` を場当たり的に新設しないよう、00-foundation-backend §5 を「結合テストは `scripts/test/*.sh`（実値検証は psql シード）」へ訂正する**こと。将来 library target を導入する判断をするなら、それ自体を独立チケットにする。
- **命名統一（cross-check では不十分、編集必須）**: 本書は `feed_overview` / `/api/feeds/overview` / `FeedOverview` / `listFeedOverview()` を正とする。放置すると 01/09 が旧名 `/api/feeds/stats` でエンドポイントを二重実装するリスクがあるため、以下を**実際に編集**すること:
  - フロント土台設計 §4.3/§4.5: `feed_stats`→`feed_overview`、`GET /api/feeds/stats`→`GET /api/feeds/overview`、`listFeedStats()`→`listFeedOverview()`、`interface FeedStat`→`interface FeedOverview`（フィールドは本書 §6.1 に合わせる）。
  - 機能01・機能09 の設計書中の同名参照も同様に置換。
  - 置換後、エンドポイントが本書の1本（`GET /api/feeds/overview`）に集約されていることを確認する。
