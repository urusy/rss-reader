# 04 ダークテーマ切り替え

## 1. 概要

ライト / ダークテーマをユーザーが切り替えられるようにする。単一ユーザーかつ「この端末の見え方」に属する表示プリファレンスなので、状態はサーバーに持たず **完全にクライアント側**（localStorage + DOM クラス）で完結させる。`app.css` には既に `.dark` トークンブロックと `@custom-variant dark` が配線済みで、`<html>`（`document.documentElement`）に `class="dark"` を付与するだけで全コンポーネントの oklch トークンがダーク値に切り替わる。したがって本機能の実体は、(1) 永続化／OS 設定由来の初期テーマ決定、(2) DOM への反映（`.dark` クラス + `color-scheme`）、(3) トグル UI の 3 点に集約され、バックエンドは一切触らない。あわせて、この機能を **フロントエンドにユニットテストランナー（vitest）を初めて導入する自然な置き場所** とし、純粋ロジックを TDD（Red → Green）で固める。

## 2. スコープ / 非スコープ

含む:
- テーマの初期値決定（`localStorage["theme"]` を最優先、無ければ `prefers-color-scheme`、それも無ければ `light`）。
- テーマの localStorage 永続化と `<html>` への `dark` クラス付与/除去、および `document.documentElement.style.colorScheme` の同期（ネイティブ UI = スクロールバー/フォーム部品/キャンバス背景をテーマに追従させる）。
- 反応的なテーマ状態（`lib/theme.ts` のモジュールスコープ signal）と切り替えアクション（`toggleTheme` / `setTheme`）、および起動時初期化 `initTheme()`。
- トグル UI（Ark UI Switch を薄ラップした `components/ui/switch.tsx` + 設置用 `components/layout/ThemeToggle.tsx`）。
- 初回描画前のテーマ適用による FOUC（ちらつき）回避。
- **vitest + jsdom + @solidjs/testing-library をフロントエンドに導入**し、`lib/theme.ts` の純粋ロジックを Red → Green でテストする（§9）。

含まない:
- バックエンド / DB / API の変更（一切なし）。
- グローバル store（`lib/store.tsx`）そのものの新設（土台設計 §2 / 機能 10 の範疇）。本機能は store に依存せず動作し、store が来たら `lib/theme.ts` を再利用させる（§6.2・§8 で接続方針を明記）。
- 二ペインシェル（`App.tsx` 再構成、機能 10）や `/settings` ルート（機能 05）。本機能はトグルを現行 `App.tsx` ヘッダに置く。Sidebar / Settings が出来たら 1 行移設で済む形にする。
- システムテーマ追従の「自動（auto）」モード（明示的 light/dark の 2 値のみ。`prefers-color-scheme` はあくまで初期値の出所であり、ユーザーが一度選んだら固定）。将来拡張は §11。
- フロントのコンポーネント結合テスト（Switch の DOM 操作テスト等）。本スライスでは `@solidjs/testing-library` を依存として入れるが、テスト対象は `lib/theme.ts` の純粋関数に限定する（Switch は手動 + 型で検証、§9.2）。

## 3. 既存実装の調査と再利用

実ファイルを確認済み（パスは絶対）。再利用する資産:

