# 11 「すべて/未読のみ」表示切り替え

> 対象読者: このリポジトリは持っているが本会話の文脈を知らない、別セッションの実装者。
> この機能は **フロントエンド専用**。バックエンド変更・マイグレーションは一切ない。

---

## 1. 概要

左ペイン（Sidebar）上部に「すべて / 未読のみ」のセグメント切り替えを置き、記事一覧（右／中ペイン）を未読記事だけに素早く絞り込めるようにする。RSS リーダーの基本的な読書動線（未読を片付ける）を支える機能で、未読が溜まったときの一覧の見通しを良くするのがユーザー価値。

バックエンドは既に `GET /api/articles?unread=true` を実装済みで、`lib/api.ts` の `listArticles({ unread })` もこのパラメータを送れる。したがって本機能の実体は **「トグル UI 1個 + グローバル状態 1フィールド + 一覧リソースのソース関数への合成」** に分解できる。新規バックエンド作業はない。

> 配置の確定: セグメントは **Sidebar 上部（FeedTree の前）に1箇所だけ** 置く。記事一覧ペインの中には置かない（土台設計 §6 のファイル構成「`Sidebar.tsx` ★ フィルタ + FeedTree + [+追加]」に従う）。Sidebar は #10 で永続インスタンス化されるため、ルート遷移してもセグメントの状態は保持される。

---

## 2. スコープ / 非スコープ

### スコープ（含む）
- グローバル UI ストア（#10 が `src/lib/store.tsx` に導入）への `filter: "all" | "unread"` フィールドと `setFilter` アクション追加。
- `filter` の localStorage 永続（単一ユーザーなので「この端末の見え方」として保存）。
- 自前 cva の 2択（以上）セグメント部品 `components/ui/segmented.tsx` の新設。**radiogroup ロールを名乗る以上、roving tabindex と矢印キー操作まで実装して契約を満たす**（§6.4）。
- 左ペイン（`Sidebar.tsx`、#10 が新設）上部にセグメントを配置。
- 記事一覧ルート（`ArticleList.tsx`、#10 が `FeedList.tsx` を改名）の `createResource` ソース関数に `unread: filter === "unread"` を合成し、トグルで自動再フェッチ。
- 未読フィルタ時の空状態コピーの出し分け（「未読の記事はありません」）。
- **（フォールバック・条件付き）** #09 が未マージで、UI に既読化トリガーが1つも無い場合に限り、`ArticleView` を開いたときの `api.markRead(id)` 1行（自動既読）を本機能で追加する。これは「未読フィルタが体感できる」ための最小トリガーであり、#09 の本格的な既読管理（一括既読・既読 UI）の代替ではない（§8 参照）。

### 非スコープ（含まない）
- バックエンドの変更（`?unread=true` は既存。フォルダ単位の `folder_id` フィルタは #02 の責務）。
- DB マイグレーション（DB 変更なし）。
- 既読管理の本体（一括既読・既読トグル UI・自動既読の体系は #09 の責務。本機能は「読まれた結果として未読一覧から消える」ことに依存するだけ。上記フォールバックは #09 不在時の最小代替に限る）。
- 未読数バッジ（Sidebar の未読カウント表示は #03/#09 の `feed_overview` 系。本機能はカウントを再計算しない）。
- フィード/フォルダ選択（URL 由来、#10 の `useSelection`）。本機能は selection を読むだけで持たない。
- フィルタ状態の URL 反映（設計判断として filter は URL に載せず store に持つ。§6.2 参照）。

---

## 3. 既存実装の調査と再利用

実ファイルを確認済み。以下は **再利用し、作り直さない**。

