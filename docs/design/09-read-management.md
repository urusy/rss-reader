# 09 既読管理

自動既読・一括既読・未読件数表示。

## 1. 概要

記事の既読/未読の扱いを「実用に足る」状態まで仕上げる。現状は記事を開いても既読にならず、まとめて既読にする手段もなく、未読が何件あるか分からない。本機能で **(a) 記事を開いたら自動で既読**、**(b) フィード単位／全体の一括既読**、**(c) 未読件数の表示** を実現し、RSS リーダーとして当然期待される「読んだものは消え、未読だけを追える」体験を成立させる。バックエンドの追加は一括既読エンドポイント1本だけで、残りは既存資産（`articles.is_read`、`POST /api/articles/{id}/read`、`GET /api/stats`、`http<T>` の 204 畳み込み）の再利用とフロント配線に閉じる。

## 2. スコープ / 非スコープ

含む:
- バックエンド: articles スライスに一括既読エンドポイント `POST /api/articles/read-all`（body `{ feed_id?: uuid }`、body 自体を省略してもよい。`feed_id` 省略/null = 全フィード対象）を追加。
- フロント: `ArticleView` で記事ロード時に未読なら自動既読（既存 `POST /api/articles/{id}/read` を再利用）。
- フロント: 「すべて既読」（全体）／「このフィードを既読」（フィード単位）ボタンの配線。
- フロント: 未読件数の表示。全体未読は既存 `GET /api/stats` の `unread`。フィード別未読は feed-stats（03）の per-feed 集計を消費。
- 既読/未読の整合更新（既読化後に未読カウントを再取得）。

含まない（非スコープ）:
- フォルダ単位の一括既読（`feeds.folder_id` に依存。feed-folders(02) が入った後の拡張として §11 に記載）。
- 未読/既読の **フィルタ切替 UI**（すべて/未読トグル）は別機能 11（unread-filter-toggle）。本機能はカウント表示と一括/自動既読まで。
- per-feed 未読数を返す集計エンドポイント自体の新設は feed-stats（03）の責務。本機能はその契約を **消費するだけ**（§8 に契約の固定化を明記）。
- 既読を手動で「未読に戻す」一括操作（個別トグルは既存 `POST /{id}/read {read:false}` で可能。一括 unread は要件外）。
- サイドバー/二ペインシェルそのものの構築（two-pane-layout(10) の責務）。本機能はそこにバッジとボタンを差し込む。

## 3. 既存実装の調査と再利用

裏取りした実ファイル: `backend/src/features/articles/{domain,repository,service,handler,mod}.rs`, `backend/src/features/stats/*`, `backend/migrations/0001_init.sql`（スキーマは文脈情報で確認）, `backend/src/shared/error.rs`, `frontend/src/lib/api.ts`, `frontend/src/routes/{ArticleView,FeedList}.tsx`, `frontend/src/index.tsx`, `scripts/test/api-stats.sh`, `justfile`（`test:` = `cd backend && cargo test`）, `compose.yml`（DB サービス名 `db` / DB 名 `rssreader`）。

そのまま再利用する資産（再発明しない）:
- **`articles.is_read BOOLEAN NOT NULL DEFAULT false`** カラムは既存。**部分インデックス `idx_articles_is_read(is_read) WHERE is_read=false`** も既存 → 未読走査・未読の一括更新が効率的に効く。**カラム/インデックスの追加は不要**。
- **`repository::set_read(pool, id, read)`**（`backend/src/features/articles/repository.rs:63`）と **`service::mark_read`**（`service.rs:25`）、**`POST /api/articles/{id}/read`（204 を返す、`handler.rs:44`）** が既存 → **自動既読はこの既存エンドポイントを叩くだけ**でよい。新規の単記事既読 API は作らない。
- **`repository::list(pool, feed_id, unread_only)`**（`repository.rs:36`）が `unread_only` フィルタを持ち、`($1::uuid IS NULL OR feed_id = $1)` の NULL 分岐も既にある → 一括既読 SQL はこのパターンを踏襲。一覧側の未読絞り込みも API 既対応（消費は 11 の責務）。
- **`GET /api/stats` → `{ feeds, articles, unread }`**（`stats` スライス）に **グローバル未読数が既にある** → 全体未読バッジはこれを使う。新エンドポイント不要。
- **`scripts/test/api-stats.sh`** が「稼働中スタック（nginx :8081）へ curl して HTTP/JSON を検証する」結合テストの前例。ただし api-stats.sh は **読み取り専用（GET）** であり、本機能のテストは **書き込み（既読化）を伴い実 DB の状態を変える**点が異なる（§9 で隔離・再シードと破壊性の警告を扱う）。
- フロント: `lib/api.ts` の `http<T>()` は **常に `Content-Type: application/json` ヘッダを送り**（`api.ts:29`）、**204 を `undefined` に畳む**（`api.ts:37`）既存挙動 → 一括既読（204）もそのまま扱える。`Article.is_read` は既に型に存在。

