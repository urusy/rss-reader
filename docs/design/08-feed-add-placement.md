# 08 フィード追加 UI の配置見直し

## 1. 概要

現状、記事一覧ルート（`frontend/src/routes/FeedList.tsx`、実体は「記事一覧」）の最上部に、フィード追加用の URL `input` + 追加ボタンが**常駐**している（`FeedList.tsx:28-39`）。フィード追加は日常的な閲覧体験では低頻度（初期セットアップや稀な購読追加）であり、毎回の閲覧で一覧の一番目立つ位置を占有するのは情報設計上の無駄である。

本機能では、この常駐 input を撤去し、**Ark UI Dialog で起動する再利用可能なフィード追加コンポーネント `AddFeedDialog`** を新設する。トリガーボタンを目立たない場所（既定は左ペイン下部の Sidebar、補助として `/manage` 管理ビュー）に置くことで、閲覧画面を記事に集中させつつ、追加機能はワンクリックで到達できる状態を保つ。あわせて、現状の `alert()` による失敗通知をダイアログ内インライン表示に改善する。**バックエンド・DB の変更は一切ない**（既存 `POST /api/feeds` / `api.addFeed` を再利用）。

## 2. スコープ / 非スコープ

### スコープ（含む）
- 記事一覧ルートから「フィード追加 input + ボタン」ブロックを撤去する。
- 配置非依存の再利用コンポーネント `frontend/src/components/feeds/AddFeedDialog.tsx` を新設（Ark UI Dialog をラップ、トリガーボタン + URL 入力フォーム）。
- 共有 UI プリミティブ `frontend/src/components/ui/input.tsx`（自前 Tailwind の薄いラッパ。**variant を持たないため cva は使わない** — §6.2 注記）を新設。現状 FeedList にインラインで書かれている input スタイルを昇格させ、05/02 でも再利用できるようにする。
- 追加成功時に親へ通知する `onAdded` コールバックを用意し、一覧・未読数などの再取得をフックできるようにする。
- 失敗時のエラーをダイアログ内にインライン表示する（`alert()` 廃止）。
- AddFeedDialog の**配置（mount）先**を定義する: 既定 = Sidebar 下部（#10 提供）、補助 = `/manage`（#01 提供）、interim = 記事一覧ツールバー右寄せ（#10 着地前の単独出荷用）。
- `dialog.tsx` 冒頭の使用例コメント（`dialog.tsx:19-26`）を、誤った `as={Button}` 形から正しい **`asChild` レンダープロップ形**へ修正する（誤パターンの他スライスへの伝播を止める。§10 手順）。

### 非スコープ（含まない）
- バックエンドの追加・変更（`POST /api/feeds` のシグネチャもエラー応答も変更しない）。
- DB マイグレーション。
- 重複 URL 追加時のバックエンド応答改善（現状 UNIQUE 制約違反が 500 になりうる点は §11 に既知課題として記載のみ）。
- OPML 一括インポート、フィード検出（HTML からの feed autodiscovery）。
- 二ペインシェル本体（#10）、Sidebar / FeedTree 本体（#10/#02）、管理ビュー本体（#01）の実装。本書はそれらに**マウントポイントを1つ追加するだけ**。
- ダークテーマ・フィルタ等のグローバル状態（本機能は触れない）。
- フロントエンドのテストランナー（Vitest 等）の導入そのもの。これはフロント横断のインフラ追加であり foundation レベルの決定（§9.3 で扱いを明記）。

## 3. 既存実装の調査と再利用

実ファイルを確認済み。再利用する資産は以下。**車輪の再発明をしない。**

