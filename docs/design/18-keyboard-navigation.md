# 18 キーボード操作（j/k 移動・m 既読・o 原文・/ 検索・g 一覧・Enter で開く）

> 読み手向けメモ: このドキュメントは「リポジトリは手元にあるが、この会話の文脈を知らない別セッションの実装者」が、これだけ読めば着手・完了できる粒度で書いている。**本機能はフロントエンドのみ**（バックエンド・DB・マイグレーション変更なし）。裏取りした実ファイル（このセッションで実際に開いて確認した）: `frontend/src/App.tsx`, `frontend/src/index.tsx`, `frontend/src/lib/store.tsx`, `frontend/src/lib/selection.ts`, `frontend/src/lib/api.ts`, `frontend/src/lib/search.ts`, `frontend/src/lib/theme.ts`(フロント専用 lib の「純関数 + 副作用 hook」前例), `frontend/src/lib/read-trigger.ts`(純関数 + DOM 分離の前例), `frontend/src/routes/Reader.tsx`, `frontend/src/routes/ArticleList.tsx`, `frontend/src/components/article/ArticleDetail.tsx`, `frontend/vitest.config.ts`(`environment: "jsdom"`), `frontend/src/lib/theme.test.ts`(jsdom を使うテストの前例)。

## 1. 概要

3ペインリーダーをマウスに触れず操作できるグローバルキーボードショートカットを追加する。Reader（`/`, `/feeds/:feedId`, `/folders/:folderId`）で中央の記事一覧を `j`/`k` で上下移動し、`Enter` で本文を開き、`m` で既読、`o` で原文をブラウザの新規タブで開き、`/` で全文検索へ、`g` で本文から一覧へ戻る。`?` で任意のチートシート overlay を開閉する。

実装は **フロント専用の純ロジック `lib/keyboard.ts`（キー→アクションの解決と一覧の前後移動を純関数化、vitest 対象）+ 副作用 hook `useKeyboardShortcuts()`（window への `keydown` リスナ登録）** に閉じる。`theme.ts`（純関数 `initialTheme`/`applyTheme` + 副作用 `setTheme`/`initTheme`）と同じ二層構成を踏襲する。

新しい状態・API・コンポーネントは最小限。記事の「現在の表示順」だけは中央ペイン（`ArticleList.tsx`）とグローバルハンドラの2箇所で共有が要るため、既存 `lib/store.tsx`（`readIds` を兄弟ペイン間で共有しているのと同じ理由・同じ仕組み）に薄く1フィールド追加する。既読化（`api.markRead` + `store.markReadLocal`）・記事選択（`?article=<id>` クエリ）・検索遷移（`/search`）はすべて**既存の仕組みをそのまま再利用**する。

## 2. スコープ / 非スコープ

**含む（このチケットでやる）**
- 新フロント lib `frontend/src/lib/keyboard.ts`: キー→アクション解決 `resolveAction`、編集要素判定 `isEditableTarget`、一覧前後移動 `stepId` の**純関数群** + 副作用 hook `useKeyboardShortcuts()`。
- 純関数の vitest 一式 `frontend/src/lib/keyboard.test.ts`（既存 `*.test.ts` と同列・jsdom）。
- `lib/store.tsx` への薄い追加: 中央一覧の表示順 `navItems`（`{ id, url }[]`）と setter。`?`overlay 用に `helpOpen`（任意）。
- `routes/ArticleList.tsx` に1つ `createEffect` を足し、現在の `articles()` の `{id,url}` 列を `store.setNavItems(...)` で公開する（行クリックの挙動は不変）。
- `App.tsx`（`AppProvider` 配下）に `<KeyboardShortcuts />` を1枚マウントして hook を起動。
- 任意のチートシート overlay `components/keyboard/KeyboardHelp.tsx`（`?` で開閉。実装が重ければ後回し可、§6.5）。

**含まない（別チケット / 別機能）**
- バックエンド・DB・マイグレーション・API エンドポイントの追加や変更（**一切なし**）。
- `Search.tsx` 内のキー操作（検索結果の j/k 移動など）。本チケットは Reader の一覧/本文操作に閉じる（`/` は検索ページへ「遷移」するのみ）。
- キーバインドのユーザー設定 UI / リマップ（将来課題、§11）。
- フォーカスリング/フォーカストラップなどの a11y 全面改修。`isEditableTarget` による入力中の抑止だけ行う。
- `vim` 的な連続キー（`gg` 等）・カウントプレフィックス（`5j`）。