- **`frontend/src/app.css`（編集不要）**: 5 行目 `@custom-variant dark (&:is(.dark *));` でクラスベースのダークモードが有効。`:root`（ライト）と `.dark`（ダーク）の両方に同一トークン名（`--background`, `--foreground`, `--card`, `--border`, `--ring`, `--primary`, `--input` ほか）が定義済みで、`@theme inline` で Tailwind の `bg-background` / `text-muted-foreground` 等にマップ済み。**`<html>` に `dark` クラスを付ける/外すだけ**で全画面が切り替わる。新トークンや生 hex は追加しない。※`app.css` には `color-scheme` 指定が無いため、ネイティブ UI を追従させる `color-scheme` は JS（`applyTheme` / インライン script）側で当てる（§6.2・§6.5）。
- **`frontend/src/index.tsx`（要編集）**: 現状 `render(() => <Router root={App}>...)` を呼ぶだけで、テーマ初期化が無い。ここに `render()` 前の `initTheme()` 呼び出しを 1 箇所追加する（FOUC 回避のバンドル側の差し込み点）。
- **`frontend/src/App.tsx`（要編集）**: 現状はヘッダ（`<a>RSS Reader</a>` と `<span class="text-sm text-muted-foreground">self-hosted</span>`）。`<main class="mx-auto max-w-3xl ...">` 構成。`bg-background text-foreground` を既に使っておりテーマ切替に追従する。ヘッダ右側にトグルを置く。
- **`frontend/src/components/ui/dialog.tsx`（パターン参照）**: Ark UI（`@ark-ui/solid/dialog`）を薄ラップし `splitProps` + `ComponentProps` + `cn()` + トークンで装飾する正準パターン。Switch も同じ流儀で書く。冒頭コメントに「part 名が変わったら ark-ui.com で確認」とある運用方針もそのまま踏襲。
- **`frontend/src/components/ui/button.tsx`（パターン参照）**: `cva` + `cn`、`focus-visible:ring-2 focus-visible:ring-ring` のフォーカスリング規約。Switch では「実際にフォーカスを受けるのは隠し input」なので、リングは Control に zag 由来のデータ属性 `data-[focus-visible]:` 経由で当てる（§6.3）。
- **`frontend/src/lib/utils.ts`**: `cn()`（clsx + tailwind-merge）。Switch ラッパで使用。
- **`@ark-ui/solid` 5.37.1（導入済み・`node_modules` で実体確認済み）**: Switch を新規追加で依存追加は不要。`package.json` の `exports` に `"./*"` ワイルドカードがあり、`@ark-ui/solid/switch` は `dist/components/switch/index.js` に解決される（実機で確認）。内部の状態機械は `@zag-js/switch` 1.41.2。
- **`@/` エイリアス**: `tsconfig.json` の `paths`（`@/*` → `./src/*`）と `vite.config.ts` の `resolve.alias` で設定済み。新規ファイルでも `@/lib/...`, `@/components/...` を使う。
- **TS 設定（`tsconfig.json`）**: `strict` / `noUnusedLocals` / `noUnusedParameters` / `verbatimModuleSyntax` / `isolatedModules` が有効。型 import は `import type`、未使用エクスポートを残さない。`include: ["src"]` なのでルート直下の設定ファイル（`vitest.config.ts`）は `tsc --noEmit` の対象外。
- **`just lint`**: フロントは `cd frontend && pnpm typecheck`（= `tsc --noEmit`）。**`just test`**: 現状 `cd backend && cargo test` のみ（フロントのテスト導線が無い）→ 本スライスで追加する（§6.7・§10）。

→ ダークモードの CSS 配線は完成しているので、CSS 側の作業はゼロ。新規追加は「テーマ状態モジュール」「Switch プリミティブ」「トグル設置」「初期化呼び出し」「テスト基盤 + テスト」のみ。

## 4. データモデルとマイグレーション

**DB 変更なし。** バックエンド・マイグレーションともに追加・編集なし。テーマは `localStorage` キー `theme`（値 `"light"` | `"dark"`）にのみ保存する。土台設計 §1（新規マイグレーションは 0002〜0004 のみ）に対し本機能は 0 本。

## 5. バックエンド設計

**バックエンド変更なし。** 新スライス・既存スライス拡張・`features/mod.rs` への `.merge()`・`AppError` の利用・trait 追加、いずれも不要。本機能はフロントエンド単独で閉じる（土台設計バックエンド §4「この端末の見え方 → クライアント」、同 §6 マトリクスの 04 行「マイグレーションなし / バックエンドなし」と一致）。

## 6. フロントエンド設計

### 6.1 追加/変更ファイル一覧

| パス | 種別 | 役割 |
|------|------|------|
| `frontend/src/lib/theme.ts` | 新規 | テーマの単一の真実。`Theme` 型、`STORAGE_KEY`、純粋関数 `initialTheme()`、副作用関数 `applyTheme()`、起動時 `initTheme()`、モジュールスコープ signal（`theme`）とアクション（`setTheme` / `toggleTheme`）。 |
| `frontend/src/lib/theme.test.ts` | 新規 | `lib/theme.ts` の純粋ロジックの vitest ユニットテスト（§9.3）。 |
| `frontend/src/components/ui/switch.tsx` | 新規 | Ark UI Switch の薄ラップ（トークン装飾）。汎用プリミティブ。 |
| `frontend/src/components/layout/ThemeToggle.tsx` | 新規 | `Switch` + `lib/theme.ts` を結線した設置用コンポーネント（土台設計 §6 の `components/layout/` に配置）。 |
| `frontend/src/index.tsx` | 変更 | `render()` 前に `initTheme()` を同期実行（FOUC 回避・signal 初期化）。 |
| `frontend/src/App.tsx` | 変更 | ヘッダ右側に `<ThemeToggle />` を設置。 |
| `frontend/index.html` | 変更（推奨・任意） | `<head>` 先頭にインライン script を追加し、バンドル読み込み前に `dark` クラス + `color-scheme` を同期適用して残留 FOUC を消す（§6.5）。 |
| `frontend/vitest.config.ts` | 新規 | vitest 設定（solid plugin + jsdom + `@` エイリアス）。§6.7。 |
| `frontend/package.json` | 変更 | devDependencies に vitest 一式追加 + `test` スクリプト追加。§6.7。 |
| `justfile` | 変更 | `test` レシピにフロントの vitest 実行を追記。§10。 |