| 資産 | 場所 | 本機能での使い方 |
|------|------|------------------|
| `api.addFeed(url)` | `frontend/src/lib/api.ts:43` | そのまま使用。`POST /api/feeds {url}` を呼び `Feed` を返す。**新メソッド不要。** |
| `Feed` 型 | `frontend/src/lib/api.ts:3` | `onAdded(feed: Feed)` の型に使用。変更不要。 |
| `Dialog` / `DialogContent` / `DialogTitle` / `DialogDescription` / `DialogTrigger` / `DialogCloseTrigger` | `frontend/src/components/ui/dialog.tsx` | Ark UI Dialog ラップ済み。`Dialog = ArkDialog.Root` の直接 re-export なので**制御プロップ `open` / `onOpenChange` / `initialFocusEl` / `unmountOnExit` 等を Root にそのまま透過できる**。`DialogContent` は Backdrop + Positioner + Portal を内包済み。 |
| `Button`（variant: default/outline/ghost/destructive, size: default/sm/icon） | `frontend/src/components/ui/button.tsx` | トリガー / 送信 / キャンセルボタンに使用。`splitProps` 後に `{...rest}` を `<button>` へ spread するため、Ark の `asChild` が渡す属性（`ref` 含む）を正しく転送できる。 |
| input のスタイル文字列 | `FeedList.tsx:30`（`flex-1 h-9 rounded-md border border-input bg-background px-3 text-sm focus-visible:...`） | このクラス列を `components/ui/input.tsx` に昇格して再利用（インラインの重複を解消）。 |
| `cn()` ユーティリティ | `frontend/src/lib/utils.ts` | input プリミティブのクラス合成に使用。 |
| oklch デザイントークン（`bg-background` / `border-input` / `text-muted-foreground` / `text-destructive` / `ring-ring` 等） | `frontend/src/app.css` | 装飾はトークンのみ。生 hex を持ち込まない。 |

### 3.1 Ark UI v5 の制御パターンと `asChild`（実装前に必読・検証済み）

インストール済みバージョン `@ark-ui/solid@5.37.1` の型定義を直接確認した結果、**Solid 版の `asChild` は boolean ではなく「レンダープロップ関数」**である。`components/factory.d.ts` 抜粋:

```ts
type ParentProps<T extends ElementType> =
  (userProps?: JSX.IntrinsicElements[T]) => JSX.HTMLAttributes<any>;
type PolymorphicProps<T extends ElementType> = {
  /** Use the provided child element as the default rendered element, combining their props and behavior. */
  asChild?: (props: ParentProps<T>) => JSX.Element;
};
```

つまり `asChild` は **`(props) => JSX.Element` を受け取り、その `props` 自身も関数**である。`props()` を呼んで属性オブジェクトを取り出し、自前要素へ spread する:

```tsx
<DialogTrigger asChild={(p) => <Button {...p()} variant="outline">…</Button>} />
```

- **`as={Button}` は Ark UI v5 の有効なプロップではない**（Kobalte/Corvu/solid-aria の流儀）。`as` を使うと `strict` + `noUnusedLocals` の本リポジトリの tsconfig 下で `tsc --noEmit` が失敗する。本書のコードはすべて `asChild` レンダープロップ形で記述する。
- `Dialog.Root`（= 本リポジトリの `Dialog`）は制御モードで `open` / `onOpenChange(details)`（`details.open: boolean`）を受ける。`initialFocusEl?: () => HTMLElement | null`、`lazyMount?: boolean`、`unmountOnExit?: boolean` も Root のプロップとして利用可能（v5.37.1 型で確認済み）。
- Ark UI のコンパウンド API はメジャー更新で変わりうる。**実装時に必ず ark-ui.com（Solid / Dialog, Composition）で `asChild` の最新シグネチャと part 名を再確認**すること（`dialog.tsx` 冒頭の注意書きと同じ運用）。

## 4. データモデルとマイグレーション

**DB 変更なし。** 新テーブル・新カラム・新マイグレーションファイルとも不要。フィード追加は既存 `feeds` テーブル（`0001_init.sql`）と既存エンドポイントで完結する。

## 5. バックエンド設計

**バックエンド変更なし。** 既存の `POST /api/feeds {url}`（作成 + 即時取得、`Feed` を返す）をそのまま利用する。新スライス・既存スライス拡張・`features/mod.rs` への `.merge()` 追加はいずれも発生しない。

参考（変更しないが、フロントが依存する既存挙動）:
- リクエスト: `{ "url": "<string>" }`。`feeds` スライスの `domain.rs` 内 `FeedUrl::parse` が URL を検証し、不正なら `AppError::Validation`（HTTP 400）。**URL の妥当性検証はバックエンドが唯一の権威**であり、フロントは形式チェックを行わない（§6.3 の input は `type="text"`）。
- 成功時: 作成された `Feed` を JSON で返す。作成と同時にフィードを即時取得するため、**レスポンスに数秒かかりうる**（フロントは busy 状態で吸収する）。
- 既存の URL（UNIQUE 制約）を渡した場合の応答は本書では変更しない（§11 既知課題）。

## 6. フロントエンド設計

### 6.1 追加・変更ファイル一覧

