# 07 ミニマルデザインと視認性向上

> 種別: フロントエンドのみ（バックエンド・マイグレーション・API 変更なし）
> 位置づけ: 全画面に適用する**横断デザイン指針**と、07 が直接所有する**少数の具体プリミティブ/単独変更**。土台設計 `docs/design/00-foundation-frontend.md` の「§5 デザイントークン運用とミニマル化」を本書が所有・具体化する。
> 成果物の境界（重要）: 07 が実際に編集/追加するのは **層A**（後述）に限定する。一覧行・ヘッダ・サイドバーの restyle（**層B**）は 07 の編集物ではなく、**#10 two-pane-layout / #08 feed-add-placement が実装時に従うスタイル指針**である（§6.0・§8）。

---

## 1. 概要

セルフホスト RSS リーダーの UI を、情報密度が高く視認性に優れた**ミニマルデザイン**へ統一する。記事を流し読みして要約・翻訳を判断するユーザーにとって、「未読/既読」「選択中」「アクション」が一目で分かり、本文・要約・翻訳が読みやすいことを最優先する。

本書がやることは2系統に分かれる。

- **層A（07 が直接実装する成果物）**: 再利用される基盤プリミティブと、既存ファイルへの単独変更。具体的には (1) `components/ui/badge.tsx` 新規、(2) `components/ui/button.tsx` への加算的バリアント、(3) `lib/format.ts` 新規（絶対短縮日付の純関数）、(4) アイコン基盤 `lucide-solid` の採用提案、(5) 既存 `routes/ArticleView.tsx` の要約セクションを翻訳セクションと同じタイポ運用へ統一。**これらは他機能に依存せず今すぐ単独で出荷できる。**
- **層B（07 が定義し、他機能が従う指針）**: タイポ階層・余白・色運用・`prose` 運用ルール・一覧の罫線リスト化・ヘッダ/サイドバーの軽量化。これらは **#10/#08 がシェルやルートを実装する際に適用する規約**であって、07 自身が `App.tsx` / `Sidebar.tsx` / `ArticleList.tsx` を新設・改名するわけではない。

`app.css` の oklch デザイントークンと `.dark` 配線はそのまま使い続け、新しい色体系や生 hex は一切持ち込まない。これは新機能というより**全画面に効く restyle 規約**であり、#10 二ペインのシェルと #04 ダークテーマの上に薄く積む。

---

## 2. スコープ / 非スコープ

### スコープ（含む）
**層A（07 の編集物）**
- 新規 UI プリミティブ `components/ui/badge.tsx`（自前 Tailwind + cva、未読数表示用）。
- 既存 `components/ui/button.tsx` への**最小・加算的**バリアント追加（`secondary` 変種、密な行向け `icon-sm` サイズ）。既存変種は不変。
- 日付メタ表示用の純関数 `lib/format.ts`（タイムゾーン固定の絶対短縮日付。相対表記「N日前」「週Y件」は #03 feed-stats が**同ファイルに追記**して所有）。
- アイコン基盤 `lucide-solid` の**採用提案**（アプリ全体に効くランタイム依存のため批准事項。不採用時のフォールバックも定義）。
- 既存 `routes/ArticleView.tsx` の要約セクションを翻訳セクションと同じタイポ運用（`prose ... whitespace-pre-wrap`）へ統一する単独変更（外部リンクのアイコン化は任意）。

**層B（07 が定義する横断指針。適用主体は #10/#08 ほか）**
- タイポグラフィ階層 / 余白・密度 / 色運用（中立基調・意味のある所だけ着色） / `prose` 運用ルールの確定（§6.1〜6.5）。
- 記事一覧行の再設計指針: Card 羅列 → `divide-y` 罫線リスト（タイトル＋1行サマリ＋メタ）。
- 右ペインヘッダの軽量化指針とサイドバー行の密度指針（クラス・トークン単位）。
- ライト/ダーク両モード・デスクトップ/モバイル両幅での視認性担保（QA チェックリスト §9.3）。

### 非スコープ（含まない）
- バックエンド・DB・API の変更（一切なし）。
- 二ペインのシェル再構成そのもの・`App.tsx` の再構成・`Sidebar.tsx`/`FeedTree.tsx` の新設・`FeedList.tsx → ArticleList.tsx` の改名（**すべて #10 two-pane-layout が所有**。本書はその上の見た目規約を定義するだけで、これらのファイルを 07 として編集しない）。
- ダークテーマのトグル機構・FOUC 回避（#04 dark-theme が所有。本書は dark での見えを担保する指針だけ）。
- フィード追加 UI の配置変更（#08 feed-add-placement が所有。本書は撤去後の一覧に追加 UI を**再導入しない**だけ）。
- 未読フィルタ・一括既読・未読数集計のロジック（#09/#11/#03 が所有。本書は Badge の見た目のみ提供）。
- フォルダツリー・サイドバーの機能実装（#02/#10）。本書はツリー行の**密度・タイポ**だけを規定。
- 新しい oklch トークンの新色追加（既存トークンのみ使用。どうしても必要な面は §6.1 の手順で `:root` と `.dark` 両方へ）。
- フロントのテストランナー（vitest 等）導入（現状なし。§9 参照）。
- 要約・翻訳の Markdown→HTML リッチ描画（§6.5 のとおり現状はテキストノード表示。将来課題）。

---

## 3. 既存実装の調査と再利用