### 6.2 `lib/theme.ts`（テーマの単一の真実）

**設計の要点（レビュー反映）**: モジュール eval 時には環境（`window.matchMedia` / `localStorage`）を一切読まない。signal は安価な定数 `"light"` で seed し、実際の初期値解決（`initialTheme()`）と DOM 反映（`applyTheme()`）は `index.tsx` が明示的に呼ぶ `initTheme()` の中で行う。これにより:

- `lib/theme.ts` を **import しただけでは副作用ゼロ**（jsdom で `window.matchMedia` 未定義でも import が例外を投げない）。
- `initialTheme()` が **呼び出し時に**環境を読む純粋関数になり、テストが「import 後に matchMedia をモックしてから呼ぶ」ことが可能（§9.3 のケースが成立する）。

```ts
import { createSignal } from "solid-js";

export type Theme = "light" | "dark";

export const STORAGE_KEY = "theme";

/** prefers-color-scheme: dark か。matchMedia 未実装環境（jsdom 既定）でも安全に false。 */
function prefersDark(): boolean {
  return (
    typeof window !== "undefined" &&
    !!window.matchMedia?.("(prefers-color-scheme: dark)").matches
  );
}

/** localStorage 最優先 → prefers-color-scheme → "light"。副作用なしの純粋関数（呼び出し時に環境を読む）。 */
export function initialTheme(): Theme {
  const stored =
    typeof localStorage !== "undefined" ? localStorage.getItem(STORAGE_KEY) : null;
  if (stored === "light" || stored === "dark") return stored;
  return prefersDark() ? "dark" : "light";
}

/** <html> に dark クラスと color-scheme を反映。副作用のみ。 */
export function applyTheme(t: Theme): void {
  const el = document.documentElement;
  el.classList.toggle("dark", t === "dark");
  el.style.colorScheme = t; // ネイティブ UI（スクロールバー/フォーム/キャンバス）も追従
}

// 安価な定数で seed（import 時に matchMedia / localStorage を読まない＝テスト容易・jsdom 安全）。
const [theme, setThemeSignal] = createSignal<Theme>("light");
export { theme };

/** 明示設定: signal 更新 + 永続化 + DOM 反映。ユーザー操作はここを通る。 */
export function setTheme(t: Theme): void {
  setThemeSignal(t);
  localStorage.setItem(STORAGE_KEY, t);
  applyTheme(t);
}

export function toggleTheme(): void {
  setTheme(theme() === "dark" ? "light" : "dark");
}

/**
 * 起動時に一度だけ呼ぶ（index.tsx の render 前）。
 * 解決済みテーマで signal を seed し DOM へ反映する。
 * localStorage への書き込みはしない（prefers 由来の初期値を勝手に固定しないため）。
 */
export function initTheme(): void {
  const t = initialTheme();
  setThemeSignal(t);
  applyTheme(t);
}
```

注意点:
- `seed = 状態の初期化`、`applyTheme = DOM 反映`、`setTheme = ユーザー操作（永続化込み）`、`initTheme = 起動時 1 回（永続化しない）` と責務を分離。
- `theme` は `noUnusedLocals` を満たすため必ず利用箇所がある（`ThemeToggle`）。
- 将来 `lib/store.tsx`（機能 10）が来たら、store の `theme`/`toggleTheme` は **この `lib/theme.ts` を re-export / 委譲**するだけにし、ロジックを複製しない（土台設計フロント §2.3「`theme.ts` に initialTheme/applyTheme、store は toggleTheme を呼ぶ」と整合）。

### 6.3 `components/ui/switch.tsx`（Ark UI Switch 薄ラップ）

`dialog.tsx` と同じ流儀（`splitProps` + `ComponentProps` + `cn`）。**本リポジトリ同梱の実体（`@ark-ui/solid` 5.37.1 / 内部 `@zag-js/switch` 1.41.2）を確認済み**の事実:

- import は `import { Switch as ArkSwitch } from "@ark-ui/solid/switch";`（`exports` の `"./*"` ワイルドカードで解決）。
- アナトミー: `Switch.Root`（**`<label>` としてレンダリングされ、`htmlFor` が隠し input を指す**） / `Switch.Control`（`<span>`, `aria-hidden`） / `Switch.Thumb`（`<span>`, `aria-hidden`） / `Switch.Label`（`<span>`, 任意） / `Switch.HiddenInput`（実際にフォーカスを受ける視覚的に隠れた `<input type="checkbox">`）。
- 制御 props: `checked?: boolean`（制御）+ `onCheckedChange?: (details: { checked: boolean }) => void`。
- zag が **全 part（Root/Control/Thumb/Label）に同じ data 属性群を撒く**: チェック状態 `data-state="checked" | "unchecked"`、フォーカス `data-focus` / `data-focus-visible`（隠し input の `onFocus` がトリガ、キーボード時のみ `data-focus-visible`）、ほか `data-disabled` / `data-hover` 等。

```tsx
// Switch — Ark UI の headless Switch をトークンで装飾した薄ラップ。
// part 名・data 属性は @ark-ui/solid 5.37 / @zag-js/switch 1.41 で確認済みだが、
// メジャー更新で変わりうる。壊れたら https://ark-ui.com (Solid / Switch) で確認。
import { Switch as ArkSwitch } from "@ark-ui/solid/switch";
import { splitProps, type ComponentProps } from "solid-js";
import { cn } from "@/lib/utils";

type SwitchProps = ComponentProps<typeof ArkSwitch.Root> & { label?: string };

export function Switch(props: SwitchProps) {
  const [local, rest] = splitProps(props, ["class", "label"]);
  return (
    <ArkSwitch.Root class={cn("inline-flex items-center gap-2", local.class)} {...rest}>
      <ArkSwitch.Control
        class={cn(
          "inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full bg-input p-0.5 transition-colors",
          "data-[state=checked]:bg-primary",
          // 実フォーカスは隠し input。zag が Control に data-focus-visible を立てるのでそこにリングを当てる。
          "data-[focus-visible]:outline-none data-[focus-visible]:ring-2 data-[focus-visible]:ring-ring",
        )}
      >
        <ArkSwitch.Thumb class="h-4 w-4 rounded-full bg-background shadow-sm transition-transform data-[state=checked]:translate-x-4" />
      </ArkSwitch.Control>
      {local.label ? (
        <ArkSwitch.Label class="select-none text-sm">{local.label}</ArkSwitch.Label>
      ) : null}
      <ArkSwitch.HiddenInput />
    </ArkSwitch.Root>
  );
}
```

装飾は oklch トークンのみ（`bg-input` = オフ、`bg-primary` = オン、`bg-background` = つまみ、`ring-ring` = フォーカス）。生の色は使わない。フォーカスリングは **Control に `data-[focus-visible]:ring-2`** で当てる（Control 自体は `aria-hidden` でフォーカス不能なので `focus-visible:` 擬似クラスは一致しない。代わりに zag が撒く `data-focus-visible` を使う ── これがレビュー指摘「Control vs HiddenInput のリング位置」の確定回答）。

**実装時の最終確認（ark-ui.com / Solid / Switch、本リポジトリの版に対して）**: `data-[state=checked]`（撒かれる値は `checked`/`unchecked` を確認済み）と `data-[focus-visible]`（確認済み）が現物の DOM に出ているかを devtools で一度目視し、装飾セレクタが一致していることを確かめる。`Switch.Root` が `<label>` なので **`ThemeToggle` 側でさらに `<label>` で囲まない**。

### 6.4 `components/layout/ThemeToggle.tsx`（設置用）

土台設計フロント §6 のツリー（`components/ui/` = プリミティブ、`components/layout/` = シェル部品）に合わせ、**結線済みの設置部品は `components/layout/`** に置く（`Sidebar` / `MobileTopBar` と同じ層）。`components/layout/` ディレクトリは本スライスで新規作成する。

```tsx
import { Switch } from "@/components/ui/switch";
import { theme, setTheme } from "@/lib/theme";

export function ThemeToggle() {
  return (
    <Switch
      label="ダークモード"
      checked={theme() === "dark"}
      onCheckedChange={(e) => setTheme(e.checked ? "dark" : "light")}
    />
  );
}
```