## 3. 既存実装の調査と再利用

**車輪の再発明をしないため、以下を再利用する。** 以下はこのセッションで実ファイルを開いて確認済み。

- **`lib/theme.ts` の「純関数 + 副作用 hook/関数」二層構成**。`initialTheme()`/`applyTheme()` が純（テスト容易）、`setTheme()`/`initTheme()` が DOM・localStorage 副作用。本機能も `resolveAction`/`stepId`/`isEditableTarget`（純）と `useKeyboardShortcuts()`（`window` リスナ副作用）に分ける。
- **`lib/read-trigger.ts` の純関数 + DOM 分離パターン**。`scrolledEnough`（純・vitest 対象）と `findScrollParent`/`readScrollMetrics`（DOM 依存）を分けている。`onCleanup` でリスナを破棄する作法もここに前例がある（`ArticleDetail.tsx` の `createEffect`+`onCleanup`）。
- **`lib/store.tsx`（`createStore` + `createContext`）**。`readIds` は「本文ペインで立て、一覧ペインが読む」兄弟ペイン間共有のために存在する。一覧の表示順 `navItems` も「`ArticleList` が書き、グローバルハンドラが読む」同型の共有なので、ここに足すのが正しい場所。新しい状態ライブラリは入れない（CLAUDE.md / 土台設計）。
- **`?article=<id>` クエリによる記事選択**（`Reader.tsx` / `ArticleList.tsx`）。記事を「開く/移動する」= `setSearchParams({ article: id })`、「一覧へ戻る」= `setSearchParams({ article: null })`。`j/k/Enter/g` はこの既存メカニズムを呼ぶだけで、本文表示（`ArticleDetail`）・自動既読（滞在/スクロール）は無改修で連動する。
- **`api.markRead(id, true)` + `store.markReadLocal(id)`**（`api.ts` / `ArticleDetail.tsx`）。`m` の既読化はこの2つを呼ぶだけ。`markReadLocal` で一覧行のグレーアウト（`ArticleList.tsx` の `app.state.readIds[a.id]` 判定）に即追従する。POST 本体は `ArticleDetail` の自動既読と同じ `/api/articles/{id}/read`。
- **`lib/search.ts` と `/search` ルート**（`index.tsx` の `<Route path="/search" component={Search} />`）。`/` は検索ページへ `navigate("/search")` で遷移する（検索 UI は `Search.tsx` が持つ）。`searchHref` は「クエリ付き遷移」用なので本チケットの素の遷移では使わないが、将来「選択語で検索」へ拡張する余地として記す。
- **`lib/selection.ts`（`useSelection()` / `scopeFromPath`）**。`g`（一覧へ戻る）はスコープ（feed/folder）を保持したまま `?article` を外すだけにするため、scope を変更しない。selection の仕組みはそのまま生きる。
- **`@solidjs/router` の `useNavigate` / `useSearchParams`**。Reader 群と同じルータ API を hook 内で使う（`App` は `<Router root={App}>` 直下なのでルータコンテキスト内）。
- **vitest（jsdom）の慣習**（`vitest.config.ts` の `environment: "jsdom"`、`theme.test.ts` が `document`/`localStorage` を使う前例）。純関数テストは jsdom 上で `document.createElement` などを使ってよい。

## 4. データモデルとマイグレーション

**DB 変更なし・マイグレーション追加なし。** 本機能はフロントエンド専用で永続データを一切持たない（キーバインドは定数、状態は揮発のメモリのみ）。

> 参考: 最新マイグレーションは `0005_search.sql`（全文検索）。本チケックは番号を消費しない。将来「ユーザー定義キーバインドの永続化」を入れる場合のみ DB/設定が要るが、それは別チケット（§11）。**着手前に `backend/migrations/` の最新番号を確認**し、本機能が番号を取らない（取る必要がない）ことを再確認すること。

## 5. バックエンド設計

