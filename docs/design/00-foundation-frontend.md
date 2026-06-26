# 00 — フロントエンド設計の土台（Foundation）

このドキュメントは RSS リーダーのフロントエンドが**11機能すべてを一貫したアーキテクチャ上に載せる**ための土台を定義する。
個別機能の詳細設計（01〜11）は、ここで決めた枠組み（シェル / ルーティング / グローバル状態 / UI プリミティブ / API クライアント / デザイントークン）の上に薄く積む。

対象スタック: SolidJS 1.9 / `@solidjs/router` 0.16 / Vite 6 / Tailwind CSS v4 / Ark UI 5（`@ark-ui/solid`）。
既存資産: `App.tsx`（単一カラムシェル）、`lib/api.ts`（型付き fetch クライアント）、`components/ui/{button,card,dialog}.tsx`、`app.css`（oklch デザイントークン + `.dark` 配線済み）。

> 設計原則: **既存の薄さを壊さない。** バックエンドの Vertical Slice に呼応して、フロントも「画面（route）+ 局所状態」を基本単位とし、横断する巨大ストアや共通レイヤーは最小に保つ。グローバル状態は「テーマ・フィルタ・選択・モバイルUI」だけに限定する。

---

## 0. 機能 → 土台の対応表（この設計が11機能を担保することの確認）

| # | 機能 | 載る場所（土台） | バックエンド依存 |
|---|------|------------------|------------------|
| 01 | フィード管理 | `/manage` ルート + `dropdown-menu`/`select`/`dialog` | `PATCH /api/feeds/{id}`（rename, folder_id）, フィード別未読数 |
| 02 | フォルダ分け | Sidebar の `tree-view` + 選択ルート + `select`（割当） | 新 `folders` スライス, `feeds.folder_id` マイグレーション |
| 03 | 最終投稿/頻度 | `/manage` の各行 + `listFeedStats()` | 新集計エンドポイント（per-feed 集計） |
| 04 | ダークテーマ | グローバルストア `theme` + `switch` + `index.tsx` 初期化 | なし（クライアント） |
| 05 | Instapaper 連携 | `/settings` + `instapaper` API メソッド | 新 `instapaper` スライス |
| 06 | 後で読む | `ArticleView` の保存ボタン → `saveToInstapaper()` | 05 に依存（任意で `articles.saved_at`） |
| 07 | ミニマルデザイン | 本書「§5 デザイン指針」を全画面で適用 | なし |
| 08 | フィード追加の配置 | Sidebar 下部 or `/manage` の `dialog` 起動ボタン | なし（既存 `POST /api/feeds`） |
| 09 | 既読管理 | 自動既読（`ArticleView`）+ 一括既読 + 未読数バッジ | `POST .../mark-read`（一括）追加 |
| 10 | 二ペインレイアウト | `App.tsx` 再構成 + ネストルート + レスポンシブ | なし |
| 11 | すべて/未読トグル | グローバルストア `filter` + `segmented`/`switch` | なし（既存 `?unread=`） |

**結論: 11機能はすべて「ルート1枚 + 既存/新規 UI プリミティブ + グローバルストアの1フィールド」に分解でき、互いに密結合しない。**

---

## 1. 二ペインのアプリシェル設計

### 1.1 レイアウト構造

現状の `App.tsx` は `header + <main class="max-w-3xl">` の単一カラム。これを **左ペイン（ナビゲーション）+ 右ペイン（コンテンツ）** に再構成する。
右ペインは `@solidjs/router` のルートで切り替わり、**左ペインは常に同一インスタンスのまま**（再マウントしない＝ツリーの開閉状態・スクロール位置・未読数リソースを保持）。

```
┌───────────────────────────────────────────────────────────┐
│ Header (薄い。アプリ名 + テーマトグル + モバイル時ハンバーガー) │  ← md+ では省略可、Sidebar に統合
├──────────────┬────────────────────────────────────────────┤
│  Sidebar     │  <main>  = ルーティングされる右ペイン         │
│ (永続)        │                                            │
│  ・すべて/未読 │   /                → ArticleList(all)        │
│  ・フォルダ／  │   /feeds/:feedId   → ArticleList(feed)      │
│    フィード    │   /folders/:fId    → ArticleList(folder)    │
│    ツリー      │   /articles/:id    → ArticleView(本文)       │
│  ・未読数バッジ │   /manage          → FeedManage             │
│  ・[+ 追加]    │   /settings        → Settings               │
│  ・[設定/管理]  │                                            │
└──────────────┴────────────────────────────────────────────┘
```