`checked` は `theme()` から派生（signal なので自動追従）、`onCheckedChange` の `e.checked`（`{ checked: boolean }`）で `setTheme` を呼ぶ。アイコン（sun/moon）が欲しい場合は `lucide-solid` 導入後に `label` をアイコンに差し替え可能だが、本機能では新規依存を避けテキストラベルで完結させる（`lucide-solid` の採否は土台設計で未決）。

### 6.5 `index.tsx`（FOUC 回避の初期化）

`render()` の前に `initTheme()` を同期実行する。`initTheme()` が `initialTheme()` で値を解決し、signal を seed し、`applyTheme()` で `<html>` にクラスと `color-scheme` を当てる:

```tsx
/* @refresh reload */
import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import "./app.css";
import App from "./App";
import FeedList from "./routes/FeedList";
import ArticleView from "./routes/ArticleView";
import { initTheme } from "./lib/theme";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");

initTheme(); // render 前に <html> へ dark クラス + color-scheme を同期適用

render(
  () => (
    <Router root={App}>
      <Route path="/" component={FeedList} />
      <Route path="/articles/:id" component={ArticleView} />
    </Router>
  ),
  root,
);
```

**FOUC の dev / prod の違い（レビュー反映・重要）**:
- **本番ビルド**: `import "./app.css"` はビルド時に抽出され `<head>` の render-blocking な `<link rel="stylesheet">` になる。`index.html` の `<head>` 先頭に置いたインライン script（下記）が **スタイルシート適用前**に `.dark` と `color-scheme` を `<html>` へ当てるため、ダーク選択ユーザーでもライトのちらつきは出ない（ゼロ FOUC が成立）。
- **Vite dev（`pnpm dev` / `just front`）**: `app.css` は JS（`import "./app.css"`）経由で実行時に注入され、**バンドル実行までスタイルシートが存在しない**。よってインライン script を入れても dev では「JS ロード前は素の UA 背景」という一瞬は残る。ただしインライン script が `color-scheme: dark` を即座に当てるため、ブラウザがダークの既定キャンバスを描き、体感のちらつきは大幅に減る。**dev で完全ゼロ FOUC を追わないこと**（これは Vite dev の構造によるもので、本番では問題にならない）。

`index.html` の `<head>` 先頭に置くインライン script（Vite が処理しない素の script）:

```html
<!-- frontend/index.html の <head> 先頭。lib/theme.ts と同じキー/判定の最小重複。 -->
<script>
  (function () {
    try {
      var t = localStorage.getItem("theme");
      if (t !== "light" && t !== "dark") {
        t = window.matchMedia &&
            window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light";
      }
      var el = document.documentElement;
      if (t === "dark") el.classList.add("dark");
      el.style.colorScheme = t;
    } catch (e) {}
  })();
</script>
```

このインライン script は localStorage キー（`theme`）と判定ロジック・`color-scheme` 適用を最小限だけ重複させる。**ランタイムの読み書きは依然 `lib/theme.ts` が唯一の真実**で、インライン script は「最初の 1 フレームのクラス + color-scheme 当て」だけを担う。土台設計フロント §2.3 の baseline（`index.tsx` で適用）は維持しつつ、本番のゼロ FOUC を求めてこの script を足す二段構え。採否は実装者判断だが **推奨**。

### 6.6 `App.tsx`（トグル設置）

ヘッダ右側の `self-hosted` span の隣に `<ThemeToggle />` を置く:

```tsx
import type { ParentComponent } from "solid-js";
import { ThemeToggle } from "@/components/layout/ThemeToggle";

const App: ParentComponent = (props) => {
  return (
    <div class="min-h-screen bg-background text-foreground">
      <header class="border-b border-border">
        <div class="mx-auto max-w-3xl px-4 py-3 flex items-center justify-between">
          <a href="/" class="text-lg font-semibold tracking-tight">
            RSS Reader
          </a>
          <div class="flex items-center gap-3">
            <span class="text-sm text-muted-foreground">self-hosted</span>
            <ThemeToggle />
          </div>
        </div>
      </header>
      <main class="mx-auto max-w-3xl px-4 py-6">{props.children}</main>
    </div>
  );
};

export default App;
```

機能 10（二ペイン）/機能 05（Settings）が来たら、`<ThemeToggle />` を Sidebar 下部または `/settings` に **import 1 行 + JSX 1 行**で移設できる。

### 6.7 状態管理・ルーティング・テスト基盤