| 種別 | パス | 内容 |
|------|------|------|
| 新規 | `frontend/src/components/ui/input.tsx` | 自前 Tailwind の `Input` プリミティブ（共有・cva なし）。 |
| 新規 | `frontend/src/components/feeds/AddFeedDialog.tsx` | Ark UI Dialog をラップした配置非依存のフィード追加コンポーネント。 |
| 変更 | 記事一覧ルート（現 `frontend/src/routes/FeedList.tsx`。#10 で `ArticleList.tsx` にリネーム予定） | 最上部の input ブロックを撤去。interim 配置時のみ、ツールバーに `<AddFeedDialog>` を右寄せでマウント。 |
| 変更 | `frontend/src/components/ui/dialog.tsx` | 冒頭の使用例コメント（L19-26）の `as={Button}` を `asChild` レンダープロップ形へ修正（§10 手順3）。エクスポート本体は変更しない。 |
| 変更（配置先・#10 提供時） | `frontend/src/components/layout/Sidebar.tsx` | 下部に `<AddFeedDialog>` を全幅 `outline` ボタンとしてマウント。 |
| 変更（配置先・#01 提供時、任意） | `frontend/src/routes/FeedManage.tsx` | ヘッダ右に同じ `<AddFeedDialog>` をマウント。 |

> **設計の要: 「フィード追加の能力（AddFeedDialog）」と「配置（どこに mount するか）」を分離する。** AddFeedDialog はトリガーボタン + ダイアログだけを持ち、置かれる場所を知らない。これにより Sidebar・管理ビュー・interim ツールバーの3箇所が同一コンポーネントを再利用でき、#10/#01 の進捗に関わらず能力を失わない。

#### `components/feeds/` という新ディレクトリについて（taxonomy の正当化）
foundation のコンポーネント分類は `components/ui/`（データに依存しない汎用プリミティブ: button/card/dialog/input）と `components/layout/`（シェル専用: Sidebar/MobileTopBar）の2つ。AddFeedDialog は **`lib/api` に依存する「機能複合コンポーネント」**で、どちらにも当てはまらない（`ui/` 不可: データ層を参照する。`layout/` 不可: シェル部品ではなく複数箇所に置かれる widget）。したがって **`components/feeds/`（機能スコープの複合コンポーネント置き場）** を新設し、これを最小の foundation 拡張として明示する。05（`components/instapaper/` 等）/ 02（`components/folders/` 等）も同じ規約に乗れる。
- **代替（チームが新フォルダを避けたい場合）**: 既定 mount 先である Sidebar の隣に `components/layout/AddFeedDialog.tsx` として置いてもよい。本書は `components/feeds/` を既定とするが、この点は foundation 担当と着手前に1度すり合わせること。

### 6.2 `components/ui/input.tsx`（新規・共有プリミティブ）

`FeedList.tsx:30` にインライン化されている input クラスを昇格する。05（Instapaper 資格情報）/ 02（フォルダ名）でも再利用される共有部品なので、**もし他機能が先に作っていれば再実装せず再利用する**こと。

```tsx
import { splitProps, type ComponentProps } from "solid-js";
import { cn } from "@/lib/utils";

export function Input(props: ComponentProps<"input">) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <input
      class={cn(
        "flex h-9 w-full rounded-md border border-input bg-background px-3 text-sm",
        "placeholder:text-muted-foreground",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        "disabled:cursor-not-allowed disabled:opacity-50",
        local.class,
      )}
      {...rest}
    />
  );
}
```

> **cva を使わない点に注意（並行開発者向け）**: foundation の表では Input を「自前 cva」と記載しているが、Input には variant が存在しない（サイズ/色のバリエーションを持たない単一形）。そのため `Button` と異なり cva を導入しない。これは意図的な逸脱である。**05/02 の担当者はこの Input をそのまま再利用し、competing な cva 版 Input を別途作らないこと。** バリアントが本当に必要になった時点で cva 化を1ファイル内で行う。
> `{...rest}` が `ref` を含む `ComponentProps<"input">` を `<input>` へ転送するため、§6.3 のように親から `ref` を渡してフォーカス対象の DOM を取得できる。

### 6.3 `components/feeds/AddFeedDialog.tsx`（新規）

配置非依存。`onAdded` で親に成功を通知する以外、外部状態に依存しない。制御モードのダイアログで成功時にプログラム的に閉じる。