| 資産 | 場所 | 本機能での扱い |
|------|------|----------------|
| 記事一覧の unread フィルタ（SQL） | `backend/src/features/articles/repository.rs` の `list()`（L36-53）。除外条件は L44 `AND ($2 = false OR is_read = false)` | そのまま。変更不要 |
| `unread` クエリパラメータ受理 | `backend/src/features/articles/handler.rs` `ListQuery`（L13-18）。`{ feed_id: Option<Uuid>, unread: bool (#[serde(default)]) }` | そのまま。`unread` 省略時は `false`＝全件 |
| `idx_articles_is_read` 部分インデックス | `backend/migrations/0001_init.sql`（`WHERE is_read=false`） | 未読クエリの裏付け。変更不要 |
| API クライアント `listArticles` | `frontend/src/lib/api.ts` `listArticles`（L47-53）。`(params?: { feed_id?, unread? })`。`unread` が truthy のときだけ `?unread=true` を付与（L50） | **そのまま使える。api.ts 変更不要**（`folder_id` 追加は #02 の作業） |
| API クライアント `markRead` | `frontend/src/lib/api.ts` `markRead(id, read=true)`（L55）。`POST /api/articles/{id}/read` を叩く | **既読化トリガー（フォールバック）に流用**。新規実装不要 |
| 自前 cva 部品のパターン | `frontend/src/components/ui/button.tsx`（`cva` + `cn` + `splitProps`） | `segmented.tsx` を同じ流儀で実装 |
| `cn()` ユーティリティ | `frontend/src/lib/utils.ts`（`clsx` + `tailwind-merge`） | セグメントのクラス合成に使用 |
| デザイントークン | `frontend/src/app.css`（`--background`/`--muted`/`--accent`/`--border`/`--radius`/`--ring`、`.dark` 配線済み） | `bg-muted`/`bg-background`/`text-muted-foreground`/`ring-ring` で装飾。生 hex 不使用 |
| グローバル UI ストア | `src/lib/store.tsx`（**#10 が新設**：`createContext` + `createStore<UiStore>`、`useApp()`、`<AppProvider>`） | `filter` フィールドと `setFilter` を追記 |
| 選択導出 | `src/lib/selection.ts`（**#10 が新設**：URL → 選択スコープ） | 読むだけ。一覧リソースのソースに合成（§6.3 で最小契約を明記） |
| 記事一覧ルート | `src/routes/ArticleList.tsx`（**#10 が `FeedList.tsx` を改名**） | リソースのソース関数に `unread` を足す |

> 重要: 本機能はバックエンド（API・SQL・DB）を一切触らない。`?unread=true` は実装・インデックス済みで、`api.listArticles` も対応済み。既読化の API（`api.markRead` / `POST /api/articles/{id}/read` / `articles.is_read`）も**既に存在する**。車輪の再発明をしないこと。

---

## 4. データモデルとマイグレーション

**DB 変更なし。** 新規テーブル・カラム・マイグレーションは追加しない（既存 `0001_init.sql` も不編集）。`filter` は「この端末の見え方」であり共有すべき購読データではないため、土台設計 §4 の線引きどおり DB ではなく localStorage（クライアント）に持つ。

---

## 5. バックエンド設計

**変更なし。** 既存スライス（articles）に手を入れない。本機能が依拠する契約は以下の通り、すでに存在する。

- ルート: `GET /api/articles`（`articles/mod.rs::routes()` に既存）。
- クエリ: `unread`（bool、省略時 `false`）。`feed_id`（任意）と AND で併用可能。
- 挙動: `unread=true` → `is_read = false` の行のみ。`unread=false`/省略 → 全件。いずれも `published_at DESC NULLS LAST, created_at DESC`、`LIMIT 200`。
- 既読化（フォールバックで利用）: `POST /api/articles/{id}/read`（body `{read:bool}`、既存・204）。
- エラー: 本機能で新たに発生しうる `AppError` はない（クエリパラメータの型不一致時は axum の `Query` 抽出が 400 を返す既存挙動）。

`features/mod.rs` への `.merge()` 追加も**なし**（新スライスを作らないため）。

---

## 6. フロントエンド設計

### 6.1 追加・変更ファイル一覧

| パス | 区分 | 内容 |
|------|------|------|
| `frontend/src/components/ui/segmented.tsx` | **新規** | 自前 cva の 2択（以上）セグメント部品（汎用、a11y 完備） |
| `frontend/src/lib/store.tsx` | 変更（#10 が新設したものへ追記） | `filter` フィールド + `setFilter` + localStorage 永続 |
| `frontend/src/components/layout/Sidebar.tsx` | 変更（#10 が新設したものへ追記） | 上部にセグメントを配置し `ui.filter`/`setFilter` を接続 |
| `frontend/src/routes/ArticleList.tsx` | 変更（#10 が改名したものへ追記） | リソースのソースに `unread` 合成 + 空状態コピー出し分け |
| `frontend/src/routes/ArticleView.tsx` | 変更（条件付きフォールバックのみ） | #09 不在時に限り、開いたとき `api.markRead(id)` 1行を追加 |
| `frontend/src/lib/api.ts` | **変更なし** | `listArticles` は既に `unread?` 対応済み |