新たに作るのは「一括既読の SQL/サービス/ハンドラ/ルート1本」「`ReadAllBody` の serde 単体テスト1本」「フロントの配線・カウント表示・バッジ部品」だけ。

## 4. データモデルとマイグレーション

**DB 変更なし。**

理由: 既読状態は既存の `articles.is_read` で表現でき、一括更新に効く部分インデックス `idx_articles_is_read ... WHERE is_read=false` も既にある。未読件数は読み取り時に `COUNT(*) FILTER (WHERE is_read=false)` で算出（全体は既存 `/api/stats`、フィード別は feed-stats(03) が JOIN 集計）。新カラム・新テーブルは不要なため、土台設計の「03/09/01 の集計系は読み取り時計算でマイグレーション不要」に一致し、`0002〜0004` の番号は消費しない。

（将来の最適化メモ・本機能では実装しない）per-feed 一括既読が高頻度・大量記事になった場合、複合インデックス `(feed_id, is_read)` を新マイグレーションで追加する余地はある。単一ユーザ・家庭内 LAN 規模では不要と判断。

## 5. バックエンド設計

**方針: 既存 `articles` スライスを最小拡張する（新スライスは作らない）。** 正当化: `is_read` は articles アグリゲートが所有する列で、その一括書き込みは同一アグリゲート内の新規ユースケース。別スライスから `articles` を UPDATE するのは越境書き込みで土台設計の禁止事項に当たる。土台設計 §2.2 でも「articles 拡張（read-all）」として明示的に許可されている。新 trait/dyn は追加しない。`shared/error.rs` も編集しない。

変更は articles スライス内の4ファイルへの追記のみ。`features/mod.rs` は **不変**（`articles::routes()` の中身が増えるだけで `.merge()` 行は変わらない）。

### 5.1 repository.rs（追記）

```rust
/// is_read=false の記事を一括で既読にする。
/// feed_id=None なら全フィード、Some(id) ならそのフィードのみ。
/// 既に既読の行は対象外なので、戻り値（rows_affected）= 今回新たに既読化した件数。
pub async fn mark_all_read(pool: &PgPool, feed_id: Option<FeedId>) -> AppResult<u64> {
    let res = sqlx::query(
        r#"UPDATE articles
           SET is_read = true
           WHERE is_read = false
             AND ($1::uuid IS NULL OR feed_id = $1)"#,
    )
    .bind(feed_id.map(|f| f.0))
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}
```

- 実行時クエリ（`sqlx::query`）。`query!` は不使用（規約遵守）。
- `WHERE is_read = false` で既読行を除外 → **冪等**（2回目以降は 0 件）。部分インデックスが効く。
- `($1::uuid IS NULL OR feed_id = $1)` は既存 `list` と同じ NULL 分岐パターンを踏襲。
- 戻り値 `u64` は件数（テスト/将来のレスポンス拡張用）。存在しない `feed_id` を渡しても 0 件更新で正常終了（NotFound にしない＝バルク操作の冪等性を優先）。

### 5.2 service.rs（追記）

```rust
pub async fn mark_all_read(state: &AppState, feed_id: Option<FeedId>) -> AppResult<u64> {
    repository::mark_all_read(&state.db, feed_id).await
}
```

オーケストレーションは単純委譲（LLM 等の副作用なし）。

### 5.3 handler.rs（追記）

**抽出器は `Option<Json<ReadAllBody>>` を使う**（ここが本設計の要点）。axum 0.8 で `Json<T>` をそのまま使うと **リクエストボディと `Content-Type: application/json` ヘッダが必須**になり、ボディ無し POST は 415/400 を返してしまう。「ボディ省略可」という契約を**真**にするため、`Option<Json<T>>`（axum の `OptionalFromRequest`）を採用する。axum 0.8 の `OptionalFromRequest for Json<T>` は **Content-Type が `application/json` でない（＝ヘッダ無し含む）場合に `Ok(None)` を返し**、`application/json` のときだけボディをデシリアライズする（失敗時のみ拒否）。これにより「ボディ無し＝全体既読」が自然に成立する。