**変更なし。** 新スライス・新ルート・`features/mod.rs` への `.merge()` 追加はいずれも**不要**。既存エンドポイント（`POST /api/articles/{id}/read`, `GET /api/search`, `GET /api/articles/{id}` 等）をフロントから呼ぶだけで、サーバ側のコードには一切触れない。AI 機能も含まないため `shared/llm` も関係しない（参考: AI を伴う機能なら `shared/llm` 再利用 + DB キャッシュ + `ANTHROPIC_API_KEY` 未設定で `AppError::NotEnabled`。本機能は該当しない）。

> Vertical Slice 方針との関係: 本機能は「新スライス1枚」を作らない数少ない例外で、**バックエンドの縦割りに触れないフロント単独機能**である。スライス境界・`AppError`・`sqlx` 規約はいずれも無関係。

## 6. フロントエンド設計

二層構成（純ロジック `lib/keyboard.ts` ＋ 副作用 hook ＋ 起動コンポーネント ＋ 状態 store ＋ 一覧側の publish）。

### 6.1 `lib/keyboard.ts`（純ロジック + hook、新規）

純関数（vitest 対象）と副作用 hook を1ファイルに同居させる（`theme.ts` と同じ）。hook は JSX を返さないので拡張子は `.ts` のままでよい。

```ts
import { onCleanup, onMount } from "solid-js";
import { useNavigate, useSearchParams } from "@solidjs/router";
import { api } from "@/lib/api";
import { useApp } from "@/lib/store";

/** ショートカットが起こす論理アクション。 */
export type KeyAction =
  | "next" // j: 次の記事へ選択移動
  | "prev" // k: 前の記事へ選択移動
  | "open" // Enter: 選択記事を開く（未選択なら先頭）
  | "markRead" // m: 選択記事を既読化
  | "openOriginal" // o: 原文を新規タブで開く
  | "search" // /: 検索ページへ
  | "gotoList" // g: 本文を閉じて一覧へ戻る（scope 保持）
  | "toggleHelp"; // ?: チートシート開閉

/** KeyboardEvent の必要部分だけを抜いた、テスト用に注入しやすい型。 */
export interface KeyEventLike {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}

/**
 * キー → アクションの対応表（唯一の真実）。
 * 注: "?" は shift+"/" だが、ブラウザは KeyboardEvent.key に解決後の "?" を入れるので
 * そのまま引ける。shift は許可（"?" に必要）。修飾は ctrl/meta/alt のみ抑止する。
 */
export const KEY_BINDINGS: Readonly<Record<string, KeyAction>> = {
  j: "next",
  k: "prev",
  Enter: "open",
  m: "markRead",
  o: "openOriginal",
  "/": "search",
  g: "gotoList",
  "?": "toggleHelp",
};

/**
 * イベント様オブジェクトをアクションへ解決する純関数。
 * - ctrl/meta/alt のいずれかが押されていれば null（ブラウザ/OS ショートカットを奪わない）。
 * - 未割り当てキーは null。
 * shift は抑止しない（"?" のため）。
 */
export function resolveAction(e: KeyEventLike): KeyAction | null {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  return KEY_BINDINGS[e.key] ?? null;
}

/**
 * フォーカスが「文字入力中の要素」かを判定する純関数。
 * 入力中はショートカットを発火させない（検索ボックス等でのタイプを壊さない）。
 */
export function isEditableTarget(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    el.isContentEditable
  );
}

/**
 * 一覧の前後移動を計算する純関数。ラップせず端でクランプ（端の更に先は据え置き）。
 * - ids 空 → null（移動先なし）。
 * - current が一覧に無い（未選択含む） → dir=1 で先頭 / dir=-1 で末尾。
 */
export function stepId(
  ids: readonly string[],
  current: string | null,
  dir: 1 | -1,
): string | null {
  if (ids.length === 0) return null;
  const i = current ? ids.indexOf(current) : -1;
  if (i === -1) return dir === 1 ? ids[0] : ids[ids.length - 1];
  const next = i + dir;
  if (next < 0 || next >= ids.length) return ids[i]; // 端でクランプ
  return ids[next];
}

/**
 * グローバルショートカットの副作用 hook。AppProvider 配下かつ Router コンテキスト内で呼ぶ。
 * window に keydown を1つ張り、onCleanup で外す（read-trigger / ArticleDetail と同じ作法）。
 */
export function useKeyboardShortcuts(): void {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const app = useApp();

  const selectedId = (): string | null => {
    const a = searchParams.article;
    return (Array.isArray(a) ? a[0] : a) ?? null;
  };

  const dispatch = (action: KeyAction): void => {
    const items = app.state.navItems;
    const ids = items.map((it) => it.id);
    const cur = selectedId();

    switch (action) {
      case "next":
      case "prev": {
        const id = stepId(ids, cur, action === "next" ? 1 : -1);
        if (id) setSearchParams({ article: id });
        return;
      }
      case "open": {
        const id = cur ?? ids[0] ?? null;
        if (id) setSearchParams({ article: id });
        return;
      }
      case "markRead": {
        if (!cur) return;
        void api.markRead(cur, true).catch((e) =>
          console.error("keyboard mark-read failed", e),
        );
        app.markReadLocal(cur); // 一覧行のグレーアウトに即追従
        return;
      }
      case "openOriginal": {
        const it = items.find((x) => x.id === cur);
        if (it) window.open(it.url, "_blank", "noopener,noreferrer");
        return;
      }
      case "search":
        navigate("/search");
        return;
      case "gotoList":
        setSearchParams({ article: null }); // scope は保持、本文だけ閉じる
        return;
      case "toggleHelp":
        app.toggleHelp();
        return;
    }
  };

  const onKeyDown = (e: KeyboardEvent): void => {
    if (isEditableTarget(e.target)) return;
    const action = resolveAction(e);
    if (!action) return;
    e.preventDefault(); // j/k のページスクロールや "/" のクイック検索を抑止
    dispatch(action);
  };

  onMount(() => window.addEventListener("keydown", onKeyDown));
  onCleanup(() => window.removeEventListener("keydown", onKeyDown));
}
```