実ファイルを確認済み（`button.tsx` / `card.tsx` / `ArticleView.tsx` / `FeedList.tsx` / `app.css` / `package.json`）。以下を**再利用**し、車輪の再発明をしない。

| 資産 | 場所（確認済み） | 本書での扱い |
|------|------|------|
| oklch デザイントークン一式 | `frontend/src/app.css`（`:root` / `.dark` / `@theme inline`） | **そのまま維持**。`bg-background` `bg-card` `bg-muted` `text-muted-foreground` `bg-accent` `text-accent-foreground` `bg-secondary` `text-secondary-foreground` `border-border` `bg-destructive` `bg-primary` `text-primary-foreground` `ring-ring` を意味トークンとして使う。生 hex 禁止 |
| ダークモード配線 | `app.css` 5行目 `@custom-variant dark (&:is(.dark *))` | **配線済み**（#04 のトグル有無に関係なく `.dark` クラスが付けば効く）。本書は `dark:prose-invert` 等で dark での見えを担保するのみ |
| Tailwind Typography | `app.css` 2行目 `@plugin "@tailwindcss/typography"` | `prose prose-sm dark:prose-invert max-w-none` を**本文・要約・翻訳の3箇所限定**で使用（§6.5） |
| 本文フォント | `app.css` `body`（system + Hiragino/Noto Sans JP fallback） | 維持。和文可読性のため変更しない |
| Button（cva） | `components/ui/button.tsx`（variant: default/outline/ghost/destructive、size: default/sm/icon） | 再利用。**加算的に** `secondary` variant と `icon-sm` size を足すのみ。既存変種は不変 |
| `--secondary` / `--secondary-foreground` トークン | `app.css` 15-16, 35-36行 | 既に定義済み。`button` の `secondary` 変種・`badge` の `secondary` 変種で利用（新トークン追加なし） |
| Card 一式 | `components/ui/card.tsx`（`rounded-lg border shadow-sm`、Header/Title/Content） | 再利用するが**用途を限定**（囲いが意味を持つ管理画面・本文枠）。記事一覧からは外す |
| Dialog（Ark UI ラップ） | `components/ui/dialog.tsx` | 既存スタイル流用。本書では変更しない |
| `cn()` ユーティリティ | `lib/utils.ts`（clsx + tailwind-merge） | 条件付きクラス結合に使用 |
| `@/` エイリアス | `vite.config.ts`（`@ → ./src`） | import に使用 |
| 記事一覧の既存マークアップ | `routes/FeedList.tsx`（Card 羅列・`is_read` で `text-muted-foreground`・`line-clamp-2`・素 `<a href>`） | 罫線リスト指針の土台。`is_read` 出し分け・`line-clamp` の発想は流用。**改名・add-feed 撤去は #10/#08 が実施** |
| 記事本文タイトル | `routes/ArticleView.tsx` 36行 `h1 text-2xl font-bold tracking-tight` | **そのまま採用**（本文側タイトルの基準値） |
| 翻訳セクションの prose 運用 | `ArticleView.tsx` 71行 `prose prose-sm dark:prose-invert max-w-none whitespace-pre-wrap` | **要約セクションをこれに合わせる**（§6.10。現状要約は素の `<p>`） |
| 既存 focus リング | `button.tsx` 6行 `focus-visible:ring-2 focus-visible:ring-ring` | 全インタラクティブ要素の基準として踏襲 |
| API クライアント | `lib/api.ts`（`Article.is_read` `summary` `translation` `published_at` `title` `content`） | 表示に使うフィールドは既存のまま。**API 追加なし** |

> 重要: ダークは「配線済み」、`is_read` 列・`summary`/`translation`/`published_at` は「取得済み」。本書はそれらの**見せ方**だけを設計する。

---

## 4. データモデルとマイグレーション

**DB 変更なし。** 新規テーブル・カラム・マイグレーションファイルは追加しない。`backend/migrations/` には一切触れない。

---

## 5. バックエンド設計

**バックエンド変更なし。** 新スライス・既存スライス拡張・`features/mod.rs` への `.merge()`・`AppError` 編集のいずれも行わない。表示に必要なフィールド（`is_read` / `summary` / `translation` / `published_at` / `title` / `content`）はすべて既存エンドポイントの JSON に含まれている。

---

## 6. フロントエンド設計

### 6.0 二層構成と成果物境界（最重要 — 別セッション実装者はここを最初に読む）

本書の内容は実装責務の所在で2層に分かれる。**07 を単独で実装するセッションは「層A」だけを編集する。**

| 層 | 内容 | 07 が編集するファイル | 依存 |
|----|------|----------------------|------|
| **層A（07 の成果物）** | `badge.tsx` 新規 / `button.tsx` に変種加算 / `lib/format.ts` 新規 / `lucide-solid` 採用判断 / `ArticleView.tsx` 要約 prose 統一 / 本 §6 指針の確定 | `frontend/src/components/ui/badge.tsx`（追加）, `frontend/src/components/ui/button.tsx`（変更）, `frontend/src/lib/format.ts`（追加）, `frontend/src/routes/ArticleView.tsx`（変更）, `frontend/package.json`（依存追加） | **なし（今すぐ単独出荷可）** |
| **層B（07 が定義する指針。適用は他機能）** | 一覧行の罫線リスト化（§6.11） / ヘッダ・サイドバー軽量化（§6.12） | **07 は編集しない。** これらの対象（`App.tsx` 再構成・`components/layout/Sidebar.tsx`/`FeedTree.tsx` 新設・`routes/ArticleList.tsx`＝`FeedList.tsx` 改名）は **#10 が新設/改名し、#08 が add-feed を撤去する**。07 はその実装が従うべき**スタイル規約**を提供する | **#10 two-pane-layout / #08 feed-add-placement の後**（同一ブランチで重ねる） |