```rust
#[derive(Debug, Deserialize)]
pub struct ReadAllBody {
    #[serde(default)]
    pub feed_id: Option<Uuid>, // 省略 or null = 全フィード
}

pub async fn mark_all_read(
    State(state): State<AppState>,
    body: Option<Json<ReadAllBody>>,
) -> AppResult<StatusCode> {
    // body=None（ボディ無し or Content-Type が application/json でない）→ 全体既読。
    // body=Some(Json(b)) かつ b.feed_id=None（{} や {"feed_id":null}）→ 全体既読。
    let feed_id = body.and_then(|Json(b)| b.feed_id).map(FeedId);
    // 件数は現状クライアントへ返さない（§11 で {marked} 化の選択肢）。
    let _marked = service::mark_all_read(&state, feed_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

抽出器の振る舞い（axum 0.8、`Option<Json<ReadAllBody>>`、context7 で確認済み）:

| リクエスト | 抽出結果 → 動作 | HTTP |
|---|---|---|
| `Content-Type: application/json` + `{}` / `{"feed_id":null}` | `Some` → `feed_id=None` → 全体既読 | 204 |
| `Content-Type: application/json` + `{"feed_id":"<uuid>"}` | `Some` → 該当フィードのみ既読 | 204 |
| ボディ無し / Content-Type が `application/json` でない | `None` → 全体既読（ボディは無視） | 204 |
| `Content-Type: application/json` + 構文の壊れた JSON | `JsonSyntaxError` で拒否 | 400 |
| `Content-Type: application/json` + 構文は正しいが `feed_id` が UUID 文字列でない（例 `{"feed_id":"abc"}`, `{"feed_id":123}`） | `JsonDataError` で拒否 | 422 |

- 注意（既知の割り切り）: `Option<Json<T>>` は **Content-Type が `application/json` でないとボディを読まない**ため、JSON ボディを付けても content-type が違えば無視され「全体既読」になる。フロント `http<T>()` は常に `application/json` を送る（`api.ts:29`）のでこの罠は踏まない。結合テストの curl では `-H 'Content-Type: application/json'` を明示する（§9.2）。
- レスポンスは **204 No Content**。既存の単記事 `mark_read`（204）と一貫させ、土台設計 §2.2 の「read-all → 204」に従う。フロントは更新後に未読カウントを再取得する方針（§6.5）。
- 既存 import（`State`, `Json`, `StatusCode`, `Deserialize`, `Uuid`, `FeedId`）はファイル冒頭に揃っているので追加 import 不要。

### 5.4 mod.rs（ルート1行追記）

```rust
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/articles", get(handler::list))
        .route("/api/articles/read-all", post(handler::mark_all_read)) // ← 追加
        .route("/api/articles/{id}", get(handler::get_one))
        .route("/api/articles/{id}/read", post(handler::mark_read))
        .route("/api/articles/{id}/summarize", post(handler::summarize))
        .route("/api/articles/{id}/translate", post(handler::translate))
}
```

- **ルート衝突の正しい根拠**: `/api/articles/read-all`（静的セグメント `read-all`）と `/api/articles/{id}`（capture `{id}`）は **同じ位置に静的セグメントと capture が共存するケース**。axum 0.8 が同梱する **matchit 0.8 はこれを衝突とみなさず、静的セグメントを優先**する（公式ドキュメント: "The static route `/foo` and the dynamic route `/{key}` are not considered to overlap and `/foo` will take precedence."）。HTTP メソッドはパスマッチングに無関係（メソッドが違うから衝突しない、という理解は誤り）。`read-all` をハイフン入り literal にしておけば UUID パースとも混同しない。`.route()` の登録順序も結果に影響しない。
- `features/mod.rs` は **無編集**（`.merge(articles::routes())` のまま）。

### 5.5 AppError の使い分け

- 新バリアントは追加しない（`shared/error.rs` 不編集）。
- バルク操作は「該当 0 件でも成功」とするため `NotFound` を返さない。不正な body は §5.3 の表のとおり axum の `Json` 抽出が 400/422 を返す既存挙動に委ねる（明示の `Validation` は不要）。
- DB エラーは `?` 経由で `AppError::Database`（500、`{ "error": "internal error" }`）へ自動変換。

## 6. フロントエンド設計

本機能は「09 が所有する部分」と「他機能の器を消費する部分」を分けて配線する。器（Sidebar・グローバルストア・feed-stats）が未着手でも、**バックエンド一括既読・自動既読・全体未読バッジ（/api/stats 由来）は単独で動く**ように段階化する。

### 6.1 lib/api.ts（09 が所有・追記）

> **クロス土台の整合（重要）**: バルク既読のパスは **`/api/articles/read-all` を正**とする（バックエンド土台 §2.2/§6 準拠）。フロントエンド土台 §0/§4.4/§4.5 が記す `POST /api/articles/mark-read` というパスは **本設計で破棄（supersede）** する。メソッド名は既存規約に合わせ `markAllRead` のままだが、**POST 先は必ず `/api/articles/read-all`**。10/11 の実装者がフロント土台の古い `mark-read` パスを配線して 404 にしないよう注意。

```ts
// 一括既読。feed_id 省略 = 全体。204 を http<void> が undefined に畳む。
// 送信先は /api/articles/read-all（mark-read ではない）。
markAllRead: (params?: { feed_id?: string }) =>
  http<void>("/api/articles/read-all", {
    method: "POST",
    body: JSON.stringify({ feed_id: params?.feed_id ?? null }),
  }),