`App.tsx` の骨子（擬似コード）:

```tsx
const App: ParentComponent = (props) => (
  <AppProvider>                              {/* §2 グローバル状態。Router 内なので route hooks が使える */}
    <div class="min-h-screen bg-background text-foreground
                md:grid md:grid-cols-[clamp(220px,22vw,300px)_1fr]">
      {/* 左ペイン: md+ は常時表示、モバイルは Drawer(Dialog) に退避 */}
      <Sidebar class="hidden md:flex md:flex-col md:border-r md:border-border md:h-screen md:sticky md:top-0" />
      <MobileTopBar class="md:hidden" />     {/* ハンバーガー → MobileDrawer を開く */}

      {/* 右ペイン: ルーティングされる本体 */}
      <main class="min-w-0">
        <div class="mx-auto max-w-3xl px-4 py-6">{props.children}</div>
      </main>
    </div>
  </AppProvider>
);
```

ポイント:
- 右ペインの読書幅は従来どおり `max-w-3xl`（可読性最優先、Feature 07）。一覧・管理画面もこの幅に収める。
- `md:grid-cols-[clamp(220px,22vw,300px)_1fr]` でサイドバー幅を可変・上限付きに。`min-w-0` を右ペインに付け、長い記事タイトルでのグリッド破綻を防ぐ。
- Sidebar は `sticky top-0 h-screen overflow-y-auto` で独立スクロール。

### 1.2 ルーティング構成（`index.tsx`）

`Router root={App}` は維持（App をレイアウトルートにする既存方針）。右ペインのルートを追加する。

```tsx
<Router root={App}>
  <Route path="/" component={ArticleList} />            {/* scope=all */}
  <Route path="/feeds/:feedId" component={ArticleList} />   {/* scope=feed  */}
  <Route path="/folders/:folderId" component={ArticleList} />{/* scope=folder */}
  <Route path="/articles/:id" component={ArticleView} />
  <Route path="/manage" component={FeedManage} />
  <Route path="/settings" component={Settings} />
</Router>
```

**設計判断: 「いま何を見ているか（選択フィード/フォルダ、開いている記事）」は URL を正とする。**
理由:
- リロード・ブラウザの戻る/進む・ホームLAN内での共有が自然に効く。
- 選択状態を別途ストアに二重管理しなくて済む（後述の `useSelection()` が `useParams`/`useLocation` から導出する）。
- Vertical Slice の精神（状態を画面に閉じる）にも合う。

`ArticleList` は3ルートで共有し、`useSelection()`（§2.4）が `feedId`/`folderId`/`all` を判定して `listArticles()` の引数を組み立てる。`filter`（すべて/未読）は URL ではなくグローバルストアから取る（§2）。

二ペイン内の「一覧 ⇄ 本文」遷移: 右ペインは**一覧か本文のどちらか**を表示する2ペイン構成（プロンプト指定どおり）。`/articles/:id` に入ると右ペインが本文になり、上部に「← 一覧へ戻る」（`history.back()` か直前の scope ルートへ）を置く。Sidebar の選択ハイライトは本文表示中も維持する。
> 拡張余地: `xl:` 以上で「Sidebar | 一覧 | 本文」の3ペインに広げられる構造にしてあるが、初期実装は2ペインで確定。

### 1.3 レスポンシブ（モバイル退避）