**なぜこの分割か**: §6.11/§6.12 が触れる `routes/ArticleList.tsx`・`components/layout/Sidebar.tsx`・`FeedTree.tsx` は**現リポジトリに存在しない**（現状は `routes/FeedList.tsx` と単一カラム `App.tsx`）。07 がこれらを先に作ると #10/#08 の領域（シェル再構成・サイドバー新設・ルート改名）を踏み、マージ衝突する。よって層B は「07 が `App.tsx` 等を編集する」のではなく、「**#10/#08 がそれらを作る際に従う見た目の決定**」として宣言する。オーケストレータ向けの順序宣言は §8 に置く。

> 適用範囲（どの画面に効くか）: 層A のプリミティブと層B の指針は、#10 二ペイン後の全ルートに適用される — サイドバー（`Sidebar.tsx`/`FeedTree.tsx`）、記事一覧（`ArticleList.tsx`）、記事本文（`ArticleView.tsx`）、管理（`FeedManage.tsx`）、設定（`Settings.tsx`）。

---

### 6.1 デザイントークン運用（oklch 維持） — 層B 指針

- 新色体系・生 hex を持ち込まない。新しい面も `bg-card` / `bg-muted` / `bg-accent` / `bg-secondary` で賄う。
- どうしても専用トークンが要る場合のみ、`app.css` の `:root` **と** `.dark` の**両方**に変数を定義し、`@theme inline` に `--color-*` を追加する（片側だけ定義しない）。**本機能では新トークン追加は想定しない。**
- コンポーネントは生の色を書かず、意味トークンのみ使用する。

### 6.2 タイポグラフィ階層（固定値） — 層B 指針

| 用途 | クラス | 備考 |
|------|--------|------|
| 本文側 記事タイトル | `text-2xl font-bold tracking-tight` | 現 `ArticleView` の h1 を基準採用 |
| 一覧 記事タイトル（未読） | `text-sm font-semibold text-foreground` | **太字のみで未読を示す**（§6.4 の決定） |
| 一覧 記事タイトル（既読） | `text-sm font-normal text-muted-foreground` | トーンダウン |
| 1行サマリ | `text-sm text-muted-foreground line-clamp-1` | 一覧の2段目 |
| メタ（日付等） | `text-xs text-muted-foreground` | 一覧の3段目・本文ヘッダ |
| セクション見出し（要約/翻訳） | `text-sm font-semibold` | 現 `ArticleView` を踏襲 |
| サイドバー項目 | `text-sm`（行高 `h-8`, `px-2`） | 詰める |
| ブランド（サイドバー上部） | `text-sm font-semibold tracking-tight` | アプリ名はサイドバーへ（#10 が配置） |
| 本文・要約・翻訳 | `prose prose-sm dark:prose-invert max-w-none` | **この3箇所のみ**（§6.5） |

### 6.3 余白・密度 — 層B 指針

- セクション間: `space-y-6`。
- 罫線リスト: `divide-y divide-border` ＋ 各行 `py-3`（行間に `space-y` を使わない）。
- カード内: `p-4`（既存 CardHeader/Content 準拠）。
- サイドバー: 項目 `h-8 px-2`、グループ間 `space-y-1`。
- 読書幅: 右ペイン本文は従来どおり `max-w-3xl`。グリッド破綻防止に親へ `min-w-0`（#10 と整合）。

### 6.4 色運用（中立基調 + 意味のある所だけ着色） — 層B 指針

| 状態/要素 | スタイル |
|------|------|
| 選択中（一覧/サイドバー） | `bg-accent text-accent-foreground`（`<A>` の active には `aria-[current]:` で当てる。§6.11/§6.12 と統一） |
| ホバー | `hover:bg-accent`（テキスト色は据え置き or `hover:text-accent-foreground`） |
| **未読（一覧）** | **タイトル太字（`font-semibold text-foreground`）のみ。ドットは付けない** |
| **未読数（サイドバー）** | **`Badge`（数値、`secondary`）**。ドット/Badge と一覧の太字で**役割を分離**し二重表現を避ける（§11 の決定） |
| 既読（一覧） | `text-muted-foreground` でトーンダウン |
| 破壊的操作 | `bg-destructive text-white`（Button `destructive`）。文言は赤に頼らず明示 |
| 罫線 | 1px `border-border` のみ |
| 角丸 | `--radius` 由来（`rounded-md` / `rounded-lg`）のみ。任意 px 角丸禁止 |
| 影 | 最大 `shadow-sm`。一覧行は**影なし**（フラット） |
| フォーカス | 全インタラクティブ要素で `focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring` を維持 |

> **未読アフォーダンスの決定（二重表現の排除）**: 一覧では「**太字のみ**」で未読を示し、ドットは置かない。未読の**数**を示すのはサイドバーの `Badge` だけにする。これにより「一覧のドット」「サイドバーの Badge」の役割が重複しない。太字だけでは弱いと感じた場合の代替は §11 に記す。