```

- 命名は既存規約「動詞+リソース」（`markRead` と並ぶ `markAllRead`）に合わせる。`http<T>()` は常に `application/json` を送るのでバックエンドの `Option<Json>` は `Some` で受ける。
- 自動既読は **新メソッド不要**：既存 `api.markRead(id, true)` を再利用。
- 全体未読数は既存の `GET /api/stats` を使う。stats 用の薄い型/メソッドが未定義なら最小限追加（任意）:
  ```ts
  export interface Stats { feeds: number; articles: number; unread: number; }
  getStats: () => http<Stats>("/api/stats"),
  ```
- フィード別未読数は feed-stats(03) が提供する集計を消費（本機能では定義しない。§8 に契約を固定）。

### 6.2 自動既読: routes/ArticleView.tsx（09 が所有・改修）

`ArticleView` は既に `createResource(() => params.id, api.getArticle)` で記事を取得し `mutate` を分割代入している（現コードの `const [article, { mutate }] = ...`）。ロード完了かつ未読なら一度だけ既読化する `createEffect` を足す。

```tsx
import { createEffect, createResource, createSignal, Show } from "solid-js";
// ...
const [article, { mutate }] = createResource(() => params.id, api.getArticle);

// 二重 POST 防止ガード。意図的に「非リアクティブ」なローカル束縛にしている
// （signal にしない）。:id だけ変わってコンポーネントが再マウントされない
// ルーター挙動のもとで「この記事は既に既読化送信済み」を覚えるだけの用途。
// signal 化すると createEffect の依存に入り、無限ループ/二重 POST を招くので変えないこと。
let lastMarkedId: string | undefined;

createEffect(() => {
  const a = article();
  if (a && !a.is_read && lastMarkedId !== a.id) {
    lastMarkedId = a.id;
    api
      .markRead(a.id, true)
      .then(() => {
        mutate((prev) => (prev ? { ...prev, is_read: true } : prev)); // 楽観更新
        // 未読カウント整合（ストアがあれば。§6.5）
        // useApp()?.counts.refresh();
      })
      .catch((e) => console.error("auto mark-read failed", e));
  }
});
```

- `lastMarkedId` で記事間遷移（`:id` だけ変わる）でも一度だけ送信。失敗してもユーザー操作は止めない（コンソールログのみ。既読は致命的でない）。
- `mutate` でローカルの `is_read` を即 true にし、戻ったときの一覧表示と整合（一覧は再フェッチでも反映、§6.5）。

### 6.3 一括既読ボタン（09 が所有・配置先は器に依存）

2つのスコープを用意する:
- **全体既読**「すべて既読にする」: `api.markAllRead()`（feed_id なし）。
- **フィード既読**「このフィードを既読」: `api.markAllRead({ feed_id })`。

配置先（器が無い段階でも動くフォールバックを併記）:
- 二ペイン（10）導入後: 全体ボタンは `components/layout/Sidebar.tsx` 上部、フィード別は Sidebar の各フィード行の `dropdown-menu`（Ark UI Menu、01 と共有）の項目「既読にする」。
- 10 が未導入の段階のフォールバック: 既存 `routes/FeedList.tsx`（実体は記事一覧。10/08 で `ArticleList.tsx` に改名予定）のヘッダに「すべて既読」ボタンを1つ置くだけでも要件を満たす。

クリックハンドラ共通形:

```tsx
const [marking, setMarking] = createSignal(false);
const markAll = async (feed_id?: string) => {
  setMarking(true);
  try {
    await api.markAllRead(feed_id ? { feed_id } : undefined);
    await refetchArticles();        // 一覧の is_read を反映（ローカル resource）
    // useApp()?.counts.refresh();   // 未読カウント再取得（§6.5）
  } catch (e) {
    alert(`既読化に失敗しました: ${String(e)}`);
  } finally {
    setMarking(false);
  }
};
```

確認ダイアログ（任意・推奨）: 全体既読は破壊的に見えるので、既存 `components/ui/dialog.tsx`（Ark UI Dialog ラップ）で「全 N 件を既読にします」を確認してから実行してもよい。MVP では省略可。

### 6.4 未読件数の表示（09 が所有・データ源は段階的）

- **全体未読バッジ**（独立して動く）: `GET /api/stats` の `unread`。サイドバー上部 or ヘッダに `badge` で表示。
- **フィード別未読バッジ**（feed-stats(03) 依存）: 03 の per-feed 集計（`unread_count`、§8 で契約固定）を `feed_id` で突合し、Sidebar の各フィード行右端に `badge`。`unread_count === 0` の行はバッジ非表示。
- バッジ部品: `components/ui/badge.tsx` を **自前 Tailwind + cva** で新設（土台設計 §3 が badge を自前 cva と規定）。oklch トークンのみ使用。例:
  ```tsx
  // components/ui/badge.tsx
  import { cva, type VariantProps } from "class-variance-authority";
  import { cn } from "@/lib/utils";
  const badge = cva(
    "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium tabular-nums",
    {
      variants: {
        variant: {
          default: "bg-muted text-muted-foreground",
          unread: "bg-accent text-accent-foreground", // 未読あり強調
        },
      },
      defaultVariants: { variant: "default" },
    },
  );
  export function Badge(props: { class?: string; variant?: "default" | "unread"; children: any }) {
    return <span class={cn(badge({ variant: props.variant }), props.class)}>{props.children}</span>;
  }
  ```
  （01 が先に badge を導入していれば重複作成せず再利用。`VariantProps` は variant 型を共有したいときのみ import。）

### 6.5 状態管理・カウント整合

- 未読カウントは土台設計 §2.5 の **グローバル `counts` リソース + `refresh()`**（`lib/store.tsx` の `useApp().counts`、内部 `createResource(() => api.listFeedStats())`）を消費する。自動既読・一括既読の **直後に `refresh()`** を呼ぶ（増分デクリメントは初期不要、refetch で十分）。
- ストア/feed-stats が未着手の段階のフォールバック: 各ルート（`FeedList`/`ArticleView`）が **ローカル `createResource(() => api.getStats())`** を持ち、操作後に `refetch()` する。全体未読のみだが要件「未読件数の表示」を満たす。器が入ったら `useApp().counts` に差し替え（局所変更）。
- 一覧の既読反映: 一括既読後は一覧 resource を `refetch()`。自動既読後に一覧へ戻るケースは、戻り遷移で一覧ルートが再評価され `refetch` される（or counts.refresh と同時に一覧 source を無効化）。

### 6.6 必要な Ark UI 部品

- 本機能が **新規に必須化する Ark UI 部品はない**。`badge` は自前 cva。確認ダイアログを使うなら既存 `dialog.tsx`（Ark UI Dialog ラップ済み）を再利用。
- フィード行の「既読にする」を `dropdown-menu`（Ark UI Menu）項目として出すのは 01 と共有の器。**Ark UI v5 の Menu の part 名（`Menu.Root/Trigger/Positioner/Content/Item` 想定）は実装時に ark-ui.com（Solid）で要確認**（土台設計の方針どおり、断定しない）。器が無ければ素のボタンで代替可。

## 7. API 契約

### 追加: 一括既読

```
POST /api/articles/read-all

