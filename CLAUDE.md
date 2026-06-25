# CLAUDE.md

このファイルは Claude Code がこのリポジトリで作業する際の指針です。**作業前に必ず読み、ここに書かれた方針から逸脱する場合は理由を述べること。**

## プロジェクト概要

セルフホスト型 RSS リーダー。フィードを購読し、指定した記事のみを **オンデマンドで** Claude API により要約・翻訳し、結果を DB にキャッシュする。家庭内ネットワークで Docker Compose 起動する想定。

## 技術スタック

| 層 | 採用技術 |
|----|----------|
| バックエンド | Rust / Axum 0.8 / Tokio / sqlx 0.8 (PostgreSQL) |
| フィード取得 | reqwest + feed-rs |
| LLM | Anthropic Messages API を reqwest で直接呼び出し（公式 Rust SDK は存在しない） |
| フロントエンド | SolidJS 1.9 / Vite 6 / Tailwind CSS v4 / 自前Tailwind + Ark UI（複雑部品のみ） |
| DB | PostgreSQL 17 |
| 配信 | nginx（静的配信 + `/api` リバースプロキシ） |

## アーキテクチャ方針（重要）

**Vertical Slice Architecture + 局所的 DDD**。レイヤーを水平に厚く積む古典的 Clean Architecture は採らない。

- 機能は `backend/src/features/<slice>/` に縦割りで閉じる。1スライス = `domain.rs` / `repository.rs` / `service.rs` / `handler.rs` / `mod.rs`。
- **新機能の追加 = 新スライス1枚 + `features/mod.rs` に `.merge()` 1行。既存スライスは原則触らない。**
- `domain.rs` では newtype で値オブジェクトを作り、不正な状態をコンパイル時／構築時に弾く（例: `FeedUrl::parse`, `FeedId`, `ArticleId`）。これが堅牢性の中核。
- **抽象化（trait）は「差し替える具体的理由が見えている境界」だけに留める。** 現状その対象は `shared/llm`（LLM プロバイダの差し替え・テストのモック）のみ。リポジトリやフィードパーサは trait 化しない。"将来替えるかも" で trait を増やさない。
- エラーは `shared/error.rs` の `AppError`（thiserror）に集約。ハンドラは `AppResult<T>` を返し `?` で伝播。`IntoResponse` が HTTP ステータスへ変換する。

### ディレクトリ構成

```
backend/src/
  main.rs                 # 起動・DIの組み立て・serve
  shared/
    config.rs             # 環境変数 → AppConfig
    db.rs                 # プール生成 + migrate
    error.rs              # AppError / AppResult / IntoResponse
    state.rs              # AppState（db, config, http client）
    scheduler.rs          # tokio::interval によるフィード定期取得（apalis に差し替え可能）
    llm/                  # ★唯一の抽象境界
      mod.rs              # LlmClient trait
      anthropic.rs        # Claude 実装（Messages API）
  features/
    mod.rs                # router() で各スライスを merge
    health/               # /api/health, /api/health/db
    feeds/                # フィード CRUD + クロール
    articles/             # 記事一覧/取得/既読 + 要約/翻訳（LLMキャッシュ）
```

## LLM 要約・翻訳の設計

- **オンデマンド**: ユーザーが記事ごとに要求したときだけ Claude を呼ぶ。全文一括処理はしない。
- **キャッシュ**: 結果は `articles.summary` / `translation`（＋ `_lang`）に保存。同一言語の要求はキャッシュを返し、トークンを消費しない（`articles/service.rs` 参照）。
- API キー未設定時は `AppError::NotEnabled` を返す（機能は任意有効）。
- 既定モデルは `claude-sonnet-4-6`、要約は日本語（`ja`）既定。

## 開発コマンド（justfile）

```
just dev-db        # DB だけ起動
just back          # バックエンド（cargo watch）
just front         # フロント（Vite, /api を :8080 にプロキシ）
just build         # 両方リリースビルド
just lint          # clippy -D warnings / tsc typecheck
just migrate       # sqlx migrate run
just up / down     # フルスタックを compose 起動/停止
just docker-build  # イメージビルド（scripts/build.sh）
just docker-push   # レジストリへ push（scripts/push-images.sh）
```

前提ツール: `cargo install cargo-watch sqlx-cli`、`corepack enable`（pnpm）、`just`。

## コーディング規約

- **Rust**: ドメイン層は `thiserror` で型付きエラー、アプリ境界は `anyhow`。sqlx は **実行時クエリ**（`query` / `query_as`）を使う。`query!` 系コンパイル時マクロは使わない（ビルドに DB 接続が必要になるため）。新カラムを足したら `migrations/` に新ファイルを追加（既存マイグレーションは編集しない）。
- **フロント**: UI は `components/ui/` のコンポーネントを使う。単純な部品は自前 Tailwind、複雑な部品は Ark UI をラップして追加する（上記「UIコンポーネントの方針」参照）。記事本文・要約・翻訳の表示には Tailwind Typography の `prose` クラスを使う（可読性が最優先）。API 呼び出しは `lib/api.ts` に集約。
- フォーマット: `cargo fmt` / prettier。lint を通してからコミット。

### UIコンポーネントの方針（重要 — 2026-06 時点の判断）

**shadcn-solid とその基盤 Kobalte/corvu は更新が停滞している**（shadcn-solid は約15ヶ月、Kobalte は約11ヶ月、corvu は約17ヶ月、新規公開なし）。そのため shadcn-solid CLI には依存しない。代わりに次の二段構えを採る。

1. **単純な見た目部品（button, card, layout, input など）= 自前の Tailwind**。`components/ui/` に同梱の `button.tsx` / `card.tsx` がその例。これらは Kobalte 等に依存せず、`cva` + Tailwind のみ。陳腐化しない。
2. **アクセシビリティ実装が要る複雑な部品（dialog, dropdown, select, tooltip, combobox など）= Ark UI（`@ark-ui/solid`）**。Ark UI は zag.js ベースで活発に更新されており（2026-06 時点で数週間以内に公開）、ヘッドレスなので Tailwind でそのまま装飾できる。同梱の `dialog.tsx` がラップ例。

新しい複雑部品を足すときは、Ark UI の該当コンポーネントを薄くラップし、`app.css` のデザイントークン（`bg-background`, `border-border`, `text-muted-foreground` 等）で装飾して `components/ui/` に置く。Ark UI の API はバージョンで変わりうるので、実装時に公式ドキュメント（ark-ui.com）で最新の構造を確認すること。

**視覚デザインのトークンは shadcn 由来の oklch 変数（`app.css`）をそのまま使い続ける**。見た目の一貫性は保たれる。停滞しているのは「コンポーネントのコード供給元」であって、デザイントークンではない。

## やってはいけないこと

- 既存スライスを横断する密結合な共通レイヤーを作らない（Vertical Slice の利点を消す）。
- 差し替え予定のない境界に trait / dyn を足さない（Rust では抽象化コストが高い）。
- `query!` コンパイル時マクロを導入しない。
- マイグレーション済みファイルを書き換えない（必ず追記）。
- API キーや `.env` をコミットしない。

## ロードマップ（未実装の足場あり）

- フィード定期取得を `apalis` に移行（リトライ・バックオフ・per-feed スケジュール）。`shared/scheduler.rs` を差し替える。
- 記事本文の抽出強化（現状はフィードの content/summary をそのまま使用）。
- 全文検索（PostgreSQL の `tsvector`）。
- 未読/既読フィルタ UI、フィード単位の絞り込み。