### 6.5 `prose` 運用ルール（厳密。テキストノードの限界を明記） — 層B 指針

`prose prose-sm dark:prose-invert max-w-none` を使うのは次の**3箇所だけ**:

1. **記事本文**（`ArticleView` の `innerHTML={a().content}` ブロック）— 既存。**HTML を `innerHTML` で注入するため prose が完全に効く**（見出し/段落/リスト/コードが描画される）。
2. **要約**（`ArticleView` の要約セクション）— 現状は素の `<p whitespace-pre-wrap>`。本書 §6.10 で prose に統一。
3. **翻訳**（`ArticleView` の翻訳セクション）— 既存（`prose ... whitespace-pre-wrap`）。

**重要な注意（誤解防止）**: 要約・翻訳は `api.summarize/translate` の戻り値を **JSX のテキストノードとして挿入**している（`innerHTML` ではない）。したがって LLM が返す Markdown 記法（`##`、`- ` など）は**見出し/リストに描画されず、リテラル文字のまま**表示される。これらに `prose` を当てる目的は **基本タイポgrafi（フォントサイズ・行間・字色・`max-w` リセット）を本文と統一すること**であり、Markdown のリッチ描画ではない。`whitespace-pre-wrap` を併用して改行を保つ。

- リッチ整形（要約の見出し/リスト描画）が将来必要になったら、要約/翻訳を Markdown→HTML 化して `innerHTML` で注入する別タスクが要る（**本書では非スコープ**）。
- それ以外（一覧サマリ・サイドバー・メタ）には `prose` を使わない（密度が崩れるため）。

### 6.6 【層A】新規プリミティブ `components/ui/badge.tsx`（自前 cva）

未読数バッジ（サイドバーのフィード/フォルダ行、#01/#09 が利用）の見た目を本書が提供する。a11y 不要なので自前 Tailwind + cva。

```tsx
import { splitProps, type ComponentProps } from "solid-js";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex h-5 min-w-5 items-center justify-center rounded-full border px-1.5 text-xs font-medium leading-none tabular-nums",
  {
    variants: {
      variant: {
        default: "border-transparent bg-primary text-primary-foreground",
        secondary: "border-transparent bg-muted text-muted-foreground",
        outline: "border-border text-foreground",
        destructive: "border-transparent bg-destructive text-white",
      },
    },
    defaultVariants: { variant: "secondary" },
  },
);

type BadgeProps = ComponentProps<"span"> & VariantProps<typeof badgeVariants>;

export function Badge(props: BadgeProps) {
  const [local, rest] = splitProps(props, ["class", "variant"]);
  return <span class={cn(badgeVariants({ variant: local.variant }), local.class)} {...rest} />;
}

export { badgeVariants };
```

未読数は中立的に見せたいので既定 `secondary`（muted）。数字は `tabular-nums` で桁ブレ防止。利用側（#01/#09）は `<Show when={count() > 0}><Badge>{count()}</Badge></Show>` で 0 を出さない。使用トークンは全て app.css に既存（`bg-primary`/`text-primary-foreground`/`bg-muted`/`text-muted-foreground`/`border-border`/`text-foreground`/`bg-destructive`）。

### 6.7 【層A】既存 `button.tsx` への加算的変更（最小）

ミニマル UI のアイコンボタン用に、既存 cva へ**加算のみ**。既存変種・サイズは不変。

- `variants.variant` に追加: `secondary: "bg-secondary text-secondary-foreground hover:bg-secondary/80"`（`--secondary`/`--secondary-foreground` は app.css 既存）
- `variants.size` に追加: `"icon-sm": "h-8 w-8"`（密な行・サイドバーのアイコンボタン用。既存 `icon` は `h-9 w-9` のまま）

これ以上の変種は足さない（ミニマル維持）。`defaultVariants` は変更しない。

### 6.8 【層A】アイコン基盤 `lucide-solid`（採用提案 — 批准事項）

土台設計フロント §7 では `lucide-solid` の採否が**「要検証」のまま留保**されている。これはアプリ全体に効く**ランタイム依存の追加**であり、07 をアイコン基盤のオーナーとして**採用を提案するが、最終採否はオーケストレータ/メンテナの批准事項**とする。07 は採用・不採用のどちらでも層A の他成果物（badge/button/format/ArticleView）が成立するように設計する。

- **採用する場合**: `cd frontend && pnpm add lucide-solid`。`pnpm typecheck`（= `tsc --noEmit`）が通ることを確認。import 形は `import { ChevronLeft } from "lucide-solid"`（**最新版が solid-js 1.9 / Vite 6 と整合するか実装時に公式で確認**）。
  - 使い方: インラインは 16px `class="h-4 w-4"`、単体アイコンボタンは 18px `class="h-[18px] w-[18px]"`、テキスト併記は `gap-2`。
  - アイコンのみのボタンは **`aria-label` 必須**＋必要に応じ Tooltip（Ark UI。`components/ui/tooltip.tsx` を初出時にラップ、part 名は ark-ui.com で確認。Tooltip 自体は #09 で初出予定）。
  - 想定アイコン例: 戻る `ChevronLeft`、外部リンク `ExternalLink`（現 `↗` の置換）、削除 `Trash2`、メニュー `MoreHorizontal`、更新 `RefreshCw`、未読 `Circle`。具体採用は各機能側。