- 状態は `lib/theme.ts` のモジュール signal のみ。グローバル store もルーティング変更も `lib/api.ts` への追加メソッドも不要（API を叩かない）。
- **テスト基盤の追加（本スライスで導入）**。SolidJS 公式の vitest 構成に従う:
  - `frontend/package.json` の devDependencies に追加: `vitest`, `jsdom`, `@solidjs/testing-library`, `@testing-library/user-event`, `@testing-library/jest-dom`。本スライスのテスト対象（`lib/theme.ts` 純粋関数）自体は vitest + jsdom だけで十分だが、後続のコンポーネントテストに備え testing-library 系も入れる。
  - `frontend/package.json` の `scripts` に `"test": "vitest run"` と `"test:watch": "vitest"` を追加。
  - `frontend/vitest.config.ts` を新規作成（solid plugin + jsdom + `@` エイリアス）:

    ```ts
    import { defineConfig } from "vitest/config";
    import solid from "vite-plugin-solid";
    import { fileURLToPath, URL } from "node:url";

    export default defineConfig({
      plugins: [solid()],
      resolve: {
        alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
        conditions: ["development", "browser"], // Solid のテスト時条件（公式推奨）
      },
      test: {
        environment: "jsdom",
      },
    });
    ```

    補足: `tsconfig.json` の `include` は `["src"]` なので `vitest.config.ts` は `tsc --noEmit` の対象外（型解決の追加設定不要）。テストファイルはテストランナー名前空間を **明示 import**（`import { test, expect, vi, beforeEach } from "vitest";`）するため、`tsconfig` の `types` に `vitest/globals` を足す必要はない（`noUnusedLocals` 等の strict 設定とも干渉しない）。

## 7. API 契約

**追加・変更するエンドポイントなし。** 本機能はネットワーク I/O を行わない。永続化は `localStorage["theme"] ∈ {"light","dark"}` のみ。

## 8. 依存関係

- **依存する機能**: なし。クライアント単独で独立して着手・出荷できる（バックエンド未着でも完結）。土台設計マトリクスの 04 行と一致。
- **関連（依存ではない）**:
  - 機能 10（二ペインレイアウト / `lib/store.tsx`）: store が来たら `theme`/`toggleTheme` を `lib/theme.ts` に委譲させる。本機能が先行しても後続が再利用できる形にしてある。
  - 機能 05（Instapaper / `/settings`）: Settings 画面が出来たらトグルの最終的な置き場所候補。移設は 1 行。
  - 機能 07（ミニマルデザイン）: ダーク配色の一貫性は `app.css` のトークンで自動担保。本機能は新トークンを足さないので 07 と矛盾しない。
- **ブロックする機能**: なし（他機能の前提にはならない）。
- **副次的に提供する基盤**: 本スライスで vitest を初導入するため、以降のフロント機能（11 のトグル、selection ロジック等）が同じランナーで純粋ロジックを TDD できるようになる。

## 9. テスト計画（TDD）

純粋ロジック（`initialTheme` / `applyTheme` / `setTheme` / `toggleTheme`）は **本リポジトリの TDD 規約（CLAUDE.md / MEMORY「書いたら必ず実行」）に従い、自動ユニットテストを Red → Green で書いて実行する**（§9.3）。Switch の DOM 挙動と FOUC は型 + 手動で検証する（§9.1 / §9.2）。

### 9.1 型・lint（自動）
- `just lint`（フロントは `tsc --noEmit`）が通ること。`noUnusedLocals` / `noUnusedParameters` / `verbatimModuleSyntax` を満たすため、型 import は `import type`、未使用エクスポートを残さない。
- Switch ラッパの props 型が `ComponentProps<typeof ArkSwitch.Root>` と齟齬なくコンパイルできること（Ark UI v5 の `onCheckedChange` の details 形 `{ checked: boolean }` 確認を兼ねる）。

### 9.2 手動検証マトリクス（Red = 期待挙動を先に固定。Switch / FOUC 用）
1. **トグルで切替** — Switch を操作 → 即座に全画面（背景・カード・枠線・本文 prose）が反転。`document.documentElement.classList` に `dark` が付く/外れる。`<html style="color-scheme">` も `dark`/`light` に追従。
2. **永続化** — トグル後にリロード → 選択が維持される（`localStorage.theme` が更新済み）。
3. **Switch の checked 同期** — `theme()` の値と Switch の見た目（オン/オフ）が常に一致。
4. **キーボードフォーカス** — Tab で Switch にフォーカス → Control にフォーカスリング（`data-[focus-visible]:ring-2`）が出る。Space/Enter でトグル。マウスクリックではリングが出ない（`data-focus-visible` はキーボード時のみ）。
5. **ネイティブ UI** — ダーク時にスクロールバー・フォーム部品・オーバースクロール背景がダーク化（`color-scheme` の効果）。
6. **FOUC（本番ビルド）** — `pnpm build && pnpm preview` でダーク選択状態のままハードリロード → ライトのちらつきが視認されない（インライン script 採用時）。dev では §6.5 の通り一瞬の素背景は許容。