```tsx
import { createSignal, Show } from "solid-js";
import { api, type Feed } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogTrigger,
  DialogCloseTrigger,
} from "@/components/ui/dialog";

type Props = {
  /** 追加成功時に親へ通知（記事一覧/フィード一覧/未読数の再取得などに使う） */
  onAdded?: (feed: Feed) => void;
  /** トリガーボタンの表示 */
  triggerLabel?: string;
  triggerVariant?: "default" | "outline" | "ghost";
  /** Sidebar 下部では w-full、管理ビューでは auto などを渡す */
  triggerClass?: string;
};

export default function AddFeedDialog(props: Props) {
  const [open, setOpen] = createSignal(false);
  const [url, setUrl] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  // フォーカス対象の DOM 参照（Ark Dialog の initialFocusEl に渡す。autofocus は portal + 条件付き
  // マウント下で不安定なため使わない）
  let inputEl: HTMLInputElement | undefined;

  const reset = () => {
    setUrl("");
    setError(null);
    setBusy(false);
  };

  const submit = async (e: Event) => {
    e.preventDefault();
    const value = url().trim();
    if (!value) {
      // 空のみフロントで弾く。URL 形式の妥当性検証はバックエンド FeedUrl::parse が権威。
      setError("URL を入力してください。");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const feed = await api.addFeed(value); // 既存 POST /api/feeds
      props.onAdded?.(feed);
      setOpen(false); // 制御モードで閉じる
      reset();
    } catch (err) {
      // バックエンドの 400/5xx 本文をそのまま見せる（alert は使わない）
      setError(`追加に失敗しました: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open()}
      onOpenChange={(e) => {
        setOpen(e.open);
        if (!e.open) reset(); // 閉じたらフォームを初期化
      }}
      initialFocusEl={() => inputEl ?? null}
      // 任意: フォームの DOM を毎回まっさらにし、再オープン時のフォーカス問題も避けたいなら付与
      // unmountOnExit
    >
      {/* Ark UI v5 Solid: asChild は (props)=>JSX のレンダープロップ。props() を呼んで spread する */}
      <DialogTrigger
        asChild={(p) => (
          <Button
            {...p()}
            variant={props.triggerVariant ?? "outline"}
            class={props.triggerClass}
          >
            {props.triggerLabel ?? "+ フィード追加"}
          </Button>
        )}
      />

      <DialogContent>
        <DialogTitle>フィードを追加</DialogTitle>
        <DialogDescription>
          RSS / Atom フィードの URL を入力してください。
        </DialogDescription>

        <form onSubmit={submit} class="mt-4 space-y-3">
          <Input
            ref={inputEl}
            type="text"
            inputmode="url"
            placeholder="https://example.com/feed.xml"
            value={url()}
            onInput={(e) => setUrl(e.currentTarget.value)}
            disabled={busy()}
          />
          <Show when={error()}>
            <p class="text-sm text-destructive">{error()}</p>
          </Show>
          <div class="flex justify-end gap-2">
            <DialogCloseTrigger
              asChild={(p) => (
                <Button {...p()} type="button" variant="ghost" disabled={busy()}>
                  キャンセル
                </Button>
              )}
            />
            <Button type="submit" disabled={busy()}>
              {busy() ? "追加中…" : "追加"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
```

注意点（レビュー指摘の反映を含む）:
- **入力は `type="text" inputmode="url"`**（`type="url"` は使わない）。`type="url"` だと submit 時にブラウザのネイティブ URL 制約検証が走り、`notaurl` や スキーム無しの `example.com` は **submit ハンドラに到達する前に**ネイティブ吹き出しでブロックされる。その結果 API 呼び出しが発生せず、§9.2 の **テスト#6（不正 URL → backend 400 をインライン表示）が到達不能**になり、かつ「URL 検証はバックエンド `FeedUrl::parse` が権威」という本設計の意図に反する。`type="text"` で全形式検証をバックエンドへ委譲する。`inputmode="url"` はモバイルキーボード最適化のみで検証は行わない。
- **`asChild` はレンダープロップ**。`as={Button}` ではない（§3.1）。`p` は関数なので `p()` を呼んでから spread する。Ark が渡す `ref` 等は `Button` → `<button>` へ `{...rest}` 経由で正しく転送される。
- **フォーカスは `initialFocusEl` で行う**（`autofocus` 属性は使わない）。`autofocus` は DialogContent が Portal + 条件付きマウントのとき初回挿入時しか発火せず、再オープン時に効かないうえ Ark の focus 管理と競合しうる。`initialFocusEl={() => inputEl ?? null}` で Ark に開閉のたびフォーカス対象を委ねる。手動テスト#2 でフォーカスを確認する。
- `<form onSubmit>` + `<Button type="submit">` により Enter キー送信が a11y 的に正しく効く（現状 FeedList は `onKeyDown` で Enter を拾っていたが、フォーム化でブラウザ標準に委ねる）。
- 即時取得でレスポンスが数秒かかりうるため、`busy()` で input/ボタンを無効化し、ボタン文言を「追加中…」にする。
- **任意で `unmountOnExit`（必要なら `lazyMount` も）を `<Dialog>` に付与してよい**（Ark 推奨）。これで開くたびに DialogContent の DOM が作り直され、フォームが物理的に新鮮になり、再オープン時の focus 問題も回避できる。ただし `url`/`error`/`busy` は AddFeedDialog 側の signal なので、いずれにせよ閉時の `reset()` で signal を初期化する（`unmountOnExit` は signal をリセットしない点に注意）。

### 6.4 配置（mount）

AddFeedDialog を3箇所で再利用する。**`useApp()` のような Solid コンテキストアクセサ（`useContext`）は、必ずコンポーネント本体（同期トラッキング文脈）で読み、イベントハンドラ内で呼ばない**（後述のレビュー指摘）。

1. **既定 — Sidebar 下部（#10 提供の `components/layout/Sidebar.tsx`）**
   - Sidebar の最下部、設定/管理リンク群の近くに全幅で配置。
   - **重要**: `useApp()` は Sidebar コンポーネント本体の先頭で1度だけ読み、`onAdded` ではその束縛を使う。`onAdded={() => useApp().counts.refresh()}` は **NG**（イベントハンドラは Solid のネイティブハンドラで reactive owner を持たないため、`useContext` が provider ではなくデフォルト値 `undefined` を返し `.counts.refresh()` が実行時に throw する）。
   ```tsx
   // components/layout/Sidebar.tsx（#10 提供）
   export default function Sidebar() {
     const app = useApp(); // ← 本体で読む（イベントハンドラ内で useApp() を呼ばない）
     // ...
     return (
       // ...
       <AddFeedDialog
         triggerClass="w-full justify-start"
         onAdded={() => app.counts.refresh()} // 未読数リソースを再取得
       />
     );
   }
   ```
   - `useApp()`（土台のグローバルストア）が未提供の段階では `onAdded` を省略するか、Sidebar 側でフィードツリーの `refetch()`（同じく本体で取得した束縛）を渡す。

2. **補助 — 管理ビュー（#01 提供の `routes/FeedManage.tsx`）、任意**
   - ヘッダ右に配置。`refetchFeeds` は FeedManage 本体で得た `createResource` の refetch を束縛して渡す。
   ```tsx
   <AddFeedDialog triggerVariant="default" triggerLabel="+ フィード追加" onAdded={refetchFeeds} />
   ```

3. **interim — 記事一覧ツールバー（#10 着地前の単独出荷用）**
   - 記事一覧ルートの最上部 input ブロックを撤去し、代わりに控えめなツールバーを置く。常駐 input よりはるかに目立たない、右寄せのトリガーボタン1個にする。`refetch` は記事一覧本体の `createResource` から束縛。
   ```tsx
   // routes/FeedList.tsx（#10 後は ArticleList.tsx）
   // const [articles, { refetch }] = createResource(() => api.listArticles());  ← 既存をそのまま使用
   <div class="flex justify-end">
     <AddFeedDialog triggerVariant="ghost" onAdded={() => refetch()} />
   </div>
   ```

### 6.5 状態管理・ルーティング

- 新しいグローバル状態は導入しない。ダイアログの開閉・フォーム値・busy・error はすべて AddFeedDialog 内のローカル signal に閉じる。
- ルーティング変更なし（モーダルのためルートを増やさない）。
- 追加後の再取得は `onAdded` コールバック経由で各 mount 元が責任を持つ（土台の「ミューテーション後に `counts.refresh()`」方針に合致）。**`onAdded` に渡す関数が context/store を参照する場合、その読み取りは mount 元コンポーネント本体で済ませてからクロージャに束縛する**（§6.4 の規約）。

### 6.6 必要な Ark UI 部品

- **Dialog** のみ（既存 `components/ui/dialog.tsx` を再利用、新規ラップ不要）。Switch/Select/TreeView 等は本機能では不要。
- 制御プロップ（`open`/`onOpenChange`）・`asChild` レンダープロップ・`initialFocusEl`/`unmountOnExit` の形は §3.1 で v5.37.1 型から確認済み。**実装時に ark-ui.com（Solid / Dialog, Composition）で最終確認**する。

## 7. API 契約

**新規・変更エンドポイントなし。** 既存 `POST /api/feeds` を利用する。参考として契約を明示（変更しない）。

Request
```
POST /api/feeds
Content-Type: application/json

{ "url": "https://example.com/feed.xml" }
```

Response 200 OK（作成 + 即時取得後）
```json
{
  "id": "0f8b...uuid",
  "url": "https://example.com/feed.xml",
  "title": "Example Blog",
  "created_at": "2026-06-26T01:23:45Z",
  "last_fetched_at": "2026-06-26T01:23:46Z"
}
```

エラー（フロントはダイアログ内 `text-destructive` で `http()` が投げた本文を表示）
- 400 Validation: URL が不正（`FeedUrl::parse` 失敗）。**`type="text"` 採用により、不正形式でもネイティブ検証で握り潰されず submit → API → 400 表示まで到達する。**
- 502 Upstream / 500: 取得先が応答しない等。
- 既存 URL（UNIQUE 違反）時の応答は現状未定義のため §11 参照。

## 8. 依存関係

- **依存（配置先を提供する機能）**:
  - `two-pane-layout`（#10）— 既定の mount 先である Sidebar 下部と、`useApp().counts`（未読数ストア）を提供。**ただしハードブロックではない**: #10 着地前は §6.4-3 の interim 配置（記事一覧ツールバー右寄せ）で単独出荷できる。
  - `feed-management`（#01）— 補助の mount 先 `/manage` を提供（任意）。
- **同居して整合すべき機能**:
  - `minimal-design`（#07）— トリガーは中立基調・控えめに（常駐入力からの脱却自体が #07 の方向性に合致）。
  - `feed-folders`（#02）— 将来、ダイアログにフォルダ選択（`select`）を追加する余地（本書では非スコープ）。
- **本機能が再利用させる資産**: `components/ui/input.tsx` は 05（Instapaper）/ 02（フォルダ名）も使う共有プリミティブ。先に存在すれば再利用、なければ本機能が作る（§6.2 の cva なし方針を共有）。

## 9. テスト計画（TDD）

### 9.1 型・lint（自動・本変更の Red→Green ゲート）
- 本変更はフロントエンドのみで、現状プロジェクトにフロント用テストランナーが無い。したがって **`just lint`（`tsc --noEmit` typecheck / prettier）の green を本変更の実質的な Red→Green ゲート**とする。
- **重要**: 旧ドラフトの `as={Button}`（Ark v5 に存在しないプロップ）のままでは `strict` + `noUnusedLocals` の tsconfig 下で `tsc` が**失敗（Red）**する。§3.1/§6.3 の `asChild` レンダープロップへの書き換えを適用して初めて **tsc green（Green）** になる。`Props`・`onAdded(feed: Feed)`・`onOpenChange` の `details.open`・`asChild` の `(p) => JSX` 型・`initialFocusEl` の `() => HTMLElement | null` がすべて解決することを確認する。
- `inputmode`（小文字）・`ref`（`ComponentProps<"input">` に含まれる）は Solid の JSX 型で受理されることを確認済み。

### 9.2 手動テストマトリクス
`just front` で起動し、ライト/ダーク両方で確認する。各項目は「修正前は失敗 = Red」を意図する。

| # | 操作 | 期待（Green） | Red の意図 |
|---|------|----------------|------------|
| 1 | 記事一覧を開く | 最上部に常駐 input が**存在しない**。トリガーボタンが目立たない位置に1個 | 撤去前は常駐 input が見える |
| 2 | トリガーを押す | Dialog が中央に開き、URL 入力に**フォーカスが入る**（`initialFocusEl`） | autofocus 依存だと再オープンで効かない |
| 3 | 正常な URL を入力し「追加」 | 数秒の「追加中…」後、ダイアログが閉じ、一覧/未読数が更新（`onAdded`） | 制御 close 未実装なら開いたまま |
| 4 | 入力中に Enter | 「追加」と同等に送信される | form 化前は挙動不安定 |
| 5 | 空のまま「追加」 | 「URL を入力してください。」がインライン表示、API 呼び出しなし | 検証なしだと空送信 |
| 6 | 不正な URL（例 `notaurl`）を入力し「追加」 | submit が発火し API が呼ばれ、**backend 400 本文がダイアログ内に `text-destructive` で表示**、ダイアログは開いたまま | `type="url"` だとネイティブ検証で握り潰され API に到達しない／`alert()` のままだと OS ダイアログ |
| 7 | 失敗後にキャンセル/閉じる→再度開く | フォームが初期化（前回の値・エラーが残らない） | `reset()`（or `unmountOnExit`）未実装だと残る |
| 8 | 追加中に「キャンセル」「追加」 | busy 中は無効化されている | 無効化漏れだと二重送信 |
| 9 | ダークモードで表示 | Backdrop/枠/エラー文がトークンで適切に見える | 生色だと不整合 |
| 10 | （#10 後）Sidebar 下部から追加 | Sidebar を再マウントせず、未読数バッジが更新（`app.counts.refresh()`） | `useApp()` をハンドラ内で呼ぶと throw／配線漏れでバッジが古い |

### 9.3 自動テストの扱い（TDD 規約との整合）
プロジェクトの TDD 規約（「書いたら必ず実行」）に厳密に従うなら、AddFeedDialog の2つの純粋ふるまいは安価に単体テスト化できる:
1. **空送信 → `error()` が立ち、`api.addFeed` が呼ばれない**（`api.addFeed` をモック）。
2. **成功 → `onAdded` が返却 `Feed` で呼ばれ、`open()` が `false` になる**。

ただし**フロント用テストランナー（Vitest + `@solidjs/testing-library`）の導入はフロント横断のインフラ追加であり、本スライス単独の決定事項ではない（foundation レベル）**。方針は次のいずれかを着手前に確定する:
- **推奨**: 本タスクで Vitest + `@solidjs/testing-library` を最小スキャフォールド（`vitest.config.ts` + `package.json` の `test` スクリプト + `just` への `test-front` 追加）し、上記2テストを Red→Green で実装する。以後の全フロント機能がこの基盤に乗れる。
- **代替（サインオフ要）**: ランナー導入を別タスクへ明示的に延期する。その場合は §9.1 の **tsc green + §9.2 の手動マトリクス**を本変更の Red→Green ゲートとして合意する。上記2テストの内容は本書で確定済みなので、ランナー導入後すぐ書ける。

## 10. 実装手順（チェックリスト）

1. `frontend/src/components/ui/input.tsx` を新設（§6.2）。**既存があれば再利用してこの手順をスキップ**（05/02 と共有・cva なし）。
2. `frontend/src/components/feeds/AddFeedDialog.tsx` を新設（§6.3）。`api.addFeed` / 既存 `dialog.tsx` / `Button` / `Input` を import。**`asChild` レンダープロップ・`initialFocusEl`・`type="text" inputmode="url"` を §6.3 の通りに記述する**（`as={Button}` / `type="url"` / `autofocus` を使わない）。
3. `frontend/src/components/ui/dialog.tsx` 冒頭の使用例コメント（L19-26）を修正し、誤った `as={Button}` 形を **`asChild` レンダープロップ形**に置き換える（例: `<DialogTrigger asChild={(p) => <Button {...p()}>削除</Button>} />`、`<DialogCloseTrigger asChild={(p) => <Button {...p()} variant="outline">キャンセル</Button>} />`）。誤パターンの伝播を止めるための変更で、エクスポート本体には触れない。
4. ark-ui.com（Solid / Dialog, Composition）で `Dialog.Root` の `open`/`onOpenChange`/`initialFocusEl`/`unmountOnExit`、および `Trigger`/`CloseTrigger` の `asChild` レンダープロップ・シグネチャを最終確認し、必要なら §6.3 を微調整。
5. 記事一覧ルート（現 `FeedList.tsx`）から最上部の input + 追加ボタンブロック（`FeedList.tsx:28-39`）を撤去。`url`/`busy` signal と `addFeed` 関数（`FeedList.tsx:8-24`）を削除（`createResource`/`refetch` は残す）。**`noUnusedLocals` のため、未使用になった import（`createSignal` 等）も整理する。**
6. interim 配置として、同ルートの一覧上に `<div class="flex justify-end"><AddFeedDialog triggerVariant="ghost" onAdded={() => refetch()} /></div>` を置く（§6.4-3）。
   - **#10 が既に着地している場合**: interim を入れず、`components/layout/Sidebar.tsx` 本体で `const app = useApp();` を取得してから下部に `<AddFeedDialog triggerClass="w-full justify-start" onAdded={() => app.counts.refresh()} />` をマウント（§6.4-1）。記事一覧側にはトリガーを置かない。
7. （#01 着地済みなら任意）`routes/FeedManage.tsx` ヘッダに `<AddFeedDialog triggerVariant="default" onAdded={refetchFeeds} />` を追加（`refetchFeeds` は本体で束縛）。
8. （§9.3 で「推奨」を選んだ場合）Vitest + `@solidjs/testing-library` をスキャフォールドし、空送信/成功の2単体テストを Red→Green で追加。
9. `just lint`（tsc / prettier）を通す（§9.1 が本変更のゲート）。
10. `just front` で開発サーバを起動し、§9.2 のマトリクスを手動確認（ダーク/ライト両方）。
11. （#10 後に本作業をした場合）記事一覧の interim トリガーを撤去し、Sidebar 下部の mount に一本化する（トリガーが二重に残らないよう統合時に確認）。

## 11. リスク・未決事項・代替案

- **Ark UI v5 の `asChild` 形**: `@ark-ui/solid@5.37.1` の型定義（`components/factory.d.ts`）で **`asChild?: (props) => JSX.Element`（props 自身も関数）** であることを確認済み（§3.1）。旧ドラフトの `as={Button}` は v5 に存在しないため tsc が落ちる。実装時に念のため ark-ui.com で最終確認するが、本書のコードは検証済み型に基づく。
- **`initialFocusEl` とカスタム Input の ref**: `ref={inputEl}` は `let inputEl: HTMLInputElement | undefined` へ Solid のコンパイル時 ref 代入で束縛され、`Input` が `{...rest}` で `<input>` へ転送する。`unmountOnExit` 併用時もオープンのたびに ref が再設定され `initialFocusEl` は現値を読む。万一フォーカスが入らない場合は `onOpenChange(open=true)` 内で `queueMicrotask(() => inputEl?.focus())` にフォールバック可（手動テスト#2 で確認）。
- **配置先の所有権（#10/#01 との順序）**: 既定 mount 先の Sidebar は #10、補助の `/manage` は #01 が提供する。本書は「能力（AddFeedDialog）」を先に確定し、配置は interim（記事一覧ツールバー）で単独出荷可能にすることでブロックを回避する。#10 着地後に mount を Sidebar へ移し interim を撤去（手順11）。co-development 前提のため、最終的にトリガーが**二重に**残らないよう統合時に確認すること。
- **即時取得による遅延**: `POST /api/feeds` は作成と同時にフィードを取得するため数秒かかりうる。busy 状態で吸収するが、取得が極端に遅い/タイムアウトする場合 UX が悪化する。将来、作成と取得を分離（202 で受理→バックグラウンド取得）する改善余地があるが本機能のスコープ外（バックエンド変更を伴うため）。
- **重複 URL の応答**: 既存 URL を追加すると `feeds.url` の UNIQUE 制約違反で現状おそらく 500 が返る。ダイアログ内にエラー本文は出るが文言が分かりにくい。UX 改善には backend で UNIQUE 違反を `AppError::Validation`（400,「既に登録済みです」）にマップするのが望ましいが、**本機能はバックエンド非変更**のため別機能（feeds スライス拡張）として扱う。本書では既知課題として記載のみ。
- **`onAdded` と未読数整合**: #10 の `useApp().counts.refresh()` が未提供の段階では、mount 元（Sidebar/記事一覧）が本体で束縛したローカル `refetch` を渡す。store 提供後に差し替える。**いずれの場合も context/store の読み取りはコンポーネント本体で行い、イベントハンドラ内で `useContext`/`useApp()` を呼ばない**（§6.4）。
- **input プリミティブの重複新設**: `components/ui/input.tsx` は 05/02 も必要とする共有部品。並行開発で二重に作られないよう、着手時に既存有無を確認し、あれば再利用する。cva 版を別途作らない（§6.2）。
- **`components/feeds/` ディレクトリ**: foundation の taxonomy（`ui/`/`layout/`）に無い新フォルダ。§6.1 で「機能複合コンポーネント置き場」として正当化したが、05/02 にも波及する規約なので foundation 担当と着手前に1度合意する（代替: `components/layout/AddFeedDialog.tsx` への colocate）。