- **不採用 / 互換問題が出た場合のフォールバック**: ごく少数のアイコンのみ自前 inline SVG コンポーネントを `components/ui/icons/`（数個だけ）に置く（依存ゼロ）。API は本書のアイコンボタン規約（`aria-label` 必須・サイズ規約）に合わせる。この場合 §6.10 の外部リンク icon 化や §6.11/§6.12 のアイコンは、テキスト/絵文字 or 自前 SVG にフォールバックする。

> 層A の他成果物（badge.tsx / button.tsx / format.ts / ArticleView 要約統一）は **`lucide-solid` に依存しない**ため、批准待ちでも先行実装できる。

### 6.9 【層A】`lib/format.ts`（小さな純関数。タイムゾーン固定）

一覧メタの**絶対短縮日付のみ**を担う。相対表記「N日前」「週Y件」は #03 feed-stats が担う。

**ファイル所有権の確定（二重作成防止）**: **`lib/format.ts` は 07 が新規作成し、`formatDate` のみを置く。#03 feed-stats はこのファイルを新規作成せず、`formatRelative` 等を同ファイルへ追記する。**

```ts
/**
 * ISO 8601 文字列を「YYYY/MM/DD」表記へ整形する純関数。
 *
 * タイムゾーンを Asia/Tokyo に**固定**するため、出力は実行環境の TZ に依存しない（決定的）。
 * これにより、UTC 午前0時のような境界値でも、どのマシン（米州 CI 等を含む）でテストしても
 * 同一の結果を返す。単一ユーザー（JST）前提に合致。
 * 不正な ISO は空文字を返し、フィード/LLM 由来の壊れた日時で UI を壊さない。
 */
const dateFormatter = new Intl.DateTimeFormat("ja-JP", {
  timeZone: "Asia/Tokyo",
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});

export function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return dateFormatter.format(d); // 例: "2026/06/26"
}
```

> 旧ドラフトの `new Date(iso).toLocaleDateString("ja-JP", ...)` は TZ を指定しておらず、`"2026-06-26T00:00:00Z"`（UTC 0時）が負オフセット環境で現地日付 2026-06-25 になり、期待値とずれてテストがフレークした。`Intl.DateTimeFormat` に `timeZone: "Asia/Tokyo"` を渡すことで決定的にする。

### 6.10 【層A】記事本文（ArticleView）の要約 prose 統一 — 単独で今すぐ出荷可能

**この変更は #10 のシェルに依存しない既存ファイルへの単独編集**であり、層A の中で唯一の実体マークアップ変更。現状の要約セクション（`ArticleView.tsx` 61-66行）は素の `<p class="text-sm whitespace-pre-wrap">` で、隣の翻訳セクション（prose）とタイポが揃っていない。要約を翻訳と同じ運用に統一する。

差し替え（現 61-66行）:

```tsx
<Show when={a().summary}>
  <section class="rounded-lg border border-border bg-muted/40 p-4">
    <h2 class="mb-1 text-sm font-semibold">要約</h2>
    {/* prose は基本タイポの統一が目的。要約はテキストノードのため Markdown は描画されない（§6.5）。 */}
    <div class="prose prose-sm dark:prose-invert max-w-none whitespace-pre-wrap">
      {a().summary}
    </div>
  </section>
</Show>
```

- 翻訳セクション（68-75行）は**既存どおり prose を維持**（変更不要）。
- 本文 `innerHTML` ブロック（77-80行）は**既存どおり**（変更不要）。
- 外部リンク（37-44行の `元記事を開く ↗`）は、`lucide-solid` 採用時に `ExternalLink`（`h-4 w-4`）＋ `gap-1` へ置換可能（**任意**。不採用時は現状の `↗` のまま）。

### 6.11 【層B 指針】一覧行の再設計（Card 羅列 → 罫線リスト） — 適用は #10/#08

> これは **07 の編集物ではない**。#08 が add-feed input を撤去し、#10 が `FeedList.tsx → ArticleList.tsx` 改名・シェル再構成を行う際に、この**目標マークアップ**へ寄せる。07 はこの規約を提供する。

```tsx
<ul class="divide-y divide-border">
  <For each={articles()}>
    {(a) => (
      <li>
        {/*
          @solidjs/router の <A> は現在ルートと一致する時 aria-current="page" を自動付与する。
          Tailwind 組込みの `aria-current:` バリアントは [aria-current="true"] にしかマッチせず
          "page" にはマッチしない。よって存在セレクタの任意値バリアント `aria-[current]:` を使う
          （§6.12 のサイドバー行と表記を統一する）。明示したい場合は <A activeClass="..."> でも可。
        */}
        <A
          href={`/articles/${a.id}`}
          class={cn(
            "-mx-2 block rounded-md px-2 py-3 transition-colors",
            "hover:bg-accent",
            "aria-[current]:bg-accent aria-[current]:text-accent-foreground",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          )}
        >
          <h3
            class={cn(
              "line-clamp-2 text-sm",
              a.is_read
                ? "font-normal text-muted-foreground"
                : "font-semibold text-foreground", // 未読 = 太字のみ（ドットなし。§6.4）
            )}
          >
            {a.title}
          </h3>
          <Show when={a.summary}>
            <p class="mt-0.5 line-clamp-1 text-sm text-muted-foreground">{a.summary}</p>
          </Show>
          <Show when={a.published_at}>
            <p class="mt-1 text-xs text-muted-foreground">{formatDate(a.published_at!)}</p>
          </Show>
        </A>
      </li>
    )}
  </For>
</ul>
```