設計ノート:
- **純関数 3 つ（`resolveAction` / `isEditableTarget` / `stepId`）が単体テストの主対象**。`dispatch`/`onKeyDown` はそれらを薄く合成して既存 API/store/router を呼ぶだけ（`theme.ts` で副作用関数を薄く保つのと同方針）。
- `selectedId()` は `Reader.tsx` / `ArticleList.tsx` と同じ「`searchParams.article` が配列なら先頭」を踏襲。
- `markRead` の POST は `ArticleDetail` の自動既読と同一エンドポイント。一覧の `is_read` は再フェッチしないが、`markReadLocal` → `store.readIds` で行のグレーアウトに即反映される（既存の見た目契約に一致）。
- `next/prev/open/gotoList` はすべて `?article` の付け外しに集約。本文描画・自動既読・モバイルの master-detail 切替（`Reader.tsx` の `selectedId()` 分岐）はすべて既存のまま連動する。

### 6.2 `lib/store.tsx`（薄い追加）

`UiState` に「中央一覧の表示順」と「help overlay 開閉」を足す。`readIds`（兄弟ペイン共有）と同じ理由・同じ場所。

```ts
// UiState に追記
export interface UiState {
  sidebarOpen: boolean;
  filter: "all" | "unread";
  readIds: Record<string, true>;
  // 中央ペインの現在の表示順（ArticleList が書き、キーボードハンドラが j/k/o/Enter で読む）。
  // o（原文）のため url も持つ。selection.readIds と同型の兄弟ペイン間共有。
  navItems: { id: string; url: string }[];
  // ? のチートシート overlay 開閉（任意機能）。
  helpOpen: boolean;
}

// UiStore に追記
export interface UiStore {
  // ...既存...
  setNavItems(items: { id: string; url: string }[]): void;
  toggleHelp(): void;
  closeHelp(): void;
}

// createStore 初期値に追記
const [state, setState] = createStore<UiState>({
  sidebarOpen: false,
  filter: "all",
  readIds: {},
  navItems: [],
  helpOpen: false,
});

// store 実装に追記
const store: UiStore = {
  // ...既存...
  setNavItems: (items) => setState("navItems", items),
  toggleHelp: () => setState("helpOpen", (v) => !v),
  closeHelp: () => setState("helpOpen", false),
};
```