# 全体既読（いずれも 204。前2者は Content-Type: application/json を付ける）
{}                      | { "feed_id": null }   | （ボディ自体を省略：Content-Type 不問）

# フィード単位（Content-Type: application/json）
{ "feed_id": "0f9a...uuid" }

→ 204 No Content   （ボディなし。冪等。該当0件でも204）
```

ステータス対応（§5.3 の抽出器表と一致）:

| 条件 | ステータス |
|---|---|
| `Content-Type: application/json` + 有効ボディ（`{}` / `{"feed_id":null}` / `{"feed_id":"<uuid>"}`） | 204 |
| ボディ無し / Content-Type が `application/json` でない | 204（`Option<Json>` → None → 全体既読、ボディは無視） |
| `Content-Type: application/json` + 構文の壊れた JSON | **400**（JsonSyntaxError） |
| `Content-Type: application/json` + 正しい JSON だが `feed_id` が UUID 文字列でない | **422**（JsonDataError） |
| DB 障害 | 500 `{ "error": "internal error" }`（`AppError::Database`） |

> 注: ステータスはすべて `Json` 抽出器（400/422）と `AppError`（500）の既存挙動に由来する。本機能で新たな分岐は書かない。`Json<T>`（非 Option）を採れば「Content-Type 欠落 → 415」だが、本設計は `Option<Json<T>>` を採るため 415 は発生せず None 扱いになる（§5.3 の理由）。

### 再利用（変更なし・本機能が叩く）

```
# 自動既読（記事を開いたとき）
POST /api/articles/{id}/read   { "read": true }   → 204