> 依存メモ: `store.tsx` / `selection.ts` / `Sidebar.tsx` / `ArticleList.tsx` は **#10（two-pane-layout）が作る**。本機能はそれらへ薄く追記する。#10 がまだ `filter` を入れていなければ本機能が入れる（土台設計 §2.1 のストア表に `filter` は予約済み）。

### 6.2 状態管理（グローバルストアへの最小追記）

土台設計どおり、グローバルに持つのは「設定 + 横断 UI フラグ」だけ。`filter` はその 1 フィールド。土台設計 §2.2 の公開アクション一覧（`setTheme`/`toggleTheme`/`setFilter`/`openSidebar`/`closeSidebar`）に合わせ、**公開するのは `setFilter` のみ**とする（`toggleFilter` のような派生アクションは消費者が無いと dead API になるため設けない。将来キーボードショートカット等で必要になった時点で追加する）。

```ts
// src/lib/store.tsx への追記イメージ（#10 の UiStore に同居）
export type Filter = "all" | "unread";

const FILTER_KEY = "rss:filter";

function initialFilter(): Filter {
  const v = localStorage.getItem(FILTER_KEY);
  return v === "unread" ? "unread" : "all"; // 既定は "all"
}

// createStore<UiStore>({ ... , filter: initialFilter() }) のように初期化。
// 公開アクション（生 setUi は外に出さない方針を踏襲）:
//   setFilter(f: Filter): setUi("filter", f); localStorage.setItem(FILTER_KEY, f);
```

- **永続化**: localStorage キー `"rss:filter"`。次回起動・リロードで復元（FOUC は一覧フェッチ前の状態決定なので問題にならない。テーマと違い同期描画前適用は不要）。
- **公開範囲**: `useApp()` から `ui.filter`（読み取り）と `setFilter`（書き込み）を露出。生 `setUi` は出さない。
- **リアクティビティ契約（重要）**: 本機能の自動再フェッチ（§6.3）と Sidebar の `value={ui.filter}`（§6.5）は、**`ui.filter` がトラッキング可能な反応値であること**に依存する。
  - #10 が `createStore<UiStore>` プロキシで実装する場合: `ui.filter` をトラッキングスコープ（`createResource` のソース関数・JSX）で読むだけで反応する。本書のコードはこの前提で書いている。
  - もし #10 がストアではなく **プレーンな signal** で `filter` を保持する設計なら、`ui.filter` は **アクセサ（`() => ui.filter()`）として渡す**こと。素のスナップショット値を渡すとトグルに追従しなくなる。実装時に #10 のストア実体（store プロキシ or signal アクセサ）を確認し、どちらでも反応するよう結線する。

### 6.3 一覧リソースへの合成（自動再フェッチの核心）

`ArticleList.tsx` で、URL 由来の選択（`useSelection`、#10）とストアの `filter` を `createResource` の **ソース関数**で合成する。Solid はソース関数内で読んだ反応値（`ui.filter`、selection）の変化を追跡し、どちらが変わっても自動で再フェッチする。

**#10 の selection API への最小依存契約**: 本機能が selection に求めるのは「現在のフィードスコープを `{ feed_id?: string }` の形で読めること」だけ。#10 の `useSelection()` がどんな形（`scope()` アクセサ / 個別アクセサ / ストアフィールド）を返しても、`feed_id` を取り出して `listArticles` に渡せれば足りる。`folder_id` は #02 の責務で、#02 マージ後に同じ合成点へ足す（本機能では触らない）。実装時に #10 の selection の実シグネチャを確認し、下記 `selection.scope()` 部分を実体へ読み替えること。