### 6.3 `routes/ArticleList.tsx`（1 effect 追加）

現在の `articles()` の `{id,url}` 列を store へ公開する。**行クリックや表示は不変**、`createEffect` を1つ足すだけ。

```ts
// 既存 import に createEffect を追加
import { createEffect, createResource, createSignal, For, Show } from "solid-js";

// 既存の const [articles, { refetch }] = ... の直後に追加:
createEffect(() => {
  app.setNavItems((articles() ?? []).map((a) => ({ id: a.id, url: a.url })));
});
```

- `articles()` はスコープ（feed/folder/all）・未読フィルタ変化で自動再フェッチされる（既存）。effect がその都度 `navItems` を更新するので、`j/k` の移動対象が常に「いま画面に見えている順序」と一致する。
- `Article` は `id` と `url` を持つ（`api.ts` の `interface Article` 確認済み）。

### 6.4 `App.tsx`（ハンドラ起動コンポーネントのマウント）

hook は `useApp()`（AppProvider 配下）と `useNavigate`/`useSearchParams`（Router 配下）の両方を要する。`App` は `<Router root={App}>` 直下なので Router コンテキスト内だが、`AppProvider` は `App` の中で children を包む。よって **AppProvider の内側に薄いコンポーネント `<KeyboardShortcuts />` を置いて hook を呼ぶ**（App の body 直書きだと useApp がコンテキスト外になる）。

```tsx
// App.tsx 内に小コンポーネントを定義（または別ファイルへ）
import { useKeyboardShortcuts } from "@/lib/keyboard";
import KeyboardHelp from "@/components/keyboard/KeyboardHelp"; // 任意（§6.5）

function KeyboardShortcuts() {
  useKeyboardShortcuts();
  return <KeyboardHelp />; // overlay を作らない場合は `return null;`
}

// JSX: <AppProvider> の内側、<div ...> の中（Sidebar の隣でよい）に1行マウント
//   <AppProvider>
//     <KeyboardShortcuts />
//     <div class="relative min-h-dvh ...">
//       ...
```

- `KeyboardShortcuts` は AppProvider の子なので `useApp()` が解決する。`App` 自体が Router 直下なので `useNavigate`/`useSearchParams` も解決する。
- overlay を作らないなら `KeyboardHelp` の import を消し `return null;` にする（機能は overlay 無しで完全動作する）。

### 6.5 `components/keyboard/KeyboardHelp.tsx`（任意・チートシート overlay）

`?` で開閉する一覧。`store.helpOpen` を見て表示し、`Esc` または背景クリックで閉じる。複雑な a11y が要るなら Ark UI の Dialog（`components/ui/dialog.tsx` のラップ前例）を使ってよいが、純表示の薄い overlay で十分。**実装が重ければ本コンポーネントは省略可**（コア機能は §6.1〜6.4 で完結する）。

```tsx
import { For, Show } from "solid-js";
import { useApp } from "@/lib/store";

const ROWS: [string, string][] = [
  ["j / k", "次の記事 / 前の記事"],
  ["Enter", "記事を開く"],
  ["m", "既読にする"],
  ["o", "原文を新しいタブで開く"],
  ["/", "検索"],
  ["g", "一覧へ戻る"],
  ["?", "このヘルプ"],
];

export default function KeyboardHelp() {
  const app = useApp();
  return (
    <Show when={app.state.helpOpen}>
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
        onClick={() => app.closeHelp()}
      >
        <div
          class="w-full max-w-sm rounded-lg border border-border bg-background p-5 text-foreground shadow-lg"
          onClick={(e) => e.stopPropagation()}
        >
          <h2 class="mb-3 text-sm font-semibold">キーボードショートカット</h2>
          <ul class="space-y-2">
            <For each={ROWS}>
              {([key, desc]) => (
                <li class="flex items-center justify-between gap-4 text-sm">
                  <kbd class="rounded border border-border bg-muted px-1.5 py-0.5 text-xs">
                    {key}
                  </kbd>
                  <span class="text-muted-foreground">{desc}</span>
                </li>
              )}
            </For>
          </ul>
        </div>
      </div>
    </Show>
  );
}
```