# 全体未読数（カウント表示の独立データ源）
GET /api/stats                 → 200 { "feeds": 3, "articles": 120, "unread": 8 }
```

### 依存（feed-stats(03) が提供・本機能は消費のみ）

```
GET /api/feeds/overview  （命名・スライス名は 03 に従う。§8 の固定契約を参照）
→ 200 [ { "feed_id": "<uuid>", "unread_count": 5, "total_count": 40, ... }, ... ]
```

## 8. 依存関係

依存する機能（dependsOn）:
- **feed-stats (03)**: フィード別未読数バッジのデータ源。これが無い間は **全体未読バッジ（/api/stats）のみで成立**し、フィード別バッジは後付け。
  - **本機能が前提とする固定契約（03 の実装者はこれを守ること）**: per-feed 集計の各要素は **`feed_id`（UUID 文字列）をキーに、未読件数フィールド名を `unread_count`（整数 / Rust 側 `i64`）** とする。エンドポイント名やスライス名（バックエンド土台では `feed_overview` / `GET /api/feeds/overview` / 型 `FeedOverview`、フロント土台では `feed_stats` / `GET /api/feeds/stats` / 型 `FeedStat`）は 03 の決定に委ねるが、**フィールド名 `unread_count` と突合キー `feed_id` は固定**。03 が `unread` や `count` といった別名で出すと per-feed バッジが黙って壊れるため、ここで明示的に釘を刺す。
- **two-pane-layout (10)**: 一括既読ボタンと未読バッジの主たる置き場所（Sidebar）とグローバル `counts` ストア。未導入なら既存一覧ヘッダ＋ローカル stats でフォールバック可能。

ソフトな統合点（ハード依存ではない）:
- **feed-management (01)**: フィード行の `dropdown-menu` に「既読にする」項目を相乗り。無ければ素ボタンで代替。`badge.tsx` は 01 と共有（先に作った側を再利用）。
- **feed-add-placement (08) / ルート改名**: `FeedList.tsx → ArticleList.tsx` 改名後はそちらに配線（パスのみ変更）。

本機能がブロック/前提を提供する先:
- **unread-filter-toggle (11)**: 「すべて/未読」トグルは本機能のカウント表示・既読整合（refresh）の上に乗る。11 は 09 に依存。

**独立して先行リリース可能な核**: バックエンド `read-all` エンドポイント＋`ArticleView` 自動既読＋全体未読バッジ（/api/stats）。これらは 03/10 無しで動く。

## 9. テスト計画（TDD）

前提（重要）: バックエンド crate は **binary のみで lib ターゲットを持たない**（`src/main.rs` のみ、`lib.rs` なし）。そのため `backend/tests/` の外部結合テストから `features::articles::repository::mark_all_read` 等の内部関数を `use` できない。これが stats スライスで「Rust の `backend/tests/` ではなく `scripts/test/api-stats.sh`（稼働スタックへ curl）」が採用された理由。**本機能も同じ結合テスト方式に倣う**（lib 化はこのスライスのスコープ外。§11）。ただし **純粋ロジック（`ReadAllBody` の serde）は crate 内 `#[cfg(test)]` で `cargo test`（= `just test`）から実行できる**ので、唯一の in-crate テストとして必ず追加する。

### 9.1 単体テスト（`#[cfg(test)]`、`handler.rs` 内・必須）

本機能で唯一の純粋ロジック Red→Green。`ReadAllBody` の serde デフォルトを検証する。**先に Red（`#[serde(default)]` を付けない状態だと `{}` のデコードが失敗）→ `#[serde(default)]` を付けて Green**。

```rust
#[cfg(test)]
mod tests {
    use super::ReadAllBody;

    #[test]
    fn read_all_body_defaults_to_none_when_absent() {
        // {} と {"feed_id":null} はどちらも feed_id=None（= 全体既読）にデコードされる。
        let a: ReadAllBody = serde_json::from_str("{}").unwrap();
        assert!(a.feed_id.is_none());
        let b: ReadAllBody = serde_json::from_str(r#"{"feed_id":null}"#).unwrap();
        assert!(b.feed_id.is_none());
    }

    #[test]
    fn read_all_body_parses_uuid() {
        let s = r#"{"feed_id":"00000000-0000-0000-0000-000000000001"}"#;
        let parsed: ReadAllBody = serde_json::from_str(s).unwrap();
        assert!(parsed.feed_id.is_some());
    }

    #[test]
    fn read_all_body_rejects_non_uuid() {
        // 構文は正しいが UUID でない → デコード失敗（実機では axum が 422 に変換）。
        assert!(serde_json::from_str::<ReadAllBody>(r#"{"feed_id":"abc"}"#).is_err());
    }
}
```

意図: `Option<Uuid>` + `#[serde(default)]` の契約（§5.3 の抽出器表の Some 行）がコードで担保されることを保証する。`just test` で走る。

### 9.2 結合テスト（`scripts/test/api-articles-read-all.sh`、`api-stats.sh` に倣う）

稼働スタック（nginx :8081）に対して実行。**Red 先行**: スクリプトを先に書くと現状エンドポイント不在で 404 → 実装後 204 で PASS。

**破壊性の警告（必読・スクリプト冒頭にも明記する）**: 本テストは api-stats.sh（GET 専用）と違い **記事の is_read を書き換える**。とくにケース#4（`feed_id` 無しの全体既読）は **その DB の全未読をゼロにする**。**本番（ユーザーが普段読んでいる）DB に対して実行しないこと。** 安全策として次のいずれかを採る:
- (推奨) **使い捨て DB に向ける**。`just dev-db` で別 DB を立てるか、compose を専用 DB 名で起動し、`API_BASE` をそのスタックへ向ける。
- 直 DB シードには **`docker compose exec -T db psql -U "$POSTGRES_USER" -d "$POSTGRES_DB"`** を使う（compose の DB はホストにポート非公開のことがあるため、ホスト `psql` ではなくコンテナ内 `psql` を使うのが確実。DB 名は既定 `rssreader`）。