- `md`（768px）未満: グリッドを解除して**単一カラム**。Sidebar は `hidden md:flex` で隠し、代わりに `MobileTopBar` のハンバーガーから **Drawer** を開く。
- Drawer は既存の `dialog.tsx`（Ark UI Dialog）を**左寄せ・全高のドロワー**として再利用（`DialogContent` のクラスを `left-0 h-full max-w-[300px] rounded-none` 系で上書き、または `components/ui/dialog.tsx` に `side` バリアントを足す薄い拡張）。新規ライブラリは不要。
- フィード/フォルダを選んだら Drawer を閉じてから遷移（ストアの `closeSidebar()` を呼ぶ）。これによりモバイルでも「一覧→本文」の単一カラム動線が保たれる。
- ブレークポイントは Tailwind 既定の `md` を基準に統一。

---

## 2. グローバル状態設計

### 2.1 方針: 「設定 + 横断UIフラグ」だけを持つ最小ストア

グローバルに必要なのは次の4種だけ。それ以外（記事リスト、フィード一覧、記事本文）は**各ルートの `createResource` + `createSignal` のローカル**に閉じる（現状方針を維持）。

| 状態 | 種別 | 永続化 | 担当機能 |
|------|------|--------|----------|
| `theme` (`'light' \| 'dark'`) | 設定 | localStorage | 04 |
| `filter` (`'all' \| 'unread'`) | 横断UI | localStorage（任意） | 11 |
| `sidebarOpen` (モバイルDrawer) | 横断UI | なし | 10 |
| 選択（folder/feed/article） | **派生（URL）** | URL | 01,02,10 |

> 選択状態を `createStore` に持たせない理由は §1.2 のとおり。URL を正とし、`useSelection()` で読み取る。これでフィルタ・テーマだけの極小ストアになり、テストもしやすい。

### 2.2 置き場所と形

`src/lib/store.tsx` に `createContext` ベースで実装し、`App`（= Router root）直下で `<AppProvider>` する。Router 内なので Provider 内から `useLocation`/`useNavigate` が使える。

```tsx
// src/lib/store.tsx
import { createContext, useContext, type ParentComponent } from "solid-js";
import { createStore } from "solid-js/store";

type Theme = "light" | "dark";
type Filter = "all" | "unread";

interface UiStore {
  theme: Theme;
  filter: Filter;
  sidebarOpen: boolean;
}

function createAppState() {
  const [ui, setUi] = createStore<UiStore>({
    theme: initialTheme(),       // §2.3
    filter: (localStorage.getItem("filter") as Filter) ?? "all",
    sidebarOpen: false,
  });

  const setTheme = (t: Theme) => { applyTheme(t); localStorage.setItem("theme", t); setUi("theme", t); };
  const toggleTheme = () => setTheme(ui.theme === "dark" ? "light" : "dark");
  const setFilter = (f: Filter) => { localStorage.setItem("filter", f); setUi("filter", f); };
  const openSidebar  = () => setUi("sidebarOpen", true);
  const closeSidebar = () => setUi("sidebarOpen", false);

  // 未読数の再取得トリガ（§2.5）
  const counts = createUnreadCounts();   // { stats, refresh }

  return { ui, setTheme, toggleTheme, setFilter, openSidebar, closeSidebar, counts };
}

const Ctx = createContext<ReturnType<typeof createAppState>>();
export const AppProvider: ParentComponent = (props) => (
  <Ctx.Provider value={createAppState()}>{props.children}</Ctx.Provider>
);
export const useApp = () => {
  const v = useContext(Ctx);
  if (!v) throw new Error("useApp must be used within <AppProvider>");
  return v;
};
```

- `createStore`（プロキシ）で構造化状態を持ち、setter は薄いアクションとして公開する（直接 `setUi` を外に出さない＝変更経路を限定）。
- フィルタやテーマの read は `useApp().ui.filter` のように**細粒度リアクティブ**で取れる（SolidJS の利点）。

### 2.3 テーマ（Feature 04）— FOUC 回避を含む

ダークモードは `app.css` で `@custom-variant dark (&:is(.dark *))` 済み。やることは「`<html>` の `class="dark"` 付与 + 永続化 + 初期値」。

**初期化は描画前（`index.tsx` 冒頭、`render()` の前）に同期実行**して、ちらつき（FOUC）を防ぐ:

```ts
// src/lib/theme.ts
export function initialTheme(): "light" | "dark" {
  const saved = localStorage.getItem("theme");
  if (saved === "light" || saved === "dark") return saved;
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}
export function applyTheme(t: "light" | "dark") {
  document.documentElement.classList.toggle("dark", t === "dark");
}
```

```ts
// src/index.tsx 先頭（import 群の直後、render の前）
applyTheme(initialTheme());
```

トグル UI は §3 の `switch.tsx`（Ark UI Switch）を `Sidebar` 下部 or `/settings` に置き、`useApp().toggleTheme()` を呼ぶ。バックエンド不要（単一ユーザ・クライアント側状態）。

### 2.4 選択の導出 `useSelection()`

URL から「いまのスコープ」を導出するヘルパ。`ArticleList` と `Sidebar`（ハイライト）が共有する。

```ts
// src/lib/selection.ts
import { useParams, useLocation } from "@solidjs/router";

export type Scope =
  | { kind: "all" }
  | { kind: "feed"; feedId: string }
  | { kind: "folder"; folderId: string };

export function useSelection(): () => Scope {
  const params = useParams();
  const loc = useLocation();
  return () => {
    if (params.feedId) return { kind: "feed", feedId: params.feedId };
    if (params.folderId) return { kind: "folder", folderId: params.folderId };
    return { kind: "all" };   // loc.pathname を見て将来の拡張も可能
  };
}
```

`ArticleList` 側:

```ts
const scope = useSelection();
const { ui } = useApp();
const [articles] = createResource(
  () => ({ scope: scope(), unread: ui.filter === "unread" }),
  ({ scope, unread }) => api.listArticles({
    feed_id:   scope.kind === "feed"   ? scope.feedId   : undefined,
    folder_id: scope.kind === "folder" ? scope.folderId : undefined,
    unread,
  }),
);
```

`filter` がストア、`scope` が URL という二系統の依存を `createResource` の source 関数で合成する。どちらが変わっても自動で再フェッチされる。

### 2.5 未読数の整合（Feature 09 と Sidebar バッジ）

Sidebar はフィード別未読数を出す（`listFeedStats()`、§4）。既読操作（自動既読・トグル・一括既読）の後にこれを更新する必要がある。グローバルに**未読数リソース + `refresh()`** を一つ持ち、既読系アクション後に呼ぶ:

```ts
function createUnreadCounts() {
  const [stats, { refetch }] = createResource(() => api.listFeedStats());
  return { stats, refresh: refetch };
}
```

- `ArticleView` で自動既読したら `useApp().counts.refresh()`。
- 一括既読後も同様。
- 楽観的更新が必要になったら `createStore` 化して個別デクリメントに差し替え可能（初期は `refetch` で十分。ホームLAN・単一ユーザでコストは低い）。

---

## 3. 必要な Ark UI 部品の洗い出し

方針（CLAUDE.md）: **単純な見た目部品 = 自前 Tailwind + cva / 複雑な a11y 部品 = Ark UI を薄くラップし `components/ui/` に置く**。すべて oklch トークンで装飾。
Ark UI v5 の compound API は版で変わりうるため、**実装時に ark-ui.com（Solid タブ）で各 part 名を必ず確認**する（既存 `dialog.tsx` のヘッダコメントと同じ運用）。