- `Esc` で閉じるのは `KEY_BINDINGS` に `Escape: "..."` を足すのではなく、overlay 内で別途 `onKeyDown` を張るか、`useKeyboardShortcuts` の `dispatch` 冒頭で「help が開いていれば Esc/任意キーで closeHelp して return」する分岐を足してもよい（任意。最小実装は背景クリックのみ）。
- 装飾は意味トークン（`bg-background`/`border-border`/`text-muted-foreground`/`bg-muted`）のみ。生 hex・新色は持ち込まない（oklch トークン維持、土台設計 §5）。

### 6.6 状態管理・トークン方針

- 新グローバル状態は `navItems` と `helpOpen` の2つのみ。いずれも `lib/store.tsx`（既存 `createStore`）に閉じ、外部状態ライブラリは増やさない。
- `lib/api.ts` への**追加は不要**（既存 `markRead` / `getArticle` / 検索ルート遷移で足りる）。原文 URL は `navItems` 経由で持つため新 API を呼ばない。

## 7. API 契約

**新規 API なし。** 本機能が呼ぶ既存エンドポイントは以下の2つだけ（いずれも実装済み・契約変更なし）。

- `m`（既読化）: `POST /api/articles/{id}/read`
  - リクエスト: `{ "read": true }`
  - レスポンス: `204 No Content`（`api.markRead` が `http<void>` で受ける）
  - 例:
    ```http
    POST /api/articles/7b1c0d2e-2a3b-4c5d-8e9f-0a1b2c3d4e5f/read
    Content-Type: application/json

    { "read": true }
    ```
    → `204 No Content`
- `/`（検索）: クライアント内ルーティングで `/search` へ `navigate` するのみ（HTTP は `Search.tsx` 側の既存 `GET /api/search?q=...` が担う。本チケットでは検索 API を直接叩かない）。

`j/k/Enter/g`（選択移動・開く・戻る）は **HTTP を一切発生させない**（`?article` クエリの付け外しのみ。本文の取得は `ArticleDetail` の既存 `GET /api/articles/{id}` が選択変化に反応して行う）。`o`（原文）は `window.open` で外部サイトを開くだけで、自アプリの API は呼ばない。

## 8. 依存関係

- **依存する機能（このチケットが必要とするもの）**: 機能10（2ペイン/3ペインリーダーレイアウト, `Reader.tsx`・`?article` クエリ・`ArticleList.tsx`）。`j/k/Enter/g` は `?article` 駆動の選択モデルに乗るため、Reader レイアウトが前提。**実装済み**（`frontend/src/routes/Reader.tsx` 確認済み）なので追加の前提作業は不要。`dependsOn = ["10-two-pane-layout"]`。
- 弱い関連（あれば嬉しいが必須でない）:
  - 機能09（既読管理）/ 機能11（未読フィルタ）: `m` の既読化と `navItems`（フィルタ後の順序）が自然に協調する。なくても動く。
  - 全文検索（`/search`）: `/` の遷移先。`Search.tsx` が無いと `/` が空ページへ飛ぶが、本リポジトリでは実装済み。
- **このチケットがブロックする機能**: なし（純粋な操作性向上。他機能の土台ではない）。将来「検索結果での j/k」「キーバインド設定」は本 lib を拡張して載せられる（§11）。

## 9. テスト計画（TDD）

**Red → 理解 → Green の順。書いたら必ず実行する。** 本機能はフロント専用なので検証は vitest（jsdom）＋ tsc ＋ 手動。バックエンドの `scripts/test/*.sh` は不要（API 変更なし）。

### 9.1 単体テスト `frontend/src/lib/keyboard.test.ts`（新規・vitest/jsdom）

純関数 `resolveAction` / `isEditableTarget` / `stepId` を**先に**テスト（Red）。`theme.test.ts` と同じく `import { test, expect } from "vitest"` ＋ jsdom の `document.createElement` を使う。