### 9.3 ユニットテスト（自動・必須・Red → Green）

`frontend/src/lib/theme.test.ts`。`import { test, expect, vi, beforeEach } from "vitest";` と `import * as theme from "./theme";`（相対 import）。jsdom 環境。各テストの前に状態をリセット:

```ts
beforeEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
  document.documentElement.className = "";
  document.documentElement.style.colorScheme = "";
});
```

テスト一覧と意図:

1. **`initialTheme()` は localStorage を最優先する** — `localStorage.setItem("theme","dark")` → `initialTheme() === "dark"`。`"light"` → `"light"`。
   意図: 永続化された明示選択が prefers より勝つこと。
2. **`initialTheme()` は不正値を無視して prefers にフォールバック** — `localStorage.setItem("theme","blue")` + `matchMedia` を dark にモック（`vi.stubGlobal("matchMedia", () => ({ matches: true }))`）→ `"dark"`。
   意図: ストアの汚染値で壊れない。レビューが「import 後に matchMedia をモックしてから呼べること」を要求したケースが、`initialTheme()` を呼び出し時評価にしたことで成立する。
3. **`initialTheme()` は localStorage 未設定なら prefers に従う** — 未設定 + matchMedia dark → `"dark"`、matchMedia light（`matches:false`）→ `"light"`。
4. **`initialTheme()` は matchMedia 未実装なら "light"** — 未設定 + `matchMedia` 未スタブ（jsdom 既定で `window.matchMedia` undefined）→ `"light"`（`prefersDark()` の optional-chaining ガードが効くこと）。
5. **`applyTheme()` が DOM に反映** — `applyTheme("dark")` → `documentElement.classList.contains("dark") === true` かつ `documentElement.style.colorScheme === "dark"`。`applyTheme("light")` → `false` / `"light"`。
6. **`setTheme()` が signal・永続化・DOM を同時更新** — `setTheme("dark")` 後、`theme() === "dark"` かつ `localStorage.getItem("theme") === "dark"` かつ `documentElement.classList.contains("dark")`。
7. **`toggleTheme()` が往復** — `setTheme("light")` 後に `toggleTheme()` → `theme() === "dark"`、再度 `toggleTheme()` → `"light"`。
8. **import に副作用が無い（回帰防止）** — `import * as theme from "./theme"` した時点では `documentElement.classList.contains("dark") === false` かつ `localStorage` 未書き込み（モジュール eval が環境を読まないこと）。
   意図: レビュー指摘の「import 時 eager seed で jsdom が落ちる」回帰を防ぐ。

> 注: 4 と 8 は「`window.matchMedia` 未実装でも import / 呼び出しが例外を投げない」というレビューの中核修正をピン留めするテスト。これらが Green であることが本スライスの肝。

## 10. 実装手順

別セッションでそのまま辿れる順序:

1. **テスト基盤を先に用意（Red を実行可能にする）**:
   - `frontend/package.json` の devDependencies に `vitest` / `jsdom` / `@solidjs/testing-library` / `@testing-library/user-event` / `@testing-library/jest-dom` を追加し、`scripts` に `"test": "vitest run"` と `"test:watch": "vitest"` を追加。`pnpm install`。
   - `frontend/vitest.config.ts` を §6.7 の内容で新規作成。
   - `justfile` の `test` レシピを次に変更（フロントの vitest を後続実行）:
     ```
     test:
         cd backend && cargo test
         cd frontend && pnpm install && pnpm test
     ```