| 部品 | 実装 | 用途（機能） | Ark UI の主要 part（v5、要確認） | 着手 |
|------|------|--------------|----------------------------------|------|
| `button` / `card` / `dialog` | 既存 | 全般 | — | 済 |
| `input` | **自前**(cva) | 追加URL・フォルダ名・Instapaper資格情報（05,08） | — | 早期 |
| `switch` | **Ark UI** `Switch` | テーマトグル（04）、任意でフィルタ | `Switch.Root / Control / Thumb / HiddenInput / Label`、`checked` + `onCheckedChange` | 早期 |
| `segmented` | **自前**(cva, 2ボタン) or Ark `SegmentGroup` | すべて/未読（11） | （自前なら不要）/ `SegmentGroup.Root / Item / ItemText / Indicator` | 早期 |
| `tree-view` | **Ark UI** `TreeView` | フォルダ→フィードのツリー（02,10） | `createTreeCollection({rootNode})`, `TreeView.Root(collection, selectedValue, expandedValue)`, `Branch / BranchControl / BranchIndicator / BranchText / BranchContent / Item / ItemText` | 中期 |
| `dropdown-menu` | **Ark UI** `Menu` | フィード行アクション: 改名/削除/フォルダ移動/一括既読（01,09） | `Menu.Root / Trigger / Positioner / Content / Item`、`onSelect` | 中期 |
| `select` | **Ark UI** `Select` | フォルダ割当ピッカー（01,02）、要約/翻訳言語（任意） | `createListCollection({items})`, `Select.Root / Label / Control / Trigger / Positioner / Content / Item / ItemText` | 中期 |
| `tooltip` | **Ark UI** `Tooltip` | アイコンボタンの補助（07,09） | `Tooltip.Root / Trigger / Positioner / Content` | 任意 |
| `badge` | **自前**(cva) | 未読数バッジ（01,09） | — | 早期 |
| Drawer | 既存 `dialog` に `side` バリアント追加 | モバイル Sidebar（10） | Dialog の Positioner/Content クラス上書き | 早期 |

補足:
- **すべて/未読（11）は自前 `segmented` を推奨**（2択・ラベル付き・見た目が単純）。a11y を厳密にしたいなら Ark `SegmentGroup`（roving focus + radiogroup）に差し替え可能。`switch` でも実装できるが「すべて/未読」は ON/OFF より2セグメントの方が意味が明確。
- **`tree-view`** は本アプリで最も重い部品。Ark の `TreeView` は `createTreeCollection({ rootNode })` にツリーデータ（`{ value, label, children }`）を渡し、`TreeView.Root` の `selectedValue`/`expandedValue` を制御する。動的レンダリングは `TreeView.NodeProvider` + 再帰コンポーネント or 静的 `Branch/Item` 合成のどちらか（**ark-ui.com の Solid 例で最新形を確認**）。
  - 代替案: 重ければ初期は **`collapsible`（Ark UI Collapsible）+ 自前リスト**でフォルダ折りたたみを実装し、後で `tree-view` に昇格してもよい。土台としては「`components/ui/tree-view.tsx` という1ファイルに閉じる」ことだけ守れば差し替えは局所で済む。
- アイコン: ミニマルUI（07）とアイコンボタン（追加・更新・後で読む・メニュー）のため **`lucide-solid` の導入を推奨**（Ark の公式 Solid 例も lucide-solid を使用、tree-shake 可・Solid ネイティブ）。新規依存の追加判断は §6 の「決定事項」に明記。

---

## 4. `lib/api.ts` の進化方針

### 4.1 規約

- **命名: `動詞 + リソース`（camelCase）**。既存 `listFeeds` / `addFeed` / `deleteFeed` / `listArticles` / `markRead` / `summarize` / `translate` に揃える。
- 1エンドポイント = 1メソッド。`http<T>()` ヘルパ（既存）をそのまま使い、`method`/`body` を渡す。`http` は 204 を `undefined` に畳む既存挙動を流用。
- リクエスト/レスポンス型は backend JSON をミラーする `interface` を同ファイルに置く（既存 `Feed`/`Article` と同じ）。
- フィルタ系の任意パラメータは `params?: { ... }` オブジェクト + `URLSearchParams` 組み立て（`listArticles` の既存パターン）。

### 4.2 既存型の拡張

```ts
export interface Feed {
  id: string;
  url: string;
  title: string | null;
  folder_id: string | null;   // ← 02 で追加（マイグレーションで feeds.folder_id）
  created_at: string;
  last_fetched_at: string | null;
}
```

`Article` は当面そのまま。Instapaper のローカル保存状態を持つなら（任意）`saved_at: string | null` を追加（06、別マイグレーション）。

### 4.3 追加する型

```ts
export interface Folder {            // 02
  id: string;
  name: string;
  created_at: string;
}

export interface FeedStat {          // 01, 03, 09  ← 集計は別エンドポイント/別スライス
  feed_id: string;
  unread_count: number;
  total_count: number;
  last_published_at: string | null;  // 最終投稿（03）
  posts_per_week: number | null;     // 投稿頻度（03、直近N件 or 週次集計）
}

export interface InstapaperStatus {  // 05
  configured: boolean;               // 資格情報が保存済みか（NotEnabled 判定の前段）
}
```