| テスト | 対象 | 意図 |
|--------|------|------|
| `resolveAction maps every binding` | `resolveAction` | `j→next, k→prev, Enter→open, m→markRead, o→openOriginal, /→search, g→gotoList, ?→toggleHelp` を全件確認 |
| `resolveAction returns null with ctrl/meta/alt` | `resolveAction` | `{key:"j",ctrlKey:true}` 等で null（OS/ブラウザ操作を奪わない） |
| `resolveAction allows shift (for ?)` | `resolveAction` | `{key:"?",shiftKey:true}` 相当（shiftKey は型に無いので無視＝許可）で `toggleHelp` |
| `resolveAction returns null for unbound key` | `resolveAction` | `{key:"x"}` → null |
| `isEditableTarget true for input/textarea/select` | `isEditableTarget` | `document.createElement("input"|"textarea"|"select")` で true |
| `isEditableTarget true for contenteditable` | `isEditableTarget` | `div` に `isContentEditable` を立てて true（jsdom は `contentEditable="true"` 設定で反映） |
| `isEditableTarget false for button/div/null` | `isEditableTarget` | 通常要素・`null` で false（ショートカットを発火させてよい場面） |
| `stepId next/prev from middle` | `stepId` | `["a","b","c"]`, cur=`b`, dir=1→`c` / dir=-1→`a` |
| `stepId from null picks first/last` | `stepId` | cur=`null`, dir=1→先頭 / dir=-1→末尾 |
| `stepId clamps at ends` | `stepId` | cur=末尾,dir=1→末尾据え置き / cur=先頭,dir=-1→先頭据え置き |
| `stepId unknown current treated as none` | `stepId` | cur が一覧外→先頭/末尾（未選択と同扱い） |
| `stepId empty list returns null` | `stepId` | `[]`→null（移動先なし） |
| `stepId single item stays` | `stepId` | `["a"]`, cur=`a`, どちら向きでも`a` |

実行: `cd frontend && pnpm test`（vitest）。`just lint`（`pnpm typecheck` = `tsc --noEmit`）も通す。

### 9.2 手動 / 結合（Reader 上で目視）

`just front`（または `just up`）で起動し、Reader（`/`）で:
- `j`/`k`: 中央一覧の選択が上下に動き、右ペインの本文が切り替わる。端でクランプ（先頭で `k`、末尾で `j` が暴れない）。
- `Enter`: 未選択時に先頭が開く。
- `m`: 選択記事が一覧でグレーアウト（`readIds` 反映）。再読み込みでも既読（DB 反映）。
- `o`: 原文が新規タブで開く（`noopener`）。
- `/`: 検索ページへ遷移し、ブラウザのクイック検索が出ない（`preventDefault`）。
- `g`: 本文が閉じて一覧へ戻る（モバイルは master-detail で一覧表示／デスクトップは選択解除）。scope（feed/folder）が保持される。
- 入力中の抑止: 検索ボックスやフィード追加 input にフォーカス中は `j` 等がショートカット発火せず文字入力になる（`isEditableTarget`）。
- `?`（overlay 実装時）: チートシートが開閉する。
- フォルダ/フィード scope・未読フィルタ切替後も `j/k` の順序が画面の並びと一致（`navItems` 追従）。

### 9.3 型チェック

`just lint` の `pnpm typecheck`（`tsc --noEmit`）で `keyboard.ts`・`store.tsx` 追加・`ArticleList.tsx` の effect・`App.tsx` のマウント・`KeyboardHelp.tsx` の整合を確認。

## 10. 実装手順（順序付きチェックリスト）

