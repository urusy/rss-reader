# 10 二ペインのリーダーレイアウト

## 1. 概要

現状のフロントエンドは単一カラム（`App.tsx` の `<main class="mx-auto max-w-3xl">`）で、記事一覧（実体は `routes/FeedList.tsx`）と記事本文（`routes/ArticleView.tsx`）を縦に切り替えるだけの構成になっている。本機能は、これを**二ペインのリーダーレイアウト**へ再構成する。左ペインにフィード/フォルダのナビゲーション（Sidebar）、右ペインに記事一覧または記事本文を表示する。

これは 11 機能の中で**最大の UX 変更**であり、以降のフロント機能（フォルダ #02、すべて/未読トグル #11、フィード追加の配置 #08、フィード管理 #01、ダークテーマ #04、Instapaper #05、既読管理 #09）が載るシェルとルーティングの土台を提供する。

選択状態（選択中のフォルダ/フィード/記事）は **URL を正**とし、`@solidjs/router` のルートパラメータから導出する（別ストアに二重管理しない）。リロード・戻る/進む・LAN 内での URL 共有が自然に効く。レスポンシブでは `md` 未満を単一カラムに退避し、Sidebar は既存 `components/ui/dialog.tsx`（Ark UI Dialog）をドロワー化して表示する。**バックエンド変更・マイグレーションは一切不要**（選択状態は端末ローカルな「見え方」であり DB に持たない）。

## 2. スコープ / 非スコープ

