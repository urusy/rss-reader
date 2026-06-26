# 00 基盤設計 — バックエンド横断方針（11機能の土台）

このドキュメントは、11 機能すべてが矛盾なく載るための「土台」を定義する。各機能の設計者は
**着手前に本書を読み、ここで確定した割り当て（マイグレーション番号・スライス境界・命名・状態の置き場）に従うこと。**
逸脱する場合は本書の該当節を参照し理由を述べる。

対象コミット時点の事実（裏取り済み）:

- 既存スライス: `health` / `feeds` / `articles` / `stats`。`features/mod.rs` で `.merge()` 合成。
- `feeds` テーブルに `title`（rename 用に既存）あり。`articles` に `is_read` + 部分インデックスあり、`repository::list` は `feed_id` と `unread_only` の両フィルタ対応済み、`set_read` 済み。summary/translation の LLM キャッシュ済み。
- マイグレーション最新は `0001_init.sql`。append-only。
- 抽象境界（trait）は `shared/llm` のみ。`AppError` は NotFound/Validation/NotEnabled/Upstream/Database/Other。

---

## 0. 結論サマリ（TL;DR）

- **新規マイグレーションは 3 本だけ**: `0002_folders.sql`（folders + `feeds.folder_id`）, `0003_instapaper.sql`（資格情報 singleton）, `0004_read_later.sql`（保存状態トラッキング）。03/09/01 の集計系は**読み取り時計算でマイグレーション不要**。
- **新スライスは 3 枚**: `folders`（CRUD）, `instapaper`（資格情報 + Instapaper add + 後で読む）, `feed_overview`（読み取り専用 read model: フィード別未読数・最終投稿・投稿頻度）。
- **既存スライス拡張は 2 枚に限定**し、いずれも「同一アグリゲートの書き込み操作だから」という明確な理由で許可: `feeds`（PATCH によるリネーム/フォルダ割当 + per-feed refresh）, `articles`（一括既読）。
- **trait は増やさない**。Instapaper は 2 つ目のプロバイダ予定がないので reqwest 直叩き（`anthropic.rs` の手法だが trait なし）。
- **`AppError` は不編集**。既存 6 バリアントで全機能を表現する（新バリアントを足さない）。
- **クライアント側のみで持つ状態**（DB に持たない）: テーマ, すべて/未読トグル, 選択中フォルダ/フィード/記事, ツリー展開状態。単一ユーザのため。
- **04/07/08/10/11 はバックエンド変更ゼロ**（フロント作業 + 既存 API 再利用）。

---

## 1. マイグレーション割り当て計画（append-only / 0001 不編集）

| No | ファイル | 主対象機能 | 内容 |
|----|----------|-----------|------|
| 0002 | `0002_folders.sql` | 02, 01 | `folders` テーブル新設 + `feeds.folder_id`（nullable FK, ON DELETE SET NULL）追加 + index |
| 0003 | `0003_instapaper.sql` | 05 | `instapaper_credentials`（singleton 1 行）新設 |
| 0004 | `0004_read_later.sql` | 06 | `read_later_items`（記事ごとの保存/送信状態）新設 |

**番号衝突の回避ルール**: 本書が `0002`〜`0004` を予約する。並行開発で別機能が新カラムを足す場合は、**マージ時に空いている次の整数へリベース**し、本表を更新してから取り込む。番号は「ファイルの先頭整数 = 適用順」。

機能 03（投稿頻度）・09（一括既読/未読数）・01（フィード別未読数）は **新カラムを足さない**。理由は §3 を参照（読み取り時集計で足りる）。

### 0002_folders.sql（スケッチ）

```sql
-- フォルダ(カテゴリ)。単一ユーザなのでフラット1階層で十分。
CREATE TABLE IF NOT EXISTS folders (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL,
    position    INTEGER NOT NULL DEFAULT 0,   -- 左ペインの並び順
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 既存 feeds への列追加は「新マイグレーションでの ALTER」が正規手順。
ALTER TABLE feeds
    ADD COLUMN IF NOT EXISTS folder_id UUID
    REFERENCES folders(id) ON DELETE SET NULL;     -- フォルダ削除→フィードは未分類へ

CREATE INDEX IF NOT EXISTS idx_feeds_folder_id ON feeds(folder_id);
```