### 4.4 追加するメソッド群

```ts
export const api = {
  // ── feeds（01, 02, 03）─────────────────────────────
  listFeeds,                                   // 既存
  addFeed,                                      // 既存（08: 呼び出し元UIの配置だけ変更）
  deleteFeed,                                   // 既存
  refreshFeed:  (id: string) =>                 // 既存 POST /feeds/{id}/refresh（将来 per-feed 化）
    http<void>(`/api/feeds/${id}/refresh`, { method: "POST" }),
  updateFeed:   (id: string, patch: { title?: string; folder_id?: string | null }) =>  // 01,02
    http<Feed>(`/api/feeds/${id}`, { method: "PATCH", body: JSON.stringify(patch) }),
  listFeedStats: () => http<FeedStat[]>("/api/feeds/stats"),                            // 01,03,09

  // ── folders（02）──────────────────────────────────
  listFolders:  () => http<Folder[]>("/api/folders"),
  createFolder: (name: string) =>
    http<Folder>("/api/folders", { method: "POST", body: JSON.stringify({ name }) }),
  updateFolder: (id: string, patch: { name: string }) =>
    http<Folder>(`/api/folders/${id}`, { method: "PATCH", body: JSON.stringify(patch) }),
  deleteFolder: (id: string) => http<void>(`/api/folders/${id}`, { method: "DELETE" }),

  // ── articles（09, 11）────────────────────────────
  listArticles: (params?: { feed_id?: string; folder_id?: string; unread?: boolean }) => {  // folder_id を追加
    const q = new URLSearchParams();
    if (params?.feed_id)   q.set("feed_id", params.feed_id);
    if (params?.folder_id) q.set("folder_id", params.folder_id);
    if (params?.unread)    q.set("unread", "true");
    const qs = q.toString();
    return http<Article[]>(`/api/articles${qs ? `?${qs}` : ""}`);
  },
  getArticle, markRead, summarize, translate,   // 既存
  markAllRead: (scope: { feed_id?: string; folder_id?: string }) =>   // 09（全体は空オブジェクト）
    http<void>("/api/articles/mark-read", { method: "POST", body: JSON.stringify(scope) }),

  // ── instapaper / 後で読む（05, 06）────────────────
  getInstapaperStatus: () => http<InstapaperStatus>("/api/instapaper/status"),
  saveInstapaperCredentials: (creds: { username: string; password: string }) =>
    http<void>("/api/instapaper/credentials", { method: "PUT", body: JSON.stringify(creds) }),
  saveToInstapaper: (url: string) =>
    http<void>("/api/instapaper/add", { method: "POST", body: JSON.stringify({ url }) }),
};
```

### 4.5 バックエンド依存の明示（フロントが期待する API 契約）

このフロント設計は以下のバックエンド追加を前提とする（各スライスの詳細設計は別ドキュメント）。**「既存スライス拡張より新スライス優先」「マイグレーションは追記のみ」「`query!` 不使用」を厳守。**

- **02 folders**: 新スライス `features/folders/`（CRUD）。`feeds.folder_id`（nullable FK）を `000N_folders.sql` で追加。`updateFeed` の `folder_id` 更新は `feeds` スライスに**小さく正当化して** PATCH を足すか、folders 側に割当エンドポイントを置く（土台はどちらでも `api.updateFeed` の契約を満たせばよい）。
- **01/03/09 feed-stats**: per-feed の未読数・最終投稿・頻度集計。`feeds` 一覧を膨らませず**新スライス `features/feed_stats/`** で `GET /api/feeds/stats` を runtime query（`COUNT`、`MAX(published_at)`、直近N件の間隔）として実装し、フロントは id で突合する。
- **09 一括既読**: `POST /api/articles/mark-read {feed_id?|folder_id?}`。既存 articles スライスへの追記か、薄い新スライスかは要判断（一括は articles の責務に近いので articles 拡張が自然＝正当化可）。
- **11 未分類/フォルダ絞り**: `listArticles` の `folder_id` 対応（articles の list クエリに JOIN/フィルタ追加）。`folder_id` 未指定=全件、`feed_id`=単一フィード。未分類（`folder_id IS NULL`）は Sidebar 上「未分類」疑似フォルダとして表示し、フロントから `feed_id` 群で引くか、専用クエリを用意（要設計）。
- **05 instapaper**: 新スライス `features/instapaper/`。資格情報の保存（新テーブル）、`AppError::NotEnabled`（未設定時 503）、`reqwest` で Simple API（HTTP Basic で `/api/add` に URL を POST）を直接呼ぶ（`shared/llm/anthropic.rs` と同手法）。**実エンドポイント仕様は instapaper.com/api で要確認**。