### 含む（本機能が所有する）
- `App.tsx` を二ペインのアプリシェルへ再構成（`AppProvider` + 左 Sidebar + 右 `main`）。
- `index.tsx` のルーティング再構成（root レイアウト維持 + `path` 配列ルート + catch-all プレースホルダルート）。
- 最小グローバルストア `src/lib/store.tsx`（`AppProvider` / `useApp`）。**本機能が所有するのは `sidebarOpen`（モバイルドロワー開閉）と `feeds`（Sidebar が2箇所で共有するフィード一覧リソース）**。`theme`(#04) / `filter`(#11) / `counts`(#09) を後続が追記できる拡張口を残す。
- 選択導出ユーティリティ `src/lib/selection.ts`（`useSelection()`：URL → `Scope`）。純粋関数 `scopeFromPath()` を分離する。
- レイアウト用コンポーネント: `src/components/layout/Sidebar.tsx`、`src/components/layout/SidebarContent.tsx`（デスクトップ aside とモバイルドロワーで共有）、`src/components/layout/MobileTopBar.tsx`。
- `src/components/ui/dialog.tsx` に `side="left"` 変種を**追記**（既存 center 利用は無改変で温存）→ モバイル左ドロワー。
- `src/routes/FeedList.tsx` を `src/routes/ArticleList.tsx` に**改名**し、`useSelection()` の scope（all/feed）に応じて `api.listArticles({ feed_id })` を呼ぶよう改修。一覧 UI を罫線区切りリストへ寄せる（§6.9 デザイン）。
- `src/routes/NotFound.tsx`（catch-all 用「準備中」プレースホルダ。未実装ルートのデッドリンク回避 + 恒久的な 404 表示）。
- 純粋関数のユニットテスト導入: **devDependency に `vitest` を一度だけ追加**し、`src/lib/selection.test.ts` で `scopeFromPath()` を実テストする（TDD 原則の遵守）。
- Sidebar の**フィードナビは当面 `useApp().feeds()` のフラット一覧**（クリックで `/feeds/:feedId` へ）。フォルダツリーへ差し替えるシームを用意。
- アプリ内ナビゲーションは **`@solidjs/router` の `<A>`** に統一（プレーン `<a>` は全ページリロードになり、Sidebar 永続の前提が壊れるため）。

### 含まない（他機能が所有 / 本機能はシームのみ用意）
- フォルダツリー `FeedTree.tsx`・`/folders/:folderId` の**データ取得**・`folder_id` フィルタの**バックエンドと `api.ts` 型拡張**（→ #02 feed-folders）。本機能では `/folders/:folderId` の**ルート文字列のみ**先行配置し、folder scope では**リクエストを発行せずプレースホルダを描画**する（理由は §3・§5・§6.5）。
- すべて/未読トグル UI と `filter` ストアフィールド（→ #11 unread-filter-toggle）。
- フィード追加 UI の配置（Sidebar 下部 or Dialog 起動）（→ #08 feed-add-placement）。本機能では `ArticleList` から追加 input を**撤去しない**（#08 が移設するまで現状維持）。
- 管理画面 `/manage`（→ #01）、設定 `/settings`・テーマトグル（→ #04/#05）。本機能は Sidebar からのリンクのみ用意し、ルート本体は各機能が追加する（未追加の間は catch-all プレースホルダが表示される）。
- 未読数バッジ・グローバル未読数リソース（→ #09/#03）。
- 3 ペイン（`xl:` で一覧+本文同時表示）。本機能では grid を将来拡張できる形にするのみで、初期は**2 ペイン確定**（右ペインは一覧か本文のどちらか）。

## 3. 既存実装の調査と再利用

実ファイルを確認済み。以下を再利用し、車輪の再発明をしない。

- **`src/index.tsx`**: 既に `Router root={App}` で `App` をルートレイアウトとして使用済み（現在は `<Route path="/" component={FeedList} />` と `<Route path="/articles/:id" component={ArticleView} />` のみ）。`path` 配列追加と import 差し替えだけで足りる（root 機構は既存）。
- **`src/App.tsx`**: 既存シェル（`min-h-screen bg-background text-foreground` + header + `<main class="mx-auto max-w-3xl px-4 py-6">{props.children}</main>`）。読書幅 `max-w-3xl` を右ペインで踏襲する。`<a href="/">RSS Reader</a>` のアプリ名表示は Sidebar 上部へ移設する。
- **`src/components/ui/dialog.tsx`**: Ark UI `@ark-ui/solid/dialog` を `Portal` + トークンでラップ済み（`Backdrop`/`Positioner`/`Content` の center 固定）。`DialogContent` は現在 `splitProps(props, ["class", "children"])`。**新ライブラリ不要**で `side` prop を追記すればモバイルドロワーになる。`Dialog`(=`ArkDialog.Root`) は `open`/`onOpenChange` で制御できる（**正確な API は実装時に ark-ui.com で要確認**、§11）。
- **`src/components/ui/button.tsx`**: `cva` ベース、`variant`（default/outline/ghost/destructive）・`size`（default/sm/icon）あり。ハンバーガー = `<Button variant="ghost" size="icon">`。`focus-visible:ring-2 ring-ring` を踏襲している。
- **`src/app.css`**: oklch デザイントークン（`bg-background`/`text-muted-foreground`/`border-border`/`bg-accent`/`text-accent-foreground` 等）と `@custom-variant dark`・`.dark` ブロック・`@theme inline` が**配線済み**。新色は持ち込まず意味トークンのみで装飾。
- **`src/lib/api.ts`**: 型付き fetch クライアント。現行シグネチャは `listArticles(params?: { feed_id?: string; unread?: boolean })` で **`folder_id` 引数を持たない**。本機能は既存 `listFeeds()` / `listArticles({ feed_id? })` / `getArticle(id)` を**そのまま消費**し、`api.ts` は**一切変更しない**。`folder_id` 引数の追加は #02 のスコープ。
- **`src/routes/FeedList.tsx`**: 実体は記事一覧。`createResource(() => api.listArticles())` + `<For>` でカード羅列。記事リンクは現状プレーン `<a href={`/articles/${a.id}`}>`（＝全ページリロード）。→ `ArticleList.tsx` へ改名し、scope 連動 + `<A>` 化 + 罫線リストへ改修する。
- **`src/routes/ArticleView.tsx`**: `useParams().id` + `api.getArticle` で 1 記事表示。本機能では**無改変**（自動既読 #09・後で読む #06 は別機能）。右ペインのルート先として再利用。
- **`src/lib/utils.ts`**: `cn()`（clsx + tailwind-merge）。全コンポーネントで使用。
- **バックエンド `articles` ハンドラ（`backend/src/features/articles/handler.rs`）**: `ListQuery { feed_id: Option<Uuid>, unread: bool }`。**`deny_unknown_fields` を持たない**ため、未知の `?folder_id=` は無視され、結果は「空」ではなく**全記事（LIMIT 200 = all scope と同じ）**が返る。→ #02 完了まで folder_id は**送らない**（§5・§6.5）。
- **`@solidjs/router` 0.16 / 公式ドキュメント（docs.solidjs.com/solid-router）で裏取り済みの API**:
  - root レイアウト: `Router root={Layout}`。`Layout` は `props.children` に現ルートを描画する。
  - `path` 配列: `<Route path={["/", "/feeds/:feedId", "/folders/:folderId"]} component={ArticleList} />` で、これらの間を遷移しても**コンポーネントは再マウントされない**（"matched routes remain mounted"。一覧 DOM を保持しつつ source 変化で再フェッチできる）。
  - `<A href activeClass inactiveClass end>` でアクティブリンク装飾。`end` を付けないと `/` リンクが全ネストパスでアクティブ判定される。
  - catch-all: `<Route path="*" component={NotFound} />`。より具体的なパスが優先されるため、後続機能が `/manage` 等を追加すれば catch-all より優先される。
  - `useParams` / `useLocation` で URL から状態を導出。

## 4. データモデルとマイグレーション

**DB 変更なし。** 選択状態（選択中フォルダ/フィード/記事）・サイドバー開閉・ツリー展開は「この端末の見え方」であり、土台設計（§4 クライアント側状態の線引き）に従い **DB に持たない**（URL + signal/createContext）。本機能で新規テーブル・カラム・マイグレーションファイルは追加しない。

## 5. バックエンド設計

**バックエンド変更なし。** 新スライス・既存スライス拡張・`features/mod.rs` への `.merge()` 追加は一切ない。本機能は既存エンドポイントのみ消費する:

- `GET /api/feeds`（Sidebar のフラットフィード一覧）
- `GET /api/articles?feed_id=<uuid>`（feed scope の一覧）
- `GET /api/articles`（all scope の一覧）
- `GET /api/articles/{id}`（記事本文。`ArticleView` で使用）

> **folder scope の扱い（重要・レビュー指摘の反映）**: `/folders/:folderId` scope の一覧（`folder_id` での絞り込み）と `folder_id` クエリのバックエンド対応は **#02 feed-folders** が担当する。本機能はルート文字列と UI シームを先に用意するだけで、folder scope では**バックエンドへ一切リクエストを送らない**。
> 理由: 現行 `articles` ハンドラの `Query<ListQuery>` は `deny_unknown_fields` を持たないため、未対応の `?folder_id=xyz` を送っても**無視され、空ではなく全記事が返る**（all scope と区別がつかない誤誘導になる）。したがって本機能では folder scope のとき `createResource` の source を `false` にして fetcher を呼ばせず、右ペインには「#02 で対応予定」のプレースホルダを描画する（§6.5）。folder のデータ経路は #02 へ完全に切り出す。

## 6. フロントエンド設計

### 6.1 ルーティング（`src/index.tsx`）
`Router root={App}` を維持し、右ペインのルートを再構成する。`path` 配列で `ArticleList` を all/feed/folder scope 間で**マウント維持**する。catch-all で未実装ルートのデッドリンクを回避する。

```tsx
/* @refresh reload */
import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import "./app.css";
import App from "./App";
import ArticleList from "./routes/ArticleList";
import ArticleView from "./routes/ArticleView";
import NotFound from "./routes/NotFound";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");

render(
  () => (
    <Router root={App}>
      <Route path={["/", "/feeds/:feedId", "/folders/:folderId"]} component={ArticleList} />
      <Route path="/articles/:id" component={ArticleView} />
      {/* /manage(#01)・/settings(#04/#05) は各機能が追加。未追加の間は下の catch-all が表示される */}
      <Route path="*" component={NotFound} />
    </Router>
  ),
  root,
);
```

設計判断: 右ペインは**一覧か本文のどちらか**を表示する 2 ペイン。記事クリックで `/articles/:id` に遷移し右ペインが本文に切り替わる。左 Sidebar は root レイアウト内にあるため**再マウントされず永続**（スクロール・将来のツリー開閉を保持）。`xl:` で一覧+本文の 3 ペインに広げる拡張は本機能のグリッドで将来可能にするが、初期スコープ外。

### 6.2 アプリシェル（`src/App.tsx`）
`AppProvider` でラップし、`md` 以上で二ペイングリッド、未満で単一カラム + モバイルトップバー。

```tsx
import type { ParentComponent } from "solid-js";
import { AppProvider } from "@/lib/store";
import Sidebar from "@/components/layout/Sidebar";
import MobileTopBar from "@/components/layout/MobileTopBar";

const App: ParentComponent = (props) => (
  <AppProvider>
    <div class="min-h-screen bg-background text-foreground md:grid md:grid-cols-[clamp(220px,22vw,300px)_1fr]">
      <Sidebar />                                  {/* hidden md:flex, sticky h-screen */}
      <div class="flex min-h-screen min-w-0 flex-col">
        <MobileTopBar />                           {/* md:hidden、ハンバーガー */}
        <main class="mx-auto w-full max-w-3xl flex-1 px-4 py-6">{props.children}</main>
      </div>
    </div>
  </AppProvider>
);

export default App;
```

- `min-w-0` で長いタイトルによるグリッド破綻を防ぐ。
- 読書幅 `max-w-3xl` を踏襲。
- `grid-cols-[clamp(220px,22vw,300px)_1fr]` が Tailwind v4 の arbitrary value で `clamp()` 内カンマにより破綻する稀な環境では、`md:grid-cols-[280px_1fr]` を確実なフォールバックとして用いる。

### 6.3 グローバルストア（`src/lib/store.tsx`、新規）
`createContext` + `createStore` の最小ストア。本機能が所有するのは **`sidebarOpen`** と **`feeds` リソース**。`theme`(#04)/`filter`(#11)/`counts`(#09) の追記口を残す。`App`（Router root）直下に Provider を置くので、Provider 配下から route hooks が使える。

`feeds` をストアに持つ理由（レビュー指摘の反映）: Sidebar の中身（`SidebarContent`）は**デスクトップ aside（`hidden md:flex` で常時マウント）とモバイルドロワー（Dialog 内）の2箇所**に置かれる。各々が個別に `createResource(() => api.listFeeds())` を持つと、CSS の `hidden` はアンマウントしないため**フィード一覧が二重フェッチ**されうる。`feeds` を 1 つのリソースとしてストアへ持ち上げ、両 `SidebarContent` が共有することで**単一フェッチ**に集約する。将来 #08（追加後の再取得）や #09（per-feed 未読数の `FeedStat` への発展）のフックもここに集まる。

```tsx
import {
  createContext, useContext, createResource,
  type ParentComponent, type Resource,
} from "solid-js";
import { createStore } from "solid-js/store";
import { api, type Feed } from "@/lib/api";

export interface UiState {
  sidebarOpen: boolean;        // モバイルドロワー
  // theme: "light" | "dark";   ← #04 が追記
  // filter: "all" | "unread";  ← #11 が追記
}

export interface UiStore {
  state: UiState;
  openSidebar(): void;
  closeSidebar(): void;
  toggleSidebar(): void;
  feeds: Resource<Feed[]>;     // Sidebar が2箇所で共有する単一リソース
  refetchFeeds(): void;        // #08 のフィード追加後などに呼ぶ
  // setTheme/toggleTheme(#04), setFilter(#11), counts(#09) を追記
}

const Ctx = createContext<UiStore>();

export const AppProvider: ParentComponent = (props) => {
  const [state, setState] = createStore<UiState>({ sidebarOpen: false });
  const [feeds, { refetch }] = createResource(() => api.listFeeds(), { initialValue: [] });

  const store: UiStore = {
    state,
    openSidebar: () => setState("sidebarOpen", true),
    closeSidebar: () => setState("sidebarOpen", false),
    toggleSidebar: () => setState("sidebarOpen", (v) => !v),
    feeds,
    refetchFeeds: () => { void refetch(); },
  };
  return <Ctx.Provider value={store}>{props.children}</Ctx.Provider>;
};

export function useApp(): UiStore {
  const v = useContext(Ctx);
  if (!v) throw new Error("useApp must be used within <AppProvider>");
  return v;
}
```

公開アクションは薄い setter に限定し、生 `setState` は外に出さない。`initialValue: []` で `feeds()` は常に `Feed[]` を返す（消費側は `feeds.loading` でスケルトン制御）。

### 6.4 選択の導出（`src/lib/selection.ts`、新規）
URL を正として scope を導出。純粋関数を分離してテスト可能にする。

```tsx
import { createMemo } from "solid-js";
import { useLocation, useParams } from "@solidjs/router";

export type Scope =
  | { kind: "all" }
  | { kind: "feed"; feedId: string }
  | { kind: "folder"; folderId: string };

/** 純粋関数（vitest 対象）。URL pathname と params から scope を決める。 */
export function scopeFromPath(pathname: string, params: Record<string, string>): Scope {
  if (pathname.startsWith("/feeds/") && params.feedId) return { kind: "feed", feedId: params.feedId };
  if (pathname.startsWith("/folders/") && params.folderId) return { kind: "folder", folderId: params.folderId };
  return { kind: "all" };
}

export function useSelection(): () => Scope {
  const loc = useLocation();
  const params = useParams();
  return createMemo(() => scopeFromPath(loc.pathname, params));
}
```

### 6.5 記事一覧（`src/routes/ArticleList.tsx`、`FeedList.tsx` を改名・改修）
scope を `createResource` の source にして自動再フェッチ。**`api.listArticles` には `feed_id` だけを渡す**（現行シグネチャと完全一致＝`tsc --noEmit` を通す）。**folder scope ではリクエストを送らず**（source が `false` を返すと fetcher は呼ばれない）、プレースホルダを描画する。`filter`(#11)・`folder_id`(#02) は後続が source/fetcher に合成する。

```tsx
import { createResource, For, Show } from "solid-js";
import { A } from "@solidjs/router";
import { api, type Article } from "@/lib/api";
import { useSelection, type Scope } from "@/lib/selection";
import { cn } from "@/lib/utils";

export default function ArticleList() {
  const scope = useSelection();

  // all / feed のみ取得。folder は #02 まで未対応:
  // source が false を返すと fetcher が呼ばれず、リクエストを送らない。
  // （未対応の folder_id をバックエンドへ送っても deny_unknown_fields 未設定のため
  //   無視され "全記事" が返ってしまうため、送らない。§5 参照）
  const [articles] = createResource<Article[], Scope>(
    () => {
      const s = scope();
      return s.kind === "folder" ? false : s;
    },
    (s) =>
      api.listArticles({
        feed_id: s.kind === "feed" ? s.feedId : undefined,
        // unread: ui.filter === "unread" ? true : undefined  ← #11 が source/fetcher に合成
      }),
  );

  return (
    <Show
      when={scope().kind !== "folder"}
      fallback={
        <p class="py-12 text-center text-sm text-muted-foreground">
          フォルダ表示は #02（feed-folders）で対応予定です。
        </p>
      }
    >
      {/* 既存 add-feed input は #08 が移設するまで現状維持（撤去しない）。
          ※プレーン <a> ではなく <A> を使い、Sidebar を再マウントさせない SPA 遷移にする */}
      <Show
        when={!articles.loading}
        fallback={<p class="text-sm text-muted-foreground">読み込み中…</p>}
      >
        <Show
          when={(articles()?.length ?? 0) > 0}
          fallback={<p class="text-sm text-muted-foreground">記事がありません。</p>}
        >
          <div class="divide-y divide-border">
            <For each={articles()}>
              {(a) => (
                <A href={`/articles/${a.id}`} class="block py-3 hover:bg-accent">
                  <p class={cn("text-sm font-medium", a.is_read && "font-normal text-muted-foreground")}>
                    {a.title}
                  </p>
                  <Show when={a.summary}>
                    <p class="line-clamp-1 text-sm text-muted-foreground">{a.summary}</p>
                  </Show>
                  {/* メタ（投稿日時等）は text-xs text-muted-foreground。整形は #03/#07 と整合 */}
                </A>
              )}
            </For>
          </div>
        </Show>
      </Show>
    </Show>
  );
}
```

> 重要: **`api.listArticles` に渡すのは `feed_id` のみ**。`folder_id` は `api.ts` の型に存在せず（§3）、渡すと TS2353（余剰プロパティ）で `tsc --noEmit` が落ちる。folder のデータ経路は #02 で `api.ts` 型拡張・バックエンド対応・FeedTree とともに有効化する。本機能単独でも all/feed scope は完全に機能する。

### 6.6 Sidebar（`src/components/layout/Sidebar.tsx` / `SidebarContent.tsx`、新規）
- `Sidebar.tsx`: デスクトップ用 aside ラッパ。`<aside class="hidden md:flex md:flex-col sticky top-0 h-screen overflow-y-auto border-r border-border">` の中に `<SidebarContent />`。
- `SidebarContent.tsx`: 中身（デスクトップ aside とモバイルドロワーで**共有**）。`useApp()` から `feeds`（共有リソース）を読む。構成:
  1. 上部: アプリ名（旧 `App.tsx` header から移設、`text-lg font-semibold tracking-tight`）。
  2. フィルタ slot（#11 が `segmented` を挿入。本機能はコメントのプレースホルダ枠）。
  3. ナビ:
     - 「すべての記事」リンク `<A href="/" end activeClass="bg-accent text-accent-foreground">`。**`end` は必須**（無いと全ネストパスでアクティブ判定される）。
     - **フラットなフィード一覧**: `<For each={useApp().feeds()}>` → `<A href={`/feeds/${f.id}`} class="block h-8 px-2 rounded-md text-sm hover:bg-accent" activeClass="bg-accent text-accent-foreground">`。**このナビ部は #02 が `FeedTree.tsx`（フォルダ→フィード + 未読バッジ）に差し替えるシーム。**
  4. 下部 footer: フィード追加ボタンの置き場（#08）、`/manage`（#01）・`/settings`（#04/#05）への `<A>` リンク、テーマトグルの置き場（#04）。未実装ルートは catch-all（§6.1）の「準備中」表示になるため、デッドリンクで右ペインが空白にならない。
- `SidebarContent` は `onNavigate?: () => void` を受け取り、各 `<A>` の `onClick` で呼ぶ（モバイルではドロワーを閉じるために `closeSidebar` を渡す）。

### 6.7 モバイル退避（`MobileTopBar.tsx` + `dialog.tsx` の side 変種）
- `MobileTopBar.tsx`: `<header class="md:hidden flex items-center gap-2 border-b border-border px-4 py-3">`。`<Button variant="ghost" size="icon" onClick={() => app.openSidebar()}>`（ハンバーガー、`lucide-solid` 採用なら `Menu` アイコン、未採用なら文字「≡」）+ アプリ名。さらに `Dialog` を内包し、`open={app.state.sidebarOpen}` で制御、中身に `<DialogContent side="left"><SidebarContent onNavigate={app.closeSidebar} /></DialogContent>`。
- `dialog.tsx` 追記（既存 center 利用は無改変、`side` 省略時 center）:

```tsx
export function DialogContent(
  props: ComponentProps<typeof ArkDialog.Content> & { side?: "center" | "left" },
) {
  const [local, rest] = splitProps(props, ["class", "children", "side"]);
  const side = () => local.side ?? "center";
  return (
    <Portal>
      <ArkDialog.Backdrop class="fixed inset-0 z-50 bg-black/50" />
      <ArkDialog.Positioner
        class={cn(
          "fixed inset-0 z-50 flex",
          side() === "left" ? "items-stretch justify-start" : "items-center justify-center p-4",
        )}
      >
        <ArkDialog.Content
          class={cn(
            side() === "left"
              ? "h-full w-72 max-w-[85%] overflow-y-auto rounded-none border-r border-border bg-background p-4 shadow-lg"
              : "w-full max-w-md rounded-lg border border-border bg-background p-6 shadow-lg",
            local.class,
          )}
          {...rest}
        >
          {local.children}
        </ArkDialog.Content>
      </ArkDialog.Positioner>
    </Portal>
  );
}
```

> Ark UI Dialog の制御 props（`open` / `onOpenChange` の detail 形、`Dialog.Root` の正確な部品名）は**実装時に ark-ui.com（Solid / Dialog, v5）で要確認**。想定: `<Dialog open={app.state.sidebarOpen} onOpenChange={(d) => (d.open ? app.openSidebar() : app.closeSidebar())}>`。フルハイト左ドロワーに転用する際、中央ダイアログ前提のフォーカストラップ/スクロールロック/Escape 閉じが過剰でないか確認し、必要なら `trapFocus` 等を調整する。

### 6.8 catch-all プレースホルダ（`src/routes/NotFound.tsx`、新規）
未実装ルート（`/manage`・`/settings` など各機能着地前）と本来の不明パスを、空白でなく友好的な表示にする。後続機能が具体ルートを追加すれば自動的にそちらが優先される（§6.1）。

```tsx
export default function NotFound() {
  return (
    <div class="py-12 text-center text-sm text-muted-foreground">
      この画面はまだありません（準備中）。
    </div>
  );
}
```

### 6.9 必要な Ark UI / 外部部品
- 本機能で**新規に必要なのは既存 `dialog.tsx` の `side` 変種のみ**（追加ライブラリ不要）。
- `switch`/`segmented`/`tree-view`/`select`/`dropdown-menu`/`badge` は後続機能（#04/#11/#02/#01/#09）が追加。本機能はそれらの**置き場（slot）**だけ用意。
- アイコン用 `lucide-solid` の採否は土台で「推奨・要決定」。本機能はハンバーガー 1 箇所のみで、未導入なら文字「≡」で代替可（採用時に差し替え）。

### 6.10 装飾（トークン）
- 選択中ナビ項目: `bg-accent text-accent-foreground`（`<A activeClass>`）。ホバー: `hover:bg-accent`。
- 罫線: `border-border` 1px。Sidebar 区切り `border-r`。一覧 `divide-y divide-border` + `py-3`。
- タイトル `text-sm font-medium`（既読は `font-normal text-muted-foreground`）、メタ `text-xs text-muted-foreground`。
- フォーカス: 全インタラクティブ要素で `focus-visible:ring-2 focus-visible:ring-ring` を維持（既存 `Button` が踏襲）。
- 生 hex 不使用、意味トークンのみ。新色は持ち込まない。

## 7. API 契約

**追加・変更するエンドポイントなし。** 既存契約を消費するのみ。`lib/api.ts` への新メソッド/型変更も**本機能ではなし**（`folder_id` 引数・`listFolders`・未読数は #02/#09）。

| 用途 | 既存エンドポイント | 備考 |
|------|--------------------|------|
| Sidebar フィードナビ | `GET /api/feeds` → `Feed[]` | フラット一覧。ストアで一元取得。#02 がツリー化 |
| all scope 一覧 | `GET /api/articles` → `Article[]` | |
| feed scope 一覧 | `GET /api/articles?feed_id=<uuid>` → `Article[]` | 既存対応済み |
| 記事本文 | `GET /api/articles/{id}` → `Article` | `ArticleView` で使用（無改変） |
| folder scope 一覧 | （送らない） | #02 まで未対応。本機能はリクエストを発行しない |

## 8. 依存関係

- **ハード依存: なし。** 既存 API のみで「二ペインシェル + フラットフィードナビ + feed/all scope 一覧 + 記事本文」が動作し、単独で出荷できる。
- **本機能がブロックする（前提となる）機能**:
  - #02 feed-folders（Sidebar の `FeedTree` 差し替え口、`/folders/:folderId` のデータ有効化、`api.listArticles` の `folder_id` 引数、`useSelection` の folder scope）。
  - #11 unread-filter-toggle（Sidebar の filter slot、`ArticleList` resource source への filter 合成、store の `filter` フィールド）。
  - #08 feed-add-placement（`ArticleList` からの追加 input 撤去 + Sidebar/Dialog への移設、`refetchFeeds()` の利用）。
  - #01 feed-management（`/manage` ルート・Sidebar 行アクション）。
  - #04 dark-theme（store の `theme`・Sidebar footer のトグル）。
  - #05 instapaper（`/settings` ルート）。
  - #09 read-management（store の `counts`・Sidebar 未読バッジ）。
- **関連（協調するが順序自由）**: #07 minimal-design（§6.10 のデザイン指針と整合）。
- 推奨着手順: 本機能（土台シェル）→ #04/#11（store フィールド追加が小さい）→ #02（FeedTree + folder_id）→ #08/#01/#05/#09。

## 9. テスト計画（TDD）

純粋関数 `scopeFromPath()` は **vitest を導入して実テスト**する（プロジェクト MEMORY の TDD 原則に従う。Red→理解→Green）。コンポーネント/DOM テスト（jsdom + `@solidjs/testing-library`）は本機能では導入せず、§9.3 の手動チェックで代替する（将来の拡張余地として記す）。

### 9.1 ツール導入（一度だけ）
- `package.json` の devDependencies に `vitest` を追加。
- scripts に `"test": "vitest run"`、`"test:watch": "vitest"` を追加（`build` の `tsc --noEmit && vite build` は変更しない）。
- vitest は既存 `vite.config` / `vite-plugin-solid` を再利用する。`selection.test.ts` は純粋関数のみを対象とするため jsdom 不要。テストは `import { describe, it, expect } from "vitest"` の明示 import を使い、`tsconfig` 変更を不要にする。

### 9.2 単体テスト（`src/lib/selection.test.ts`、Red を先に書く）
```ts
import { describe, it, expect } from "vitest";
import { scopeFromPath } from "./selection";

describe("scopeFromPath", () => {
  it("ルートは all", () => {
    expect(scopeFromPath("/", {})).toEqual({ kind: "all" });
  });
  it("/feeds/:feedId は feed", () => {
    expect(scopeFromPath("/feeds/abc", { feedId: "abc" })).toEqual({ kind: "feed", feedId: "abc" });
  });
  it("/folders/:folderId は folder", () => {
    expect(scopeFromPath("/folders/xyz", { folderId: "xyz" })).toEqual({ kind: "folder", folderId: "xyz" });
  });
  it("記事本文表示中は all 扱い（一覧 scope に影響しない）", () => {
    expect(scopeFromPath("/articles/1", { id: "1" })).toEqual({ kind: "all" });
  });
  it("不明パスは all にフォールバック", () => {
    expect(scopeFromPath("/manage", {})).toEqual({ kind: "all" });
  });
});
```
意図: URL → 選択状態の導出が一意・無曖昧であること、未対応/不明パスが安全に all へ落ちることを担保する。

### 9.3 型ゲート（必須・CI 相当）
- `pnpm typecheck`（`tsc --noEmit`）が通る。とくに **`ArticleList` が `api.listArticles` に `feed_id` 以外を渡していない**こと（TS2353 が出ない）、`useApp()` の戻り型・`Scope` ユニオンの網羅・`DialogContent` の `side` prop が型安全であることを確認する。
- `pnpm test`（vitest）が緑であること。

### 9.4 手動 E2E チェックリスト（観察）
1. `/` 表示で左 Sidebar（フィード一覧）+ 右に全記事一覧（罫線リスト）が並ぶ。
2. Sidebar のフィードをクリック → `/feeds/:feedId` に遷移、右一覧がそのフィードの記事に絞られる。アクティブ項目が `bg-accent` で強調。
3. 「すべての記事」リンクは `/feeds/:feedId` 表示中に**非アクティブ**（`end` が効いている）。
4. 一覧の記事をクリック → `/articles/:id`、右ペインが本文に切り替わる。Sidebar は再マウントされない（スクロール位置保持＝`<A>` による SPA 遷移）。
5. ブラウザの戻る → 直前の一覧（scope 付き）に復帰。URL とビューが一致。
6. `/feeds/:feedId` をブラウザで直接リロード → 同じ絞り込み状態が復元。
7. `/folders/<任意>` を直接開く → 右ペインに「#02 で対応予定」プレースホルダ。**ネットワークタブで `/api/articles` へのリクエストが発行されていない**こと（folder_id を送らない検証）。
8. `/manage` や `/settings` を開く → 右ペインに「準備中」プレースホルダ（空白でない）。
9. ウィンドウを `md` 未満に縮小 → Sidebar が消え、`MobileTopBar` のハンバーガー表示。タップで左ドロワー（Dialog `side="left"`）が開く。
10. ドロワー内のフィードをタップ → 遷移し、ドロワーが自動で閉じる（`closeSidebar`）。
11. ネットワークタブで `/api/feeds` が **1 回だけ**呼ばれている（ストア集約＝二重フェッチなし）こと。
12. ダーク（`document.documentElement.classList.add("dark")` を一時手動付与）で Sidebar・一覧・ドロワーがトークンに追従。
13. 既存 `dialog.tsx` の center 利用箇所（将来の確認ダイアログ等）が `side` 省略で従来通り中央表示（リグレッションなし）。

## 10. 実装手順（順序付きチェックリスト）

1. `package.json` に devDependency `vitest` と scripts `test`/`test:watch` を追加し、`pnpm install`。
2. `src/lib/selection.ts` を新規作成（`Scope`、`scopeFromPath()`、`useSelection()`）。続けて `src/lib/selection.test.ts` を作成し、§9.2 のケースを記述 → `pnpm test` が緑になることを確認（Red→Green）。
3. `src/lib/store.tsx` を新規作成（`UiState`/`UiStore`/`AppProvider`/`useApp`。`sidebarOpen` + open/close/toggle、`feeds` リソース + `refetchFeeds`）。
4. `src/components/ui/dialog.tsx` の `DialogContent` に `side?: "center" | "left"` を追記（§6.7、既存 center は省略時挙動で温存）。
5. `src/components/layout/SidebarContent.tsx` を新規作成（アプリ名 + フィルタ slot コメント + 「すべての記事」`<A href="/" end>` + `useApp().feeds()` のフラット一覧 `<A href={`/feeds/${id}`}>` + footer リンク slot。`onNavigate?` を各 `<A>` の onClick で呼ぶ）。**すべての記事リンクには必ず `end` を付与**し、フィード/記事リンクはすべて `<A>`（プレーン `<a>` 不可）にする。
6. `src/components/layout/Sidebar.tsx` を新規作成（`hidden md:flex md:flex-col sticky top-0 h-screen overflow-y-auto border-r` の aside で `SidebarContent` をラップ）。
7. `src/components/layout/MobileTopBar.tsx` を新規作成（`md:hidden`、ハンバーガー `Button` で `openSidebar`、`Dialog open={state.sidebarOpen} onOpenChange=...` + `DialogContent side="left"` 内に `SidebarContent onNavigate={closeSidebar}`）。
8. `src/routes/NotFound.tsx` を新規作成（§6.8 の「準備中」プレースホルダ）。
9. `src/App.tsx` を二ペインシェルへ書き換え（`AppProvider` + grid + `Sidebar` + `MobileTopBar` + `main max-w-3xl`）。旧 header のアプリ名は `SidebarContent`/`MobileTopBar` へ移設。
10. `src/routes/FeedList.tsx` を `src/routes/ArticleList.tsx` に改名し、§6.5 のとおり `useSelection()` を source にした `createResource` へ改修（folder scope は fetch せずプレースホルダ、`feed_id` のみ渡す、記事リンクを `<A>` + 罫線リスト化、add-feed input は #08 まで現状維持）。
11. `src/index.tsx` を更新（import を `ArticleList`/`NotFound` に、`<Route path={["/", "/feeds/:feedId", "/folders/:folderId"]} component={ArticleList} />` + `<Route path="/articles/:id" component={ArticleView} />` + `<Route path="*" component={NotFound} />`）。
12. `pnpm typecheck`（= `just lint` の tsc）と `pnpm test` を通す。
13. §9.4 の手動チェックリストでデスクトップ/モバイル両方を確認（とくに 7=folder で fetch なし、11=`/api/feeds` 単一フェッチ、13=center リグレッションなし）。
14. ark-ui.com（Solid / Dialog, v5）で `Dialog.Root` の `open`/`onOpenChange` の正確な API を確認し、必要なら §6.7 の制御コードを微修正。

## 11. リスク・未決事項・代替案

- **Ark UI Dialog の制御 API（要確認）**: `open` / `onOpenChange(details)` の detail 形（`details.open`）と part 名はバージョン差がある。実装時に ark-ui.com（Solid / Dialog, v5）で確認し、`dialog.tsx` の `side` 変種が既存 center を壊さないことを手動回帰で担保する（§9.4-13）。フルハイト左ドロワーでフォーカストラップ/スクロールロックが過剰なら `trapFocus` 等の props を調整。
- **2 ペインの記事クリック時に一覧 DOM がアンマウントされる**: 右ペインが list↔body の排他ルートのため、本文を開くと一覧コンポーネントが外れ、戻ると再フェッチ + 一覧内スクロールはリセットされうる（`path` 配列はあくまで all/feed/folder scope 間の維持で、`/articles/:id` は別ルート）。許容範囲。UX 改善が要れば将来 `xl:` 3 ペイン（一覧を常設カラム化し本文を第 3 カラムに）へ拡張できるよう、grid は `1fr` 右ペインを `xl:grid-cols-[..._minmax(0,1fr)_minmax(0,2fr)]` 等へ差し替えやすい構造にしておく。
- **`folder_id` scope のデータ未対応**: `/folders/:folderId` は #02 完了まで「準備中」プレースホルダ + **fetch なし**。folder ナビ自体（FeedTree）も #02 が描画するため、本機能単独では folder scope へ到達する UI 経路は存在せず（フラット一覧のみ）、手動 URL 直打ちでのみ到達する。バックエンドへ未対応クエリを送らない設計のため、誤って全記事が返る誤誘導も起きない。
- **`<A>` への統一が必須**: 既存 `FeedList`/`App` はプレーン `<a>` を使っており全ページリロードを起こす。二ペインで Sidebar を永続させるには SPA 遷移（`<A>`）が前提。改修時に取りこぼすと「Sidebar が毎回再マウントされる」回帰になるため、§9.4-4 で必ず確認する。
- **`feeds` をストアへ持ち上げる判断**: Sidebar が2箇所に出る本機能特有の事情（CSS `hidden` はアンマウントしない）に対する単一フェッチ化。土台の「最小ストア」方針からの逸脱は限定的で、#08/#09 のフックも兼ねる。代替案は Ark Dialog の `lazyMount`/`unmountOnExit` 挙動に依存してドロワー側のマウントを遅延する方法だが、デスクトップ aside の `hidden md:flex` が常時マウントされる点は解決しないため、ストア集約を採る。
- **`lucide-solid` 未導入**: ハンバーガーは文字「≡」で代替し、アイコン採用が決まり次第差し替え（1 箇所）。
- **`grid-cols-[clamp(...)]` の互換**: Tailwind v4 の arbitrary value で `clamp()` 内カンマが問題になる稀なケースに備え、`md:grid-cols-[280px_1fr]` を確実なフォールバックとして記載済み（§6.2）。
- **vitest 導入の波及**: 純粋関数のみを対象とするため jsdom 不要・最小。`tsc --noEmit` がテストファイルも型検査するが、`vitest` が devDependency に入っていれば `import { ... } from "vitest"` は解決する。コンポーネントテスト（jsdom + `@solidjs/testing-library`）は将来の別作業として保留。