```tsx
// src/routes/ArticleList.tsx（要点のみ）
import { createResource, For, Show } from "solid-js";
import { api } from "@/lib/api";
import { useApp } from "@/lib/store";
import { useSelection } from "@/lib/selection";

export default function ArticleList() {
  const { ui } = useApp();
  const selection = useSelection(); // #10。最小契約: 現在の { feed_id?: string } を読める

  const [articles] = createResource(
    () => ({
      ...selection.scope(),          // { feed_id? }（folder_id は #02 で追加）
      unread: ui.filter === "unread" // ★ #11 の合成点（ui.filter が反応値であること: §6.2）
    }),
    (src) => api.listArticles(src),
  );

  return (
    <Show
      when={!articles.loading}
      fallback={<p class="text-muted-foreground text-sm">読み込み中…</p>}
    >
      <Show
        when={(articles()?.length ?? 0) > 0}
        fallback={
          <p class="text-muted-foreground text-sm">
            {ui.filter === "unread"
              ? "未読の記事はありません。"
              : "記事がありません。"}
          </p>
        }
      >
        <For each={articles()}>{/* 行レンダリング（#10/#07 の罫線リスト） */}</For>
      </Show>
    </Show>
  );
}
```

**意図的な設計判断 — 既読化での即時消去はしない**: ソース関数の依存は `selection` と `ui.filter` のみで、**個々の記事の `is_read` 変化には依存しない**。よって未読モードで右ペインの記事が既読化（自動既読／一括既読、§8）で `is_read=true` になっても、一覧はその場では再フェッチされず行が消えない（読書中に項目が足元から消えるのを防ぐ）。一覧が更新されるのは「フィルタ切替」「フィード/フォルダ選択切替」「明示的 refetch（フィード追加など）」のとき。次に未読モードへ入り直す/絞り込み直すと既読分は落ちる。これは RSS リーダーとして自然な挙動。

> 注: `listArticles` は `unread` が falsy のとき `?unread=true` を付けない（`api.ts` L50）。よって `filter === "all"` のとき余計なパラメータは飛ばない。`unread: false` を明示送信する必要はない。

### 6.4 セグメント UI 部品（自前 cva・a11y 完備）

土台設計 §3 の判断に従い、**2択ラベル付きで意味が明確なため自前 `segmented` を採用**（Switch は ON/OFF の意味が「すべて/未読」と対応しづらい）。CLAUDE.md の UI 方針は「複雑な a11y 部品は Ark UI」だが、土台設計はこの 2択セグメントに限り自前 cva を明示的に許容している。**ただし `role="radiogroup"` / `role="radio"` / `aria-checked` を名乗る以上、WAI-ARIA の radiogroup 操作契約（roving tabindex と矢印キーによる選択移動）まで実装して整合させる**（実装しない ARIA を主張しない）。`button.tsx` と同じ `cva`/`cn` の流儀で実装する。

```tsx
// src/components/ui/segmented.tsx（新規・汎用 2択以上対応・a11y 完備）
import { For } from "solid-js";
import { cn } from "@/lib/utils";

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
}

interface SegmentedProps<T extends string> {
  options: SegmentOption<T>[];
  value: T;
  onChange: (v: T) => void;
  class?: string;
  "aria-label"?: string;
}

export function Segmented<T extends string>(props: SegmentedProps<T>) {
  // roving tabindex / 矢印キー移動のためのフォーカス対象 ref。
  const refs: HTMLButtonElement[] = [];

  const move = (dir: 1 | -1) => {
    const i = props.options.findIndex((o) => o.value === props.value);
    const n = props.options.length;
    const next = (i + dir + n) % n;            // 端は循環
    props.onChange(props.options[next].value); // radiogroup: フォーカス移動＝選択
    refs[next]?.focus();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    switch (e.key) {
      case "ArrowRight":
      case "ArrowDown":
        e.preventDefault();
        move(1);
        break;
      case "ArrowLeft":
      case "ArrowUp":
        e.preventDefault();
        move(-1);
        break;
    }
  };

  return (
    <div
      role="radiogroup"
      aria-label={props["aria-label"]}
      class={cn(
        "inline-flex items-center rounded-md border border-border bg-muted p-0.5 text-sm",
        props.class,
      )}
    >
      <For each={props.options}>
        {(opt, i) => {
          const selected = () => props.value === opt.value;
          return (
            <button
              ref={(el) => (refs[i()] = el)}
              type="button"
              role="radio"
              aria-checked={selected()}
              tabindex={selected() ? 0 : -1}   /* roving tabindex */
              onClick={() => props.onChange(opt.value)}
              onKeyDown={onKeyDown}
              class={cn(
                "h-7 flex-1 rounded-sm px-3 font-medium transition-colors",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                selected()
                  ? "bg-background text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {opt.label}
            </button>
          );
        }}
      </For>
    </div>
  );
}
```