- `ON DELETE SET NULL` が「未分類（uncategorized）」の定義を支える（§3）。`CASCADE` にしない（フォルダ削除でフィード/記事を消さない）。
- `name` の UNIQUE は付けない（リネーム時の取り回しを単純化）。重複名は UI 側の判断に委ねる。
- `position` は任意。MVP は `created_at` 順でも可だが、将来の並べ替えに備え列だけ用意。

### 0003_instapaper.sql（スケッチ）

```sql
-- 資格情報はサーバ側保管(単一ユーザ)。singleton: id は常に 1。
CREATE TABLE IF NOT EXISTS instapaper_credentials (
    id          INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    username    TEXT NOT NULL,           -- Instapaper のメール/ユーザ名
    password    TEXT NOT NULL,           -- Simple API は Basic 認証(留意 §7)
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

- なぜ env でなく DB か: Anthropic キーは**デプロイ時設定**（`AppConfig`/env）。Instapaper 資格情報は**ユーザが実行時に UI から設定**する想定なので DB。両者とも未設定時は `AppError::NotEnabled`（同じ NotEnabled パターン）。
- `CHECK (id = 1)` で 1 行に固定（UPSERT で更新）。GET では password を**返さない**（`{ configured: bool }` のみ）。

### 0004_read_later.sql（スケッチ）

```sql
-- 「後で読む」のローカル状態。記事1件につき1行(重複追加を冪等に)。
CREATE TABLE IF NOT EXISTS read_later_items (
    article_id           UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    status               TEXT NOT NULL DEFAULT 'pending',  -- pending|added|failed
    instapaper_added_at  TIMESTAMPTZ,
    last_error           TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

- **`articles` に列を足さず別テーブル**にする理由: 保存状態は `instapaper` スライスの関心。`articles` テーブルに `saved_at` を足すと「articles スライスが read-later の都合を知る」越境になる。`article_id` を PK にして重複追加を冪等化、`status`/`last_error` で失敗 UX を表現。
- `ON DELETE CASCADE` で記事削除時に自動清掃。

---

## 2. 新スライス一覧と境界

### 2.1 新設スライス（3 枚）

| スライス | ルート | 役割 | 主対象 |
|---------|--------|------|--------|
| `folders` | `GET /api/folders`, `POST /api/folders`, `PATCH /api/folders/{id}`, `DELETE /api/folders/{id}` | フォルダ CRUD。`domain` に `FolderId`(newtype) と `FolderName::parse`(値オブジェクト) | 02 |
| `instapaper` | `PUT /api/instapaper/credentials`, `GET /api/instapaper/status`, `POST /api/read-later`, `GET /api/read-later`, （任意 `DELETE /api/read-later/{article_id}`） | 資格情報保存 + Instapaper `/api/add` 呼び出し + 後で読む状態管理 | 05, 06 |
| `feed_overview` | `GET /api/feeds/overview` | **読み取り専用 read model**。フィード別に未読数・総記事数・最終投稿日時・投稿頻度を JOIN 集計して返す | 01, 03, 09, 02, 11 |

**`instapaper` に 06 を同居させる判断**: 「後で読む」のバックエンドは Instapaper だけであり、`read-later` を別スライスにすると read-later → instapaper の**越境呼び出し**が常時発生する。資格情報・add 呼び出し・保存状態は 1 アグリゲートとして強く凝集しているため、**1 スライスに収める**。06 の設計者はこのスライスに `handler`/`service`/`repository` を追記する（新スライスではなく、05 が作ったスライスへの追記）。これが「2 機能が 1 スライスを共有する」唯一の例外で、上記の理由で正当化する。

**`feed_overview` が他スライスのテーブルを読むことについて**: これは禁止される「越境共通レイヤー」ではない。共有された可変サービス層を作って書き込みパスを結合するのが禁止対象であって、`feed_overview` は**自前の SQL を発行するだけの独立した読み取りスライス**（CQRS-lite の read model）。既存 `stats` スライス（feeds/articles を横断集計する読み取り専用）と同じ前例に従う。書き込みは一切しない。

ルート設計の注意: `feed_overview` は `/api/feeds/overview` に登録するが、`feeds` スライスの `/api/feeds/{id}` とは衝突しない（axum 0.8 matchit は静的セグメント `overview` を動的 `{id}` より優先）。複数スライスが同一プレフィックス `/api/feeds` 配下にルートを `.merge()` で足すのは結合ではない（各スライスは独立 `Router<AppState>` を返すだけ）。

### 2.2 既存スライス拡張（2 枚・要正当化）

| スライス | 追加 | 正当化 |
|---------|------|--------|
| `feeds` | `PATCH /api/feeds/{id}`（`{ title?, folder_id? }` の部分更新＝リネーム + フォルダ割当）／ `POST /api/feeds/{id}/refresh` を**当該フィードのみ**の再取得に修正 | リネームもフォルダ割当も **Feed アグリゲートの書き込み**。別スライスから `feeds.title`/`feeds.folder_id` を UPDATE すると同一テーブルへの越境書き込みになり、新スライスより悪化する。`feeds/repository.rs` に `update`、`handler` に `update`（`UpdateFeed` ボディ）を追記する形で閉じる。**この `PATCH /api/feeds/{id}` エンドポイント・`handler::update`・`UpdateFeed` 構造体・`repository::update` は #02 feed-folders が唯一の定義所有者（single source of truth）**。契約は `{ title?, folder_id? }`（キー無し=据え置き / `folder_id: null`=未分類化 / 値=割当）。#01 feed-management は**リネーム UI からこのエンドポイントを再利用するだけで、ルート/ハンドラ/構造体を再定義しない**（重複定義によるコンパイルエラー回避）。 |
| `articles` | `POST /api/articles/read-all`（`{ feed_id?: uuid }`、null=全体一括既読）→ 204 | `is_read` は articles スライス所有列。一括既読は **Article への新しい書き込み操作**であり同一アグリゲート内。別スライス化は `is_read` への越境書き込みになる。記事を開いた時の自動既読は**既存 `POST /api/articles/{id}/read` を再利用**（バックエンド変更なし）。 |

### 2.3 trait 方針の確認

- **新しい trait / dyn は追加しない。** Instapaper には 2 つ目の実装予定がないので、`instapaper/service.rs` で `state.http`（reqwest）を直接使う。`shared/llm/anthropic.rs` と同じ「外部 HTTP を reqwest で叩く」手法を踏襲するが、`LlmClient` のような port trait は作らない。
- 抽象境界は引き続き `shared/llm` のみ。リポジトリ・フィードパーサ・Instapaper クライアントは具体実装で固定。

### 2.4 `features/mod.rs` への合成（追加行のみ）

```rust
// pub mod に追加
pub mod folders;
pub mod instapaper;
pub mod feed_overview;

// router() の .merge チェーンに追加
    .merge(folders::routes())
    .merge(instapaper::routes())
    .merge(feed_overview::routes())
```

`feeds` / `articles` の拡張はルート定義（各スライスの `mod.rs::routes()`）に行を足すだけで、`features/mod.rs` の合成は不変。

---

## 3. 横断データモデル

### 3.1 関係

```
folders (1) ──< feeds (folder_id, nullable) (N)
feeds   (1) ──< articles (feed_id, NOT NULL, ON DELETE CASCADE) (N)
articles(1) ──  read_later_items (article_id PK, ON DELETE CASCADE) (0..1)
instapaper_credentials : singleton (0..1 行)
```

### 3.2 未分類（uncategorized）の扱い

- 未分類 = `feeds.folder_id IS NULL`。**DB に「未分類」という行は作らない**（特別 UUID も作らない）。
- フロントは左ペインで `folder_id === null` のフィード群を仮想グループ「未分類」として描画（並びは末尾固定を推奨）。フォルダ削除時は FK の `ON DELETE SET NULL` により配下フィードが自動的に未分類へ落ちる。
- API は `folder_id: string | null` をそのまま返し、グルーピングはクライアントが行う。

### 3.3 フィード別未読数・フォルダ別未読数

- フィード別未読数は `feed_overview` の read model で算出: `COUNT(a.id) FILTER (WHERE a.is_read = false)`。
- フォルダ別未読数は **配下フィードの未読数の総和**。MVP はフロントで `folder_id` ごとに合算（バックエンドにフォルダ集計エンドポイントを別途足さない）。
- 既存のグローバル `GET /api/stats`（feeds/articles/unread 合計）はそのまま。`feed_overview` はその「フィード粒度版」。

### 3.4 投稿頻度・最終投稿経過（機能 03）— 読み取り時計算

**実体列を持たず、`feed_overview` の SQL で都度集計する。** 単一ユーザ・記事数も中規模で、`idx_articles_feed_id` / `idx_articles_published_at` があるため集計は十分軽い。

```sql
SELECT
  f.id, f.title, f.url, f.folder_id, f.last_fetched_at,
  COUNT(a.id)                                      AS total_articles,
  COUNT(a.id) FILTER (WHERE a.is_read = false)     AS unread_count,
  MAX(a.published_at)                              AS last_published_at,
  COUNT(a.id) FILTER (
    WHERE a.published_at >= now() - interval '30 days'
  )                                                AS posts_last_30d
FROM feeds f
LEFT JOIN articles a ON a.feed_id = f.id
GROUP BY f.id
ORDER BY f.created_at DESC;
```

- **最終投稿経過日数** = `now() - last_published_at`。日数換算はサービス層（Rust）かフロントで `last_published_at` から算出（`last_published_at` をそのまま返し、表示側で「N日前」整形）。
- **投稿頻度** = `posts_last_30d * 7.0 / 30`（週あたり本数）。直近 30 日窓は疎なフィードにも頑健。「直近 N 件の平均間隔」案より実装が単純で外れ値に強い。
- 派生値（週あたり本数・経過日数）の最終整形はクライアント側で行ってよい（`feed_overview` は素の集計値 `last_published_at` / `posts_last_30d` / `unread_count` / `total_articles` を返す）。
- `LEFT JOIN` なので記事ゼロのフィードも 1 行返る（`last_published_at` は NULL）。

**実体列に昇格する判断基準（将来）**: 記事総数が大きくなり集計が重くなったら、`feeds` に `last_published_at` 等の materialized 列を**新マイグレーションで追加**し、`feeds/service.rs::fetch_and_store` の取り込み時に更新する。MVP では不要。

---

## 4. クライアント側で持つべき状態の線引き

単一ユーザのため、**個人の表示プリファレンスはサーバ（DB）に持たない**。ブラウザ（localStorage / signal / `createContext`）に置く。

| 状態 | 置き場 | 機能 | 備考 |
|------|--------|------|------|
| テーマ（light/dark） | `localStorage["theme"]` + `<html>` の `.dark` クラス | 04 | 初期値は `prefers-color-scheme`。`app.css` は配線済みなので class トグル + 永続化のみ |
| すべて/未読のみ トグル | signal（必要なら localStorage） | 11 | 既存 `GET /api/articles?unread=true` を叩き分けるだけ |
| 選択中フォルダ / フィード / 記事 | ルート URL（`@solidjs/router`）+ 小さな `createContext` ストア | 10 | URL に載せるとリロード耐性・共有可能。二ペインの両側が同じ選択へ反応 |
| ツリー展開/折りたたみ、サイドバー開閉(モバイル) | signal（任意で localStorage） | 02, 10 | UI 状態。サーバに不要 |

サーバ（DB）に持つ＝**共有された真実**であるもの:

- `folders`、フィードの `folder_id` 割当、フィード `title`（リネーム結果）
- `is_read`（既存）、summary/translation キャッシュ（既存）
- Instapaper 資格情報（サーバ側・実行時設定）、`read_later_items`

線引きの原則: 「別デバイスから見ても同じであるべき/購読データの一部」=> DB。「この端末の見え方の好み」=> クライアント。

---

## 5. 各機能設計者が従う共通規約

### 5.1 スライス構成

- 1 スライス = `domain.rs` / `repository.rs` / `service.rs` / `handler.rs` / `mod.rs`。`mod.rs::routes() -> Router<AppState>`。
- 新スライス追加は `features/mod.rs` に `pub mod` 1 行 + `.merge()` 1 行（§2.4）。既存スライスの拡張は当該スライス内ファイルへの追記に閉じる。

### 5.2 ドメイン / newtype

- 主キーは newtype（`FolderId(pub Uuid)` に `#[derive(sqlx::Type)] #[sqlx(transparent)]`）。`FeedId` / `ArticleId` の前例に倣う。
- 検証付き入力は値オブジェクト + `parse() -> Result<Self, String>`（`FeedUrl::parse` の前例）。例: `FolderName::parse`（空文字・前後空白・長すぎを弾く）。`Err` の `String` は `AppError::Validation` にマップ。
- 永続エンティティは `#[derive(sqlx::FromRow, Serialize)]` でテーブルを鏡写し。`feed_overview` の戻り（テーブル非対応の集計）は専用の `FromRow` 構造体を `domain.rs` に定義する。

### 5.3 エラー

- **`shared/error.rs` は編集しない。** 既存 6 バリアントで表現する:
  - 行が無い: `fetch_optional().await?.ok_or(AppError::NotFound)`。
  - 入力不正（フォルダ名空, 不正 lang 等）: `AppError::Validation(String)`。
  - Instapaper 資格情報未設定: `AppError::NotEnabled(...)`（articles の `llm_client` と同じ早期 return パターン）。
  - Instapaper API 失敗（4xx/5xx/ネットワーク）: `AppError::Upstream(String)`。資格情報誤り(403)も Upstream 文言で表現（新バリアントを足さない）。
  - 重複「後で読む」: `read_later_items.article_id` PK で冪等化し、**エラーにせず 200 で既存状態を返す**（UX: 二重押下を成功扱い）。
- ハンドラは `AppResult<T>` を返し `?` 伝播。`IntoResponse` がステータス変換。

### 5.4 sqlx / マイグレーション

- **実行時クエリのみ**（`sqlx::query` / `query_as::<_, T>(SQL).bind(..)`）。`query!` 系コンパイル時マクロ禁止。
- リポジトリは `&PgPool` を取る自由関数。missing row は `AppError::NotFound` へ。
- 新カラム/新テーブルは**必ず新ファイル**（§1 の予約番号、空いていなければ次の整数へ）。既存マイグレーション不編集。

### 5.5 ルート / HTTP

- `/api/<resource>` REST 準拠。部分更新は `PATCH`、作成は `POST`（201 + 本体）、副作用のみは 204 / 202。更新系は更新後エンティティを JSON で返すと UI が楽（既存 `summarize`/`translate` の前例）。
- リクエストボディは `#[derive(Deserialize)]`。`Query` で任意フィルタ（`feed_id`, `unread` の前例）。

### 5.6 テスト（TDD 必須）

- **Red → 理解 → Green**。純粋ロジック（`FolderName::parse`、頻度計算をサービスに切る場合）は同一ファイル `#[cfg(test)] mod tests` でユニットテスト（`FeedUrl::parse` の前例）。
- DB を伴うもの（folders CRUD、feed の `PATCH` 割当、一括既読、`feed_overview` 集計）は `backend/tests/` の統合テストで実 DB を叩く（`stats` スライスの統合テスト前例＝`MEMORY: first-api-stats` に倣う）。
- 書いたら必ず実行（`just lint` の clippy `-D warnings` も通す）。

### 5.7 フロント規約

- 新エンドポイントは `lib/api.ts` にメソッド追加。`Folder` / `FeedOverview` / read-later 用の interface を JSON 形に合わせて追加。
- UI: 単純部品（card・行・ヘッダ・トグルボタン）は自前 Tailwind + cva。複雑な a11y 部品は Ark UI をラップして `components/ui/` へ（ダーク/未読の `Switch`、フォルダツリーの `TreeView`/`Collapsible`、フィード操作の `DropdownMenu`、フィード追加の `Dialog`）。実装時に ark-ui.com で API 構造を確認。oklch トークン（`app.css`）を維持。

---

## 6. 機能別 着地点マトリクス（11機能 → どこに載るか）

| # | 機能 | マイグレーション | バックエンド | フロント |
|---|------|------------------|--------------|----------|
| 01 | feed-management | 0002 (folder_id) | **#02 が所有する** `PATCH /api/feeds/{id}` を**再利用**（リネーム UI のみ。ルート/ハンドラ/構造体は再定義しない） + `feed_overview`(未読数) | 管理ビュー（記事一覧と分離） |
| 02 | feed-folders | **0002** | **新 `folders` スライス** + `feeds` PATCH(`PATCH /api/feeds/{id}` の唯一の定義所有者＝リネーム + folder 割当) | 左ペイン フォルダ→フィード ツリー、未分類グループ |
| 03 | feed-stats | なし（読み取り時計算） | **新 `feed_overview` スライス** | 管理ビューに「N日前」「週Y件」表示 |
| 04 | dark-theme | なし | なし | `.dark` トグル + localStorage + prefers-color-scheme |
| 05 | instapaper-integration | **0003** | **新 `instapaper` スライス**（資格情報 + add, NotEnabled, reqwest 直叩き） | 設定 UI（資格情報入力） |
| 06 | read-later | **0004** | `instapaper` スライスに追記（read-later 状態 + add 呼び出し） | 記事ビューに「後で読む」ボタン、冪等/失敗 UX |
| 07 | minimal-design | なし | なし | タイポ階層・余白・prose 運用（横断指針） |
| 08 | feed-add-placement | なし | なし | 追加 UI を Dialog/左ペイン下部へ移動（Ark UI Dialog） |
| 09 | read-management | なし | `articles` 拡張(`POST /api/articles/read-all`) + 自動既読は既存 read 再利用 | 一括既読ボタン、未読数表示（feed_overview） |
| 10 | two-pane-layout | なし | なし | `@solidjs/router` ネストルート + 選択状態 context、レスポンシブ |
| 11 | unread-filter-toggle | なし | なし（既存 `?unread=true`） | 左ペインの すべて/未読 トグル（Switch/segment） |

依存関係: 06→05、11→09/10、01/02/10 は左ペイン・管理ビューで連動、07 は 04/10 と整合（oklch トークン維持）。

---

## 7. 留意点 / 要確認

- **Instapaper Simple Developer API の実仕様は `https://www.instapaper.com/api` で実装時に確認すること。** 想定（MVP）: `POST https://www.instapaper.com/api/add` に HTTP Basic 認証で `url`（必須）, `title`/`selection`（任意）を送る。成功 201、認証失敗 403、入力不正 400、障害 5xx。`instapaper/service.rs` で `state.http` を使い、ステータスを `AppError::Upstream` / `NotEnabled`（資格情報未設定）にマップ。公式 Rust SDK は無いので reqwest 直叩き（`anthropic.rs` と同手法・trait なし）。
- **資格情報の保管**: `0003` では平文列だが、家庭内 LAN・単一ユーザ前提でも password 平文保存は留意点。最低限 GET レスポンスに password を含めない（`configured: bool` のみ返す）。将来、保存時暗号化（鍵は env）に拡張する余地を残す。`.env` / シークレットはコミットしない（既存方針）。
- **per-feed refresh の修正（機能 01/関連）**: 現状 `POST /api/feeds/{id}/refresh` は全フィード再取得。`feeds/service.rs` に当該フィードのみを取る `refresh_one(state, id)` を足し、ハンドラを差し替える（既存 `refresh_all_feeds` は scheduler から引き続き使用）。これも `feeds` スライス内に閉じる拡張。
- **投稿頻度の実体列昇格**は記事数が大きくなった時点の将来課題（§3.4）。MVP は読み取り時計算で確定。
- **AppError 新バリアントが本当に要るケースが出たら**、それは `shared/error.rs` という数少ない共有ファイルの編集になる。安易に足さず、まず既存 6 バリアントで表現できないか本書 §5.3 の対応表で確認してから提案すること。