ポイント:
- `Card`/`shadow` を外し**フラットな罫線リスト**に。情報密度が上がり視線移動が減る。
- ナビは `@solidjs/router` の `A`（クライアント遷移）を使う（現状の素 `<a>` は全リロード）。`A` の active 連動が #10 の選択状態と整合する。
- 未読は「**太字のみ**」、既読は muted（§6.4 の決定でドットは置かない）。選択中行（3ペイン拡張時に一覧と本文が同時表示される構造）には `aria-[current]:bg-accent` が当たる。
- `min-w-0` は親グリッド側（#10）で担保し、`line-clamp` で長いタイトルを安全に省略。

### 6.12 【層B 指針】ヘッダ / サイドバーの軽量化（#10 と整合） — 適用は #10

> これも **07 の編集物ではない**。#10 が `App.tsx` を二ペイン化し `Sidebar.tsx` を新設する際に従う密度・タイポ規約。

- アプリ名（現 `App.tsx` ヘッダの "RSS Reader"）は **#10 がサイドバー上部へ移す**。右ペインのヘッダは薄く: 戻る（`ChevronLeft` アイコンボタン）＋文脈アクションのみ。
  - 右ペインヘッダ推奨クラス: `sticky top-0 z-10 flex h-12 items-center gap-2 border-b border-border bg-background/80 px-4 backdrop-blur`。
- サイドバー項目: `flex h-8 items-center gap-2 rounded-md px-2 text-sm transition-colors hover:bg-accent`、選択中は `<A>` の active に対し `aria-[current]:bg-accent aria-[current]:text-accent-foreground`（§6.11 と表記統一）、未読数は右寄せ `Badge`（`ml-auto`）。
- 現 `App.tsx` の `<span>self-hosted</span>` のような装飾テキストは **#10 が撤去**（ノイズ削減）。

### 6.13 状態管理 / API / ルーティング
- グローバル状態の追加なし。本書は見た目のみ。テーマ（#04）・フィルタ（#11）・選択（URL, #10）の状態は各担当機能が所有。
- `lib/api.ts` への追加なし（既存フィールドのみ使用）。
- ルーティング変更なし（#10 が所有）。

---

## 7. API 契約

**追加・変更なし。** 表示はすべて既存エンドポイント（`GET /api/articles`, `GET /api/articles/{id}` 等）の現行 JSON で賄う。

---

## 8. 依存関係

### 8.1 機能間の関係
- **整合（ハード依存ではないが見えを担保）**:
  - `dark-theme`（#04）: `.dark` 配線・トグルを所有。本書は `dark:prose-invert` と意味トークンで**ダークでの視認性**を担保するが、トグル機構自体は持たない。`.dark` 自体は既に app.css に配線済みのため、層A の実装は #04 を待たずに動く。
- **協調（dependsOn には含めるが、層A は非依存）**:
  - `two-pane-layout`（#10）: App シェル・サイドバー・ルート構成・`ArticleList.tsx` 改名を所有。**層B（§6.11/§6.12）はこのシェルの上に乗る指針**であり、対象ファイルは #10 が作る。
  - `feed-add-placement`（#08）: 一覧から add-feed input を撤去するのは #08。本書は撤去後の一覧を罫線リスト化する指針を提供し、追加 UI を**再導入しない**。
  - `read-management`（#09）/ `unread-filter-toggle`（#11）: 未読数 `Badge`・未読/既読の見た目を本書が提供し、ロジックは #09/#11。
  - `feed-stats`（#03）: メタの相対表記は #03 が `lib/format.ts` に**追記**して所有。本書は同ファイルを**新規作成**し絶対短縮日付のみ置く（§6.9 の所有権確定）。
  - `feed-folders`（#02）: サイドバーのツリー行の密度・タイポを本書指針に従わせる。

### 8.2 オーケストレータ向け 実装順序の宣言（ハード依存）
- **層A（badge.tsx / button.tsx 加算 / lib/format.ts / lucide-solid 判断 / ArticleView 要約 prose 統一）は他機能に非依存。いつでも単独でブランチ→マージできる。** 後続の #01/#02/#09/#10 が UI を作る際の前提プリミティブになるため、**早期に層A を確定させると後続が楽**。
- **層B（§6.11 一覧罫線リスト化 / §6.12 ヘッダ・サイドバー軽量化）は #10 と #08 の後に、同一ブランチで重ねる。** 07 が先行して `ArticleList.tsx`/`Sidebar.tsx`/`App.tsx` を新設・改名すると #10/#08 と衝突する。よって層B は「#10/#08 が当該ファイルを実装/改名するときに本書 §6.11/§6.12 の規約を適用する」形で消化する。

> 結論（dependsOn）: 機能 07 全体としては **#10 two-pane-layout / #08 feed-add-placement / #04 dark-theme** に依存（順序づけ）するが、**層A 部分は依存なしで先行出荷できる**。

---

## 9. テスト計画（TDD）

フロントには現状テストランナー（vitest 等）が**ない**（`package.json` 確認済み: scripts は dev/build/preview/typecheck のみ）。土台設計の方針どおり「フロントは型 + 手動」を基本とし、純関数のみ任意で単体化する。