a11y のポイント:
- **roving tabindex**: 選択中セグメントだけ `tabindex=0`、他は `tabindex=-1`。グループ全体で Tab ストップは1つ。
- **矢印キー**: Left/Up＝前、Right/Down＝次。radiogroup の慣習どおり「フォーカス移動＝選択」（`onChange` を呼んでから `.focus()`）。端は循環。
- **クリック/Space/Enter**: ネイティブ `<button>` の既定挙動で `onClick`＝選択。
- 装飾はトークンのみ（`bg-muted` トラック / `bg-background`+`shadow-sm` で選択セグメントを浮かせる / `ring-ring` フォーカス）。生の色は書かない。`.dark` 配線済みのためダークでも自動追従。

> a11y を Ark UI に寄せたい場合の代替: `@ark-ui/solid` の `SegmentGroup`（`SegmentGroup.Root/Item/ItemText/Indicator`、`checked`/`onValueChange` 系）。roving/矢印も内蔵。差し替えは `segmented.tsx` 1ファイルに閉じる。**Ark UI v5 の正確な part 名・props は ark-ui.com（Solid / Segment Group）で実装時に要確認**（「この通り動く」と断定しない）。

### 6.5 Sidebar への配置

`Sidebar.tsx`（#10 が新設、永続インスタンス）の上部、FeedTree の前にセグメントを置く。Sidebar が再マウントされない設計なので、ルート遷移してもセグメントの見た目とストア値は保持される。

```tsx
// src/components/layout/Sidebar.tsx（追記イメージ）
import { Segmented } from "@/components/ui/segmented";
import { useApp } from "@/lib/store";

const { ui, setFilter } = useApp();

<div class="px-2 py-2">
  <Segmented
    aria-label="記事の表示フィルタ"
    options={[
      { value: "all", label: "すべて" },
      { value: "unread", label: "未読のみ" },
    ]}
    value={ui.filter}      /* §6.2 のリアクティビティ契約に注意（signal 実装なら () => ui.filter()） */
    onChange={setFilter}
    class="w-full"
  />
</div>
```

> モバイル（`md` 未満、#10 の Drawer）でも同じ Sidebar 内に出るため、追加対応は不要。

---

## 7. API 契約

**新規・変更エンドポイントなし。** 既存契約を本機能が利用するのみ。

### `GET /api/articles`（既存・再掲）

クエリパラメータ:

| 名前 | 型 | 既定 | 意味 |
|------|----|------|------|
| `feed_id` | UUID（任意） | なし | フィード絞り込み（#10 の selection 由来） |
| `unread` | bool（任意） | `false` | `true` で `is_read=false` のみ。**本機能が制御する** |

リクエスト例:
```
GET /api/articles?unread=true
GET /api/articles?feed_id=2b1f...&unread=true
```

レスポンス例（200、`Article[]` の抜粋）:
```json
[
  {
    "id": "9f1c0e2a-...",
    "feed_id": "2b1f...",
    "url": "https://example.com/post/42",
    "title": "未読の記事タイトル",
    "content": "...",
    "published_at": "2026-06-25T09:00:00Z",
    "is_read": false,
    "summary": null,
    "summary_lang": null,
    "translation": null,
    "translation_lang": null,
    "processed_at": null,
    "created_at": "2026-06-25T09:01:00Z"
  }
]
```

`unread=true` のレスポンスには `is_read=true` の記事は含まれない（SQL で除外、`repository.rs` L44）。

### `POST /api/articles/{id}/read`（既存・フォールバックで利用）

既読化トリガーが UI に1つも無い場合のフォールバック（§8）でのみ叩く。