**順序非依存にする（重要）**: 各ケースの直前に **既知 UUID の sentinel feed/article を再シード**する。ケース#4 が全未読を消すため、ケース間でシードを使い回すと後続が壊れる。各ケースで「自分が使う行を fresh に INSERT（`ON CONFLICT (url) DO UPDATE SET is_read=false` で未読へリセット）」してから API を叩く。sentinel データは本番由来の記事と URL が衝突しないよう、専用フィード（例 `https://example.test/feed-a`）の配下に置く。

テスト一覧（意図）:
1. **エンドポイント存在・204**: 明示的に
   ```bash
   curl -s -m5 -w '\n%{http_code}' -X POST \
     -H 'Content-Type: application/json' -d '{}' \
     "http://localhost:8081/api/articles/read-all"
   ```
   が **204** を返す。意図: ルート結線とハンドラの基本疎通（Red 時は 404）。`-H 'Content-Type: application/json'` を必ず付け、`-d '{}'` が `application/x-www-form-urlencoded` 既定にならないようにする（content-type を外しても `Option<Json>` 仕様上 204 にはなるが、テストは契約どおりの本筋パスを通す）。
2. **フィード単位スコープ（順序非依存・主要ケース）**: sentinel フィード A・B を再シードし、各々に未読記事を2件ずつ INSERT → `read-all {"feed_id":"<A>"}` → 204 → `GET /api/articles?feed_id=<A>&unread=true` が空、`GET /api/articles?feed_id=<B>&unread=true` が非空。意図: `WHERE feed_id=$1` の絞り込みと、他フィードを巻き込まないこと。
3. **冪等性**: 2 の直後にもう一度 `read-all {"feed_id":"<A>"}` → 204（エラーにならず、A は未読0のまま）。意図: `WHERE is_read=false` による再実行安全性。
4. **全体既読（破壊的・必ず最後・使い捨て DB のみ）**: A・B を再シード（再び未読化）→ `read-all {}` → `GET /api/stats` の `unread === 0`。意図: 全体一括既読の効果。**この1ケースだけグローバル状態に依存する**ため最後に置き、本番 DB では skip 可能にする（環境変数 `RUN_DESTRUCTIVE=1` のときのみ実行、等のガードを付ける）。

実装メモ: `api-stats.sh` と同じく `set -uo pipefail` + `curl -w '\n%{http_code}'` で HTTP コードを取り出し、JSON は `grep` でキー/値を検証する形に揃える。スクリプトは `scripts/test/` に置き `chmod +x`。シードは `docker compose exec -T db psql ...` のヒアドキュメントで INSERT。

代替（より厳密だが本機能ではコスト高と判断）: `backend` に `lib.rs` を追加して `repository::mark_all_read(pool, feed_id)` を `backend/tests/` から直接呼ぶ Rust 結合テスト（テスト DB に seed → 戻り `u64` を assert）。lib ターゲット新設は他スライスへ波及するため **本スライスのスコープ外**。採否は §11。

### 9.3 フロント（手動 + 型）

- `pnpm` / `just lint`（tsc）で `markAllRead` / `Stats` 型・呼び出しの型整合を確認。
- 手動: (a) 未読記事を開く→一覧に戻ると既読表示（薄字）になり、未読バッジが1減る。(b)「すべて既読」→一覧の全記事が既読表示・全体バッジ 0。(c) フィード別「既読にする」→当該フィードのみ 0、他は不変。(d) 既読化 API を一時的に 500 にしても UI がクラッシュしない（自動既読はログのみ、一括はアラート）。

## 10. 実装手順（順序付きチェックリスト）

バックエンド（articles スライス内に閉じる）:
1. `backend/src/features/articles/repository.rs` に `mark_all_read(pool, Option<FeedId>) -> AppResult<u64>` を追記（§5.1 の SQL）。
2. `backend/src/features/articles/service.rs` に委譲 `mark_all_read(state, Option<FeedId>) -> AppResult<u64>` を追記。
3. （TDD・in-crate）`backend/src/features/articles/handler.rs` に `ReadAllBody` を追記し、**先に §9.1 の `#[cfg(test)] mod tests` を書いて `just test` で Red→Green** を確認。続けて `mark_all_read` ハンドラ（`Option<Json<ReadAllBody>>` 抽出器・204）を追記。
4. `backend/src/features/articles/mod.rs` の `routes()` に `.route("/api/articles/read-all", post(handler::mark_all_read))` を追加（`{id}` ルートとの前後は不問。matchit が静的優先で衝突なし）。
5. （TDD・結合）`scripts/test/api-articles-read-all.sh` を **先に**書く（§9.2 の1〜4、破壊性ガードと再シード込み）。`chmod +x`。使い捨て or dev スタックを `just up` 等で起動し 404 を確認（Red）。
6. `cargo fmt` → `just lint`（clippy `-D warnings`）→ ビルド。スタック再起動して結合スクリプト PASS（Green）。