### 9.1 型ゲート（必須・自動）
- `just lint`（= `tsc --noEmit`、`package.json` の `typecheck` スクリプト）が通ること。`lucide-solid` import（採用時）・`Badge`・`button` 新変種・`formatDate` が型エラーを出さない。これが Green の最低ライン。

### 9.2 単体（純関数・テストランナー導入時のみ／タイムゾーン非依存）
`formatDate` は §6.9 で `timeZone: "Asia/Tokyo"` に固定したため、**テストはマシンの TZ 設定に依存しない**（前提 TZ = JST 固定）。次を Red→Green で確認:

- `formatDate("2026-06-26T00:00:00Z")` → `"2026/06/26"`。意図: UTC 0時（= JST 09:00 同日）が暦日 2026-06-26 になる基本ケース。**旧実装（TZ 未指定）では負オフセット環境で `"2026/06/25"` に落ちていた回帰を固定**。
- `formatDate("2026-06-25T15:00:00Z")` → `"2026/06/26"`。意図: UTC 15:00 = JST 翌日 00:00 の境界。**naive な UTC/ローカル変換だと `"2026/06/25"` になる識別テスト**で、TZ 固定を保証する。
- `formatDate("not-a-date")` → `""`（NaN ガード）。意図: フィード/LLM 由来の不正日時で UI を壊さない。

> ※ ランナー未導入のため当面は型 + 手動で代替。導入する場合は `vitest` を devDependency に追加（別タスク）。テストは TZ 環境変数に依存しないが、`Intl` がフル ICU データを持つ Node（v18+ の標準ビルドは可）で実行すること。

### 9.3 手動ビジュアル QA（Red→Green の代替チェックリスト）
各項目を**ライト/ダーク × デスクトップ二ペイン/モバイル単カラム**で確認:
- [ ] 未読行=**太字**、既読行=muted で**一目で区別**できる（一覧にはドットを置かない）。
- [ ] 長いタイトル（全角/半角）が `line-clamp-2` で崩れず省略される。
- [ ] 一覧が罫線リストでフラット（Card の影・囲いが消えている）。
- [ ] 選択中/ホバーが `bg-accent` で分かる。隣接行と混同しない（`aria-[current]:` が `<A>` の active に当たる）。
- [ ] キーボード Tab でリンク/ボタンに `ring-ring` のフォーカスリングが出る。
- [ ] **本文（innerHTML）が prose で見出し/段落/リスト/コードまで描画される。**
- [ ] **要約・翻訳は prose の基本タイポ（色・行間・字下げ）で本文と統一されている。**（テキストノードのため Markdown 記法はリテラル表示で正しい。リッチ描画は非スコープ）
- [ ] ダークで `dark:prose-invert` が効き、本文ブロックに白背景の抜けがない。
- [ ] サイドバー未読 `Badge` が `secondary` で過剰に目立たず、0 件は非表示（一覧の太字と役割が重複しない）。
- [ ] アイコンボタンに `aria-label` があり、スクリーンリーダ/ツールチップで意味が分かる。
- [ ] ダークで `muted-foreground` のメタ文字が読める（コントラスト）。

---

## 10. 実装手順（順序付きチェックリスト）

**層A（先行・他機能非依存）→ 層B（#10/#08 と整合）** の順。層A だけで 07 は単独マージ可能。

### 層A（今すぐ・単独で出荷可能）
1. **アイコン基盤の採否を確定**: オーケストレータ/メンテナに `lucide-solid` 採用可否を確認（§6.8）。
   - 採用: `cd frontend && pnpm add lucide-solid` → `pnpm typecheck` 緑、import 形を公式で確認。
   - 不採用/互換問題: フォールバック方針（自前 inline SVG を `components/ui/icons/` に最小限）を採る。**この判断は後続ステップ 2〜4 をブロックしない**（それらは lucide 非依存）。
2. **Badge 追加**: `frontend/src/components/ui/badge.tsx` を §6.6 のとおり作成。
3. **Button 変種追加**: `frontend/src/components/ui/button.tsx` の cva に `secondary` variant と `icon-sm` size を**加算**（既存変種・`defaultVariants` は触らない）。
4. **format ユーティリティ**: `frontend/src/lib/format.ts` を §6.9 のとおり**新規作成**し、`formatDate` のみ置く（#03 が後で同ファイルへ追記する前提）。
5. **記事本文の要約 prose 統一**: `frontend/src/routes/ArticleView.tsx` の要約セクション（61-66行）を §6.10 のとおり `prose ... whitespace-pre-wrap` の `<div>` に差し替え。**この単独変更は #10 非依存で今すぐ出荷可能**。`lucide-solid` 採用時は外部リンクを `ExternalLink` 化（任意）。
6. **`just lint`（tsc）緑を確認** → 層A はここでマージ可能。
7. **指針の確定**: 本書 §6.1〜6.5 を実装の基準として周知（このファイル自体が確定版）。