```
POST /api/articles/9f1c0e2a-.../read
Content-Type: application/json

{ "read": true }
```
→ `204 No Content`。`api.markRead(id)`（`api.ts` L55）がこれを呼ぶ。

---

## 8. 依存関係

### 依存する（先に必要）

- **#10 two-pane-layout（唯一の構造的ブロッカー）**: 本機能が追記する `src/lib/store.tsx`（グローバル UI ストア・`useApp`・`AppProvider`）、`src/lib/selection.ts`（`useSelection`）、`src/components/layout/Sidebar.tsx`、`src/routes/ArticleList.tsx`（`FeedList.tsx` の改名）はすべて #10 が新設/再構成する。これらが無いと「載せる場所」も「合成先」も無い。**本機能の真のブロッカーはこれだけ。**

- **#09 read-management（ソフト依存・体験を豊かにするだけ／ブロッカーではない）**: 「未読のみ」フィルタは**既存の既読化機構の上で既に機能する**。`articles.is_read` カラム・`POST /api/articles/{id}/read`・`api.markRead` はいずれも実装済みで、未読/既読の区別はこれらが生む。フィルタが「体感」できるには UI に**既読化トリガーが最低1つ**あればよく、最小は「記事を開いたら自動既読」＝`ArticleView` での `api.markRead(id)` 1行呼び出しに過ぎない。
  - 現状 `ArticleView` は `markRead` を呼んでいない（`FeedList.tsx` L60 は `is_read` で文字色を変えるだけ）。つまり今は既読化トリガーがゼロなので、何も既読にならずフィルタの効果が見えない。
  - **対処**: #09 がマージ済みなら #09 の自動既読/一括既読がトリガーを提供するのでそれに乗る。#09 未マージなら、本機能が**フォールバックとして** `ArticleView` に `api.markRead(id)` 1行だけ足してフィルタを観測可能にする（§2 スコープのフォールバック項目）。
  - 結論: **#09 は #11 をブロックしない。** #09 は一括既読・既読 UI など体験を充実させるが、無くても #11 は #10 さえ揃えば動く。

### ブロックする（本機能を待つ）
- なし（リーフ機能）。ただし #07 minimal-design は Sidebar のセグメント外観をデザイン指針へ揃える対象に含めうる（緩い整合のみ、ブロックではない）。

### 共有ファイルの衝突注意
- `src/lib/store.tsx`・`Sidebar.tsx`・`ArticleList.tsx` は #04（theme）・#10 と同居編集になる。`filter` フィールド追加は #04 の `theme` 追加と独立。マージ時は UiStore の型に両フィールドが揃うことを確認。

---

## 9. テスト計画（TDD）

> フロントエンドにテストランナーは未導入（`frontend/package.json` に vitest/jest なし）。よってフロントは **型チェック（`just lint` の `tsc`）+ 手動確認**。

### 9.1 バックエンドの扱い — 「stats 前例」は誤り。Rust 結合テストは原則作らない

**重要な訂正**: このリポジトリに `backend/tests/` ディレクトリは **存在しない**。バックエンド全体で唯一の Rust テストは `backend/src/features/feeds/domain.rs` の `#[cfg(test)] mod tests`（純粋ロジックの単体テスト）だけである。`stats` スライスには **Rust 結合テストは無い**。`stats` の検証は実 DB に繋ぐ Rust 統合テストではなく、**起動済みスタックへ curl する shell スクリプト** `scripts/test/api-stats.sh`（nginx :8081 の `/api/stats` を叩き 200 と JSON キーを検証）で行われている（MEMORY「first-api-stats」の訂正＝以前言及された "integration test" は実在しなかった、と整合）。したがって「stats 結合テスト前例に倣う」という記述は**事実誤認なので採らない**。

本機能は **バックエンドのコードを1行も変えない**。よって既定方針は次のとおり:

- **(推奨・既定) Rust 結合テストは追加しない**。バックエンド変更ゼロのため回帰対象が無く、新規の重い土台（後述）を背負う価値が無い。`?unread=true` の SQL 挙動を固定したいだけなら、`scripts/test/api-stats.sh` を**写経した curl shell スクリプト** `scripts/test/api-articles-unread.sh` を任意で足す（起動済みスタック前提、Cargo を汚さない）。これが既存の検証様式に最も忠実。
- **(任意・もし Rust 統合テストを本当に作るなら) これはリポジトリ初の `backend/tests/` になり、新規ハーネス土台が要る**。過小評価しないこと。最低限必要なもの:
  - `backend/Cargo.toml` の `[dev-dependencies]`: `tokio`（`macros`,`rt-multi-thread`）、`sqlx`（`runtime-tokio`,`postgres`,`macros` 等のテスト機能）。
  - テスト用 `DATABASE_URL`（専用 DB or 各テスト分離）。`sqlx::test` フィクスチャ（マイグレーション自動適用＋トランザクション分離）を使うか、自前で seed/teardown を書く。
  - `backend/tests/articles_list.rs` を新規作成し、下表のケースを実装。

  | テスト | 意図（Red→Green） |
  |--------|-------------------|
  | `unread_true_returns_only_unread` | 既読1件・未読1件を投入→`GET /api/articles?unread=true` が未読1件のみ。L44 の `is_read=false` 絞り込みを固定 |
  | `unread_absent_returns_all` | 同じ投入で `unread` 省略→2件。既定 `false`＝全件を固定 |
  | `feed_id_and_unread_combine` | 2フィード×既読/未読→`feed_id` と `unread=true` の AND を固定 |

  ただし上記は**既存挙動の回帰ガードに過ぎず、#11 のフロント実装には不要**。導入判断はチームに委ねる（既定は「作らない」）。

### 9.2 フロント型チェック（`just lint`）
- `Segmented<T>` のジェネリックが `value`/`onChange`/`options[].value` で同一 `T` に束縛されること（`"all" | "unread"` を渡してコンパイルが通る）。
- `useApp().ui.filter` が `Filter` 型、`setFilter` が `(f: Filter) => void` であること。
- `api.listArticles({ unread: boolean })` が既存シグネチャで型エラーにならないこと。

### 9.3 手動確認チェックリスト
前提: UI に既読化トリガーが少なくとも1つある（#09、または §8 のフォールバック1行）。
1. 既定で「すべて」が選択され、既読・未読が混在表示される。
2. 「未読のみ」に切替→既読記事が一覧から消え、未読のみになる。
3. リロード→「未読のみ」が復元される（localStorage `rss:filter`）。
4. フィード/フォルダ選択を切り替えても `filter` が保持される（Sidebar 永続インスタンス）。
5. 未読0件のフィードで「未読のみ」→「未読の記事はありません。」が出る。「すべて」では「記事がありません。」（または通常一覧）。
6. 「未読のみ」で記事を開き既読化しても、戻ったとき一覧からその場で消えていない（§6.3 の意図）。再度フィルタ/選択を切り替えると消える。
7. キーボード操作: Tab でセグメント（選択中の1つ）にフォーカス→`focus-visible:ring-2` が出る。**矢印キー（←→／↑↓）で選択が移動し一覧が再フェッチされる**（roving tabindex・矢印操作の確認、§6.4）。Space/Enter/クリックでも切替。
8. ダークモード（#04）でセグメントの選択/非選択コントラストが破綻しない。

---

## 10. 実装手順（順序付きチェックリスト）

前提: #10 がマージ済みで `store.tsx`/`selection.ts`/`Sidebar.tsx`/`ArticleList.tsx` が存在する。#09 は任意（未マージなら手順4.5を実施）。

1. `frontend/src/components/ui/segmented.tsx` を新規作成（§6.4。roving tabindex＋矢印キーまで実装）。`just lint` で型を通す。
2. `frontend/src/lib/store.tsx` に `Filter` 型・`initialFilter()`・`filter` フィールド・`setFilter` を追記、localStorage キー `"rss:filter"` で永続（§6.2）。`useApp()` から `ui.filter`/`setFilter` を露出。#10 の store 実体（createStore プロキシ or signal）を確認し、§6.2 のリアクティビティ契約を満たす。
3. `frontend/src/components/layout/Sidebar.tsx` 上部に `Segmented` を配置し `ui.filter`/`setFilter` を接続（§6.5）。
4. `frontend/src/routes/ArticleList.tsx` の `createResource` ソース関数に `unread: ui.filter === "unread"` を合成し、空状態コピーを `filter` で出し分け（§6.3）。#10 の `useSelection` 実シグネチャに合わせ `feed_id` の取り出し方を読み替える。
   - 4.5（条件付き）: #09 が未マージで既読化トリガーが UI に無ければ、`frontend/src/routes/ArticleView.tsx` で記事を開いたときに `api.markRead(id)` を1回呼ぶ（§8 フォールバック）。#09 がトリガーを持つなら本手順はスキップ。