---

## 5. デザイントークン運用とミニマル化の共通指針（Feature 07）

### 5.1 トークン: oklch を維持・拡張は @theme inline 経由のみ

- `app.css` の oklch 変数（`--background`/`--foreground`/`--muted`/`--accent`/`--border`/`--ring` …）と `.dark` ブロック、`@theme inline` マッピングを**そのまま使い続ける**。新しい色体系・任意 hex を持ち込まない。
- Sidebar 等で新しい面が必要でも、まず既存トークン（`bg-card` / `bg-muted` / `border-border`）で賄う。どうしても専用トークンが要るときだけ `--sidebar` 等を `:root` と `.dark` の両方に定義し、`@theme inline` に追加する（**片側だけ定義しない**＝ダーク対応を崩さない）。
- ダーク/ライトはトークンの差し替えで自動追従する設計なので、コンポーネント側は**生の色を書かず**意味トークン（`text-muted-foreground` 等）だけを使う。

### 5.2 タイポグラフィ階層（情報密度と可読性の両立）

| 用途 | クラス指針 |
|------|-----------|
| 記事本文・要約・翻訳 | `prose prose-sm dark:prose-invert max-w-none`（**本文表示のみ** prose を使う） |
| 画面見出し（記事タイトル本文側） | `text-2xl font-bold tracking-tight` |
| 一覧の記事タイトル | `text-sm font-medium`（未読は `font-semibold`、既読は `text-muted-foreground`） |
| メタ情報（日時・フィード名・頻度） | `text-xs text-muted-foreground` |
| Sidebar 項目 | `text-sm`、行高 `h-8`、`px-2` |

- **prose は本文・要約・翻訳の3箇所だけ。** 一覧・Sidebar・管理画面では prose を使わない（密度が落ちるため）。`ArticleView` の既存 `prose prose-sm dark:prose-invert max-w-none` を踏襲。
- 読書幅は `max-w-3xl`（右ペイン）/ 本文は prose の自然幅（約 65ch）。

### 5.3 余白・密度・色の使い方

- 余白スケールを統一: セクション間 `space-y-6`、一覧の行間 `space-y-1`〜`space-y-2`、カード内 `p-4`。Sidebar はより詰める（`py-1`〜`py-2`）。
- **色は中立を基調に、意味のある所だけ着色**:
  - 選択中（フィード/フォルダ/記事行）= `bg-accent text-accent-foreground`。
  - 未読 = 太字 + 小さな未読ドット/バッジ（`bg-primary`/`bg-muted` の `badge`）。既読 = `text-muted-foreground` でトーンダウン（既存 `FeedList` の `is_read ? text-muted-foreground` を継承）。
  - 破壊的操作（削除）= `variant="destructive"`。
  - ホバー = `hover:bg-accent`（既存 Card のパターン）。
- 罫線は 1px `border-border`、角丸は `--radius` 由来の `rounded-md`/`rounded-lg` のみ。影は `shadow-sm` 程度に抑える（ミニマル）。
- アイコンは `lucide-solid` を 16/18px で統一し、テキストと `gap-2`。アイコン単体ボタンは `Button size="icon"` + `tooltip` でラベルを補う。

### 5.4 一覧行・カード・ヘッダの具体調整