### 層B（#10/#08 の後に同一ブランチで重ねる — 07 は当該ファイルを新設しない）
8. **一覧の罫線リスト化**: #08（add-feed 撤去）・#10（`FeedList.tsx → ArticleList.tsx` 改名/シェル）が当該ファイルを用意した上で、§6.11 のマークアップへ寄せる。Card 依存を外し `divide-y` 化、ナビを `A`（router）へ、未読=太字のみ・既読 muted・`formatDate` メタ・`aria-[current]:` 選択を適用。
9. **ヘッダ/サイドバー軽量化**: #10 のシェル（`App.tsx`/`Sidebar.tsx`）上で §6.12 を適用（ブランドはサイドバー上部＝#10 が配置、右ペインヘッダを薄く、`self-hosted` 装飾撤去＝#10、サイドバー行 `h-8 px-2`、未読 `Badge` を `ml-auto`、選択は `aria-[current]:`）。
10. **色運用の通し見直し**: 全画面で生 hex・余計な影・任意角丸が無いか grep（例: `shadow-md`/`shadow-lg`、`rounded-[`、生の `#` カラー）。意味トークンのみに収束。
11. **QA**: §9.3 のチェックリストをライト/ダーク × デスクトップ/モバイルで実施。`just lint`（tsc）緑を確認。

---

## 11. リスク・未決事項・代替案

- **`lucide-solid` 採否（批准事項・要確認）**: アプリ全体に効くランタイム依存のため、07 は採用を提案するが最終判断はオーケストレータ/メンテナ（§6.8）。SolidJS 1.9 / Vite 6 との互換・import 形・tree-shake 効果を実装時に確認。重い/相性問題が出たら、ごく少数のアイコンは自前 inline SVG で代替（依存ゼロ。§6.8・実装手順 step 1 にフォールバックを明記済み）。**層A の他成果物は lucide 非依存で先行可能。**
- **要約・翻訳の prose は Markdown を描画しない**: これらはテキストノードのため、prose は基本タイポ統一に留まる（§6.5）。「要約に見出し/箇条書きを出す」要望が出たら Markdown→HTML 化 + `innerHTML` の別タスクが要る（XSS を考慮したサニタイズ込み）。本書では非スコープ。
- **未読インジケータの二重表現を排除済み**: 本書の決定として、一覧は「**太字のみ**」、サイドバーは「**Badge 数値**」と役割を分離した（§6.4）。太字だけでは未読アフォーダンスが弱いと感じる場合の代替: 既読タイトルの不透明度をさらに下げる（`text-muted-foreground` のまま `opacity` は使わずトークンで）か、未読のみ左罫線アクセント（`border-l-2 border-primary pl-2`）を足す——ただしドット復活は二重表現に戻るため避ける。
- **`aria-[current]:` バリアントの前提**: `@solidjs/router` の `<A>` が active 時に `aria-current="page"` を付与することに依存する。Tailwind の任意値バリアント `aria-[current]:` は属性の**存在**にマッチするため `"page"` でも当たる（組込み `aria-current:` は `"true"` 限定で当たらないので使わない）。実装時に `<A>` が実際に `aria-current="page"` を出すか（end/exact の扱い）を ark/router の挙動で確認。明示したい場合は `<A activeClass="bg-accent text-accent-foreground">` でも代替可。
- **罫線リスト vs Card の好み**: フラット罫線リストは密度を上げるが「区切りの弱さ」を感じる場合がある。代替として行間 `py-3.5`＋ホバー強調で調整可能。Card に戻すのは情報密度を下げるため非推奨。
- **ダークのコントラスト**: `muted-foreground`（dark: `oklch(0.708 0 0)`）のメタ文字が小サイズで読みにくい端末がありうる。気になる場合は **#04 と協議**のうえ `.dark` の `--muted-foreground` を 1 段明るく（`:root` と `.dark` 両モード定義の原則を守って）。本書単独では oklch を変更しない。
- **テストランナー不在**: 純関数 `formatDate` の自動テストは vitest 導入が前提（TZ 固定済みのため環境非依存で安定する）。導入は別タスク。当面は型 + 手動 QA で担保（土台設計の方針に一致）。
- **層B の着地順序（マージ衝突）**: 層B（§6.11/§6.12）は #10 のシェルと #08 の add-feed 撤去が前提。07 が先行して `ArticleList.tsx`/`Sidebar.tsx`/`App.tsx` を新設・改名すると衝突する。**層A を先に確定**し、層B は #10/#08 と同一ブランチ or 直後に重ねる（§8.2）。

---

### 付録 A: 主要ファイルへの着地点

| 操作 | パス | 所有 |
|------|------|------|
| 追加 | `frontend/src/components/ui/badge.tsx` | **07（層A）** |
| 追加 | `frontend/src/lib/format.ts`（`formatDate` のみ。#03 が追記） | **07（層A・新規作成）** |
| 変更 | `frontend/src/components/ui/button.tsx`（cva に `secondary` / `icon-sm` 加算） | **07（層A）** |
| 変更 | `frontend/src/routes/ArticleView.tsx`（要約を prose 化・外部リンク icon 化は任意） | **07（層A・#10 非依存で出荷可）** |
| 変更 | `frontend/package.json`（`lucide-solid` 依存。採用時） | **07（層A・要批准）** |
| 追加 (任意) | `frontend/src/components/ui/tooltip.tsx`（Ark UI ラップ・初出時。実体は #09 初出予定） | 07 指針 / #09 |
| 適用（07 は編集しない） | 記事一覧ルート（#10/#08 後の `routes/ArticleList.tsx`、現 `routes/FeedList.tsx`）の罫線リスト化 | **#10/#08**（§6.11 指針に従う） |
| 適用（07 は編集しない） | `frontend/src/App.tsx` ＋ `components/layout/Sidebar.tsx` の軽量化/密度 | **#10**（§6.12 指針に従う） |
| 不変 | `frontend/src/app.css`（oklch トークンは維持。新トークン追加は想定せず） | — |