5. `api.listArticles` が変更不要であることを確認（`unread?` 既対応、`api.ts` L47-53）。
6. `just lint`（clippy `-D warnings` + `tsc`）を通す。
7. （任意）`scripts/test/api-articles-unread.sh` を `api-stats.sh` 流儀で追加し、起動済みスタックに対し `?unread=true` 契約を curl 検証（§9.1）。Rust 統合テストは既定で作らない。
8. `just dev-db` + `just back` + `just front` で起動し §9.3 の手動チェックを実施。
9. `cargo fmt` / prettier 整形。コミットはユーザー指示があるときのみ。

---

## 11. リスク・未決事項・代替案

- **UI 形態の選択（Switch vs Segmented）**: 要件は「Ark UI Switch かセグメント」。本設計は **自前 Segmented を採用**（2択ラベルで「すべて/未読」の意味が明確、Switch の ON/OFF より誤解が少ない）。radiogroup の操作契約（roving tabindex＋矢印）は §6.4 で実装済み。a11y をライブラリに委ねたい場合の代替は Ark UI `SegmentGroup`。差し替えは `segmented.tsx` 1ファイルに閉じる。**Ark UI v5 の正確な part 名・props は実装時に ark-ui.com（Solid）で要確認**（「この通り動く」と断定しない）。
- **#10 ストア実体の確認**: `filter` が反応するかは #10 の store 実装（createStore プロキシ or signal アクセサ）に依存。§6.2 のリアクティビティ契約どおり、signal 実装なら `() => ui.filter()` で渡す。実装時に #10 のコードで確認すること。
- **#10 selection API の進化耐性**: 本機能は selection から `{ feed_id?: string }` を読めれば足りる（§6.3 の最小契約）。#10 が `useSelection()` の返り形を変えても、`feed_id` の取り出し1点を読み替えるだけで吸収できる。
- **#09 不在時のフォールバック責務**: §8 のとおり、既読化トリガーが UI にゼロだとフィルタの効果が見えない。#09 未マージ時は本機能が `ArticleView` に `api.markRead(id)` 1行を足す（最小トリガー）。本格的な既読管理（一括既読・既読 UI・自動既読の体系）は #09 の責務であり、フォールバックはあくまで暫定。#09 マージ後はそちらに一本化してよい。
- **永続化の是非**: `filter` を localStorage に保存（土台設計の表では「任意」）。本設計は単一ユーザー前提で保存を推奨。不要なら `initialFilter()` を `() => "all"` 固定にしロジックを削るだけで撤回可能。
- **既読化との一覧整合**: §6.3 のとおり「未読モードで開いた記事が即消えしない」を意図的挙動とする。もし「読了即時に未読一覧から消す」挙動が望ましいと判明したら、既読イベントを購読してソースへ依存追加 or 明示 refetch する代替に切替可能（要 UX 判断、現状は保留）。
- **未読数バッジとの整合**: Sidebar の未読カウント（#03/#09）は本機能と独立。フィルタ切替はカウントを変えない（表示中の記事集合を変えるだけ）。両者を結線しないことを明示しておく。
- **バックエンド前提の安定性**: `?unread=true` は既存・インデックス済み（`idx_articles_is_read` 部分インデックス）。`LIMIT 200` のため超大量未読時は 200 件で頭打ち（既存仕様、本機能では変更しない。将来ページングは別機能）。
- **バックエンド結合テストの土台コスト**: §9.1 のとおり、もし Rust 統合テストを作るならリポジトリ初の `backend/tests/` となり `[dev-dependencies]`・テスト DB・`sqlx::test` 等の新規土台が必要。本機能はバックエンド無変更なので既定では作らず、必要なら curl スクリプトで代替する。