1. ブランチを切る（例 `feat/keyboard-navigation`）。`main` 直コミットしない。**着手前に `backend/migrations/` の最新番号を確認**し、本機能が番号を消費しないことを再確認（バックエンド変更なし）。
2. `frontend/src/lib/keyboard.test.ts` を**先に**書く（§9.1、Red）。`resolveAction`/`isEditableTarget`/`stepId` の期待を列挙。
3. `frontend/src/lib/keyboard.ts` を作成（§6.1）。まず純関数 3 つを実装し `pnpm test keyboard` を Green に（hook は後でよい）。
4. `frontend/src/lib/store.tsx` に `navItems` / `helpOpen` と setter（`setNavItems`/`toggleHelp`/`closeHelp`）を追加（§6.2）。初期値・`UiStore` 型・実装の3箇所。
5. `frontend/src/lib/keyboard.ts` の `useKeyboardShortcuts()`（副作用 hook）を実装（§6.1 後半）。`useApp`/`useNavigate`/`useSearchParams`/`api.markRead` を結線。
6. `frontend/src/routes/ArticleList.tsx` に `createEffect` を1つ足し `app.setNavItems(...)` で表示順を公開（§6.3）。`createEffect` の import 追加を忘れない。行クリック・既存表示は触らない。
7. `frontend/src/App.tsx` の `<AppProvider>` 内に `<KeyboardShortcuts />` をマウントし hook を起動（§6.4）。
8. （任意）`frontend/src/components/keyboard/KeyboardHelp.tsx` を作成し `?` overlay を付ける（§6.5）。作らない場合は `KeyboardShortcuts` を `return null;` にする。
9. `just lint`（`tsc --noEmit` + clippy は無関係だが lint タスク全体）を通す。`pnpm test` 全 Green。prettier 整形。
10. `just front`（or `just up`）で起動し §9.2 の全ショートカットを目視確認（入力中抑止・端クランプ・scope 保持を特に確認）。
11. ユーザーが望むタイミングでコミット（メッセージ末尾に `Co-Authored-By` 行）。新規マイグレーション・バックエンド変更が無いことを最終確認。

## 11. リスク・未決事項・代替案

- **`navItems` の鮮度（採用＝ArticleList の effect で publish）**: `j/k` の対象順は「中央一覧がいま描画している順」と一致させる必要がある。`ArticleList` の `createEffect` で `articles()` 変化に追従して `setNavItems` するため、scope/未読フィルタ切替に同期する。**リスク**: Reader 以外のルート（`/search`・`/settings`・`/manage`）では `ArticleList` が無く `navItems` が前回値のまま残りうる。→ `j/k/Enter/o` は `navItems` を参照するが、それらのページで押しても選択は `?article` を持たないルートなので実害は小さい。気になるなら各非 Reader ルートの `onMount`/`onCleanup` で `setNavItems([])` する（任意・本チケット範囲外でよい）。
- **`/` と `?` のキー解決**: `KeyboardEvent.key` は shift 解決後の文字（`/` と `?`）を入れるため、`KEY_BINDINGS` で直接引ける。**リスク**: 一部キーボードレイアウト（非 US 配列）で `/`/`?` が別の物理キーに割り当たる。家庭内・単一ユーザ前提で許容。将来 `code`（`Slash`）ベースの解決へ切替可能（`resolveAction` のシグネチャに `code` を足すだけで純関数内に閉じる）。
- **`preventDefault` の副作用**: `j/k` のページスクロール抑止・`/` のクイック検索抑止は意図どおりだが、`Enter` を `preventDefault` するとフォーカス中ボタンの誤抑止が起こりうる。→ `isEditableTarget` で input 系は既に除外済み。ボタンフォーカス中の `Enter` まで奪わないよう、必要なら `dispatch` で `e.target` がボタンのとき `open` をスキップする分岐を足す（任意）。
- **`m` 既読後の一覧 `is_read`**: `markRead` 後に一覧 resource を再フェッチしないため、サーバの `is_read=true` は次回フェッチまで反映されない。→ `markReadLocal`（`readIds`）で見た目（グレーアウト）は即追従するので、`ArticleDetail` の自動既読と同じ既存挙動に一致。整合は取れている。
- **`window` への単一リスナの多重登録**: hook を複数箇所で呼ぶと多重登録になる。→ `<KeyboardShortcuts />` を**1箇所だけ**マウントする規約で防ぐ（§6.4）。`onCleanup` でアンマウント時に外れる。
- **チートシート overlay のフォーカストラップ**: 最小実装は背景クリックで閉じるのみで、`Tab` トラップ・初期フォーカス移動は持たない。a11y を厳密化するなら Ark UI の Dialog（`components/ui/dialog.tsx`）でラップする（任意・§6.5）。本チケットのコア機能は overlay 無しで完結する。
- **将来拡張（本 lib に載る）**: ①検索結果（`Search.tsx`）での `j/k`、②カウント/連続キー（`5j`/`gg`）、③ユーザー定義キーバインドの永続化（これだけは設定保存が要るので DB/localStorage を伴う別チケット）。いずれも `KEY_BINDINGS`/`resolveAction`/`stepId` を拡張する形で本 lib に閉じて追加できる。