- 記事一覧: 現状の `Card` 羅列はやや重い。ミニマル化として**境界線区切りのリスト行**（`divide-y divide-border` + 行 `py-3`）に寄せ、タイトル + 1行サマリ + メタの3段に整理。Card は管理画面・本文の枠など「囲いが意味を持つ箇所」に限定する。
- ヘッダ: アプリ名は Sidebar 上部へ移し、右ペインのヘッダは薄く（パンくず/戻る + 文脈アクションのみ）。`App.tsx` の従来ヘッダはモバイル時の `MobileTopBar` に縮約。
- フォーカスリング `focus-visible:ring-2 ring-ring` は全インタラクティブ要素で維持（a11y）。

---

## 6. ファイル構成（追加・改名の全体像）

```
src/
  index.tsx            # ★ applyTheme(initialTheme()) を render 前に呼ぶ + ルート追加
  App.tsx              # ★ 二ペインシェル化（AppProvider + Sidebar + <main>）
  app.css              # 維持（必要時のみ @theme inline にトークン追記）
  lib/
    api.ts             # ★ 型・メソッド拡張（§4）
    utils.ts           # cn（既存）
    store.tsx          # ★ AppProvider / useApp（theme, filter, sidebar, counts）
    theme.ts           # ★ initialTheme / applyTheme
    selection.ts       # ★ useSelection（URL→scope 導出）
  components/
    ui/                # button, card, dialog（既存）+ input, switch, segmented,
                       #   badge, select, dropdown-menu, tree-view, tooltip（新規・薄ラップ）
    layout/
      Sidebar.tsx      # ★ フィルタトグル + FeedTree + [+追加] + 設定/管理リンク
      FeedTree.tsx     # ★ tree-view ラップ（フォルダ→フィード + 未読バッジ）
      MobileTopBar.tsx # ★ ハンバーガー（Drawer 起動）
  routes/
    ArticleList.tsx    # ★ FeedList を改名・整理（add-feed input を撤去＝08、scope+filter 対応）
    ArticleView.tsx    # ★ 自動既読（09）+ 後で読む（06）ボタン追加
    FeedManage.tsx     # ★ 管理画面（01,03）：改名/削除/フォルダ割当/頻度表示
    Settings.tsx       # ★ テーマ（04）+ Instapaper 資格情報（05）
```

> `FeedList.tsx`（実体は記事一覧）は **`ArticleList.tsx` に改名**して役割名と一致させ、追加入力（08）を取り除く。改名はルーティング定義と import の更新で局所的に済む。

---

## 7. 主要な設計判断（要点）

1. **右ペインの「いま見ているもの」は URL を正とする**（選択フィード/フォルダ/記事）。グローバルストアはテーマ・フィルタ・モバイルDrawer・未読数リソースだけに限定する。→ 二重管理を避け、戻る/進む・リロード・共有が自然に効く。
2. **Sidebar は永続インスタンス**（ルート切替で再マウントしない）。ツリー開閉・スクロール・未読数を保持し、右ペインだけがルーティングされる。
3. **モバイルは既存 Dialog をドロワー化して退避**（新規ライブラリ不要）。`md` 未満で単一カラム + ハンバーガー。
4. **Ark UI を薄くラップするのは a11y が要る部品だけ**（switch / select / dropdown-menu / tree-view / tooltip）。すべて/未読・input・badge は自前 cva。version 差異に備え各 part 名は実装時に ark-ui.com（Solid）で確認。
5. **集計と新概念は新スライス優先**（folders / feed-stats / instapaper）。既存 feeds/articles の拡張は「一括既読」「folder_id フィルタ」「PATCH」のように責務上自然で正当化できる最小限に留める。マイグレーションは追記のみ、`query!` 不使用。
6. **デザインは oklch トークンを温存**し、コンポーネントは意味トークンのみ使用。prose は本文・要約・翻訳の3箇所限定。ミニマル化は「中立基調 + 選択/未読/破壊のみ着色」「カード羅列→罫線リスト」で実現。
7. **未確認事項（実装時に要検証）**: Ark UI v5 各部品の part 名・props（ark-ui.com Solid）／Instapaper Simple API の正確なエンドポイントと重複時挙動（instapaper.com/api）／`lucide-solid` 新規依存の採否。