2. `frontend/src/lib/theme.test.ts` を §9.3 の 8 ケースで先に書く（**Red**: `theme.ts` 未作成なので import が解決せず失敗）。
3. `frontend/src/lib/theme.ts` を §6.2 のコードで新規作成（`Theme`, `STORAGE_KEY`, `initialTheme`, `applyTheme`, `theme`, `setTheme`, `toggleTheme`, `initTheme` をエクスポート）。`cd frontend && pnpm test` で §9.3 を **Green** にする。
4. `frontend/src/components/ui/switch.tsx` を新規作成（§6.3）。`@ark-ui/solid/switch` を import し `cn` + トークンで装飾。**ark-ui.com（Solid / Switch）または devtools で `data-[state=checked]` / `data-[focus-visible]` が現物に出ることを一度確認**し、装飾セレクタを実物に合わせる。
5. `frontend/src/components/layout/ThemeToggle.tsx` を新規作成（§6.4）。`Switch` と `theme`/`setTheme` を結線（`components/layout/` ディレクトリを作成）。
6. `frontend/src/index.tsx` を編集: `./lib/theme` から `initTheme` を import し、`render()` の直前に `initTheme()` を追加（§6.5）。
7. `frontend/src/App.tsx` を編集: `@/components/layout/ThemeToggle` を import し、ヘッダ右側に `<ThemeToggle />` を設置（§6.6）。
8. （推奨・任意）`frontend/index.html` の `<head>` 先頭にゼロ FOUC 用インライン script を追加（§6.5）。
9. `pnpm dev`（または `just front`）で起動し、§9.2 の手動マトリクス 1〜6 を実行（FOUC の 6 は `pnpm build && pnpm preview` で確認）。
10. `just lint`（`tsc --noEmit`）で型・未使用チェックを通す。`just test` でバックエンド + フロント両方のテストが通ることを確認。`prettier` 整形。

## 11. リスク・未決事項・代替案

- **Ark UI Switch の API 差異（要確認・ただし本リポジトリの版は確認済み）**: 同梱の `@ark-ui/solid` 5.37.1 / `@zag-js/switch` 1.41.2 では「チェック状態 = `data-state="checked"`」「フォーカス = Control に `data-focus-visible`」「`onCheckedChange` の details = `{ checked: boolean }`」「`Switch.Root` は `<label>`」を実体確認済み。将来メジャー更新でこれらが変わりうるため、実装時に devtools か ark-ui.com（Solid / Switch）で一度目視し、装飾セレクタを合わせること（`dialog.tsx` と同じ運用）。
- **サブパス解決（要確認）**: `@ark-ui/solid/switch` は `package.json` の `exports` の `"./*"` ワイルドカード（`./dist/components/*/index.js`）で解決されることを実機確認済み。バンドラ更新等で解決が変わった場合は名前空間 import（`import { Switch } from "@ark-ui/solid"` 経由）への切替を検討。
- **残留 FOUC**: §6.5 の通り、本番ビルドはインライン script で解消可能、Vite dev は構造上わずかな素背景が残る（`color-scheme: dark` で軽減）。重複（localStorage キー + 判定）を嫌うなら `index.html` script を入れず `index.tsx` の `initTheme()` のみ（dev/prod とも軽微なちらつき許容）でも可。**推奨はインライン script 採用**。
- **vitest 導入のスコープ**: 本スライスがフロントのテストランナーを初導入する。レビューの要求（TDD 必須ロジックを手動に落とさない）に沿って **本スライスで導入し §9.3 を Green にする**判断。導入で `pnpm install` のグラフが増えるが、`lib/theme.ts` は依存の薄い純粋ロジックでテスト価値が高く、初導入の置き場所として最適。
- **`color-scheme` の置き場所**: `app.css` のトークンに `color-scheme` を入れる手もあるが、`:root`/`.dark` 両方に静的定義すると JS のクラス切替と二重管理になる。JS（`applyTheme` + インライン script）で一元的に当てる方が単一の真実を保てるため本設計はそちらを採る。
- **トグルの最終配置**: 現状はヘッダ。機能 10（Sidebar）/機能 05（Settings）確定後に移設想定。`ThemeToggle` を独立コンポーネントにしてあるので移設は import 1 行 + JSX 1 行。
- **store との二重定義回避**: 機能 10 で `lib/store.tsx` を作る担当は、テーマを再実装せず `lib/theme.ts` を委譲利用すること。本書がそれを前提に `lib/theme.ts` を単一の真実として設計（土台設計フロント §2.3 と整合）。
- **自動（auto）モード非対応**: 本機能は明示 2 値のみ。OS 追従を動的に続けたい要望が出たら、`Theme` に `"system"` を足し `matchMedia` の `change` を購読する拡張になる（別機能）。現状は YAGNI で見送り。
- **自前ボタン代替案**: UI ポリシー（複雑な a11y 部品は Ark UI）に従い Switch を採用。アイコンのみのトグルが好まれる場合は `Button size="icon"` + sun/moon（`lucide-solid` 導入後）+ `toggleTheme()` で代替可能。その場合 `switch.tsx` は不要になるが、汎用 Switch は他機能（11 の代替等）でも使えるため作っておく価値はある。