フロント:
7. `frontend/src/lib/api.ts` に `markAllRead(...)`（POST 先 `/api/articles/read-all`）を追記。必要なら `Stats` 型 + `getStats()` も追記。
8. `frontend/src/routes/ArticleView.tsx` に自動既読 `createEffect`（§6.2、`lastMarkedId` は非リアクティブのままにするコメント込み）を追加。`mutate` で楽観更新。
9. 未読カウント表示: `components/ui/badge.tsx` を新設（無ければ）。全体未読バッジを表示（データ源は `getStats()`、または導入済みなら `useApp().counts`）。
10. 一括既読ボタンを配線（§6.3）。10 導入済みなら Sidebar、未導入なら一覧ヘッダにフォールバック配置。操作後に一覧 `refetch()` ＋カウント `refresh()/refetch()`。
11. （feed-stats(03) 導入後）Sidebar 各フィード行に `unread_count` バッジ、フィード別「既読にする」を配線（突合キー `feed_id` / フィールド `unread_count`、§8）。
12. `just lint`（tsc/prettier）→ §9.3 の手動確認。

## 11. リスク・未決事項・代替案

- **抽出器の選択（決定済み・`Option<Json<ReadAllBody>>`）**: 「ボディ省略可」を真にするため非 Option の `Json<T>` ではなく `Option<Json<T>>` を採用した。代償として「JSON ボディを付けたのに content-type が `application/json` でない」場合はボディが無視され全体既読になる（§5.3 の罠）。フロントは常に正しい content-type を送るため実害なし。厳密に content-type 必須にしたいなら `Json<ReadAllBody>` へ戻し §7 の表を「ボディ＋content-type 必須、欠落は 415」に置換する（その場合「ボディ省略可」の文言も撤回）。
- **結合テストの方式（要決定）**: 体裁ルール/土台設計は `backend/tests/` を推奨するが、当 crate は lib ターゲットを持たず内部関数を import できない実情がある（stats も shell スクリプトを採用）。本書は **既存前例どおり shell スクリプト**を主とし、唯一の純粋ロジック（serde）だけ in-crate `#[cfg(test)]` に置いた。より厳密にしたい場合の代替=「`backend` に `lib.rs` を導入して Rust 結合テスト化」だが、他スライスへ波及するため別タスク化を推奨。実装者はどちらかをチームと合意のこと。
- **結合テストの破壊性（要注意）**: read-all は実 DB の is_read を変える。とくに全体既読ケースは全未読を消す。§9.2 のとおり **使い捨て DB に向け、再シードで順序非依存にし、全体ケースは `RUN_DESTRUCTIVE=1` ガード**で本番 DB を守ること。
- **レスポンス形（204 vs `{marked}`）**: 本書は既存 `mark_read` と一貫して 204。`repository`/`service` は既に `u64` 件数を返すので、「N 件を既読にしました」トースト等が要件化したらハンドラを `Json(serde_json::json!({ "marked": n }))` + 200 に変えるだけで対応可能（小改修）。現状ハンドラは `let _marked` で破棄しているが、これは将来拡張の余地を残す意図的なもの。UI 要件が固まり次第判断（保留）。
- **フォルダ単位の一括既読（非スコープ）**: `feeds.folder_id`（feed-folders 02 / migration 0002）に依存。02 導入後の拡張案は2つ — (a) フロントがフォルダ配下フィードを列挙して `markAllRead({feed_id})` を順次呼ぶ（バックエンド不変）、(b) `read-all` の body に `folder_id?` を足し、SQL を `feed_id IN (SELECT id FROM feeds WHERE folder_id=$2)` に拡張。(a) を MVP 推奨、(b) は 02 依存を明示して将来追記。
- **自動既読の取り消し不能感**: 開いただけで既読化されるため、誤タップで未読を失う懸念。緩和=記事ビューに「未読に戻す」（既存 `markRead(id,false)`）を1ボタン用意。将来「自動既読 ON/OFF 設定」を入れるならクライアント設定（localStorage、04 のテーマと同方式、DB 不要）で十分。
- **未読カウントの一時的不整合**: refetch ベースのため、楽観更新とサーバ再取得の間に一瞬ズレる可能性。単一ユーザ・LAN では許容。気になれば `counts` を `createStore` 化して増分デクリメントに差し替え（土台設計 §2.5 が示す将来拡張）。
- **feed-stats(03) のフィールド名**: §8 で `unread_count` / `feed_id` を固定したが、03 が別名で実装するとフィード別バッジが黙って壊れる。03 のレビュー時にここを突き合わせること。
- **Ark UI Menu の API**: フィード行ドロップダウンに相乗りする場合、v5 の part 名/props は **実装時に ark-ui.com（Solid）で要確認**（断定しない）。器が無ければ素ボタンで代替し依存を消せる。
- **大量記事時の UPDATE コスト**: 全体既読は最悪 `articles` 全行スキャンだが、`WHERE is_read=false` 部分インデックスで対象は未読のみに絞られる。単一ユーザ規模で問題なしと判断。逼迫したら複合インデックス `(feed_id, is_read)` を新マイグレーションで追加。
