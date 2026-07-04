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

## 設計作業の effort（重要）

**「設計して」「設計書を作って」「実装方針を立てて」等、設計を行う指示を受けたときは、Plan モードで遂行する。** まだ Plan モードでなければ EnterPlanMode で入り、探索・設計の結果を計画として提示し、**ユーザーの承認を得てから**実装に着手する（承認前にコードを変更しない）。

あわせて、effort レベルは `ultracode`（マルチエージェント・オーケストレーション）として遂行する。 具体的には Workflow ツールで、①複数観点の並行探索（既存コード・再利用可能な実装・パターンの把握）→ ②複数案の設計と批評（judge/verify）→ ③統合、というフェーズ構成を組み、単一エージェントの一発設計で済ませない。トークンコストより設計の網羅性・正確性を優先する。

- 対象: 新機能の設計、アーキテクチャ判断、リファクタ方針、複数ファイルにまたがる変更の計画など「設計」に該当するもの。
- 対象外: 単純な実装・タイプ修正・既存方針の機械的な適用など、設計判断を伴わない作業（通常 effort でよい）。
- 実装フェーズは従来どおり（設計が固まった後の実装は必ずしも ultracode を要しない）。設計と実装が地続きの依頼では、少なくとも設計フェーズを ultracode で行う。

## やってはいけないこと

- 既存スライスを横断する密結合な共通レイヤーを作らない（Vertical Slice の利点を消す）。
- 差し替え予定のない境界に trait / dyn を足さない（Rust では抽象化コストが高い）。
- `query!` コンパイル時マクロを導入しない。
- マイグレーション済みファイルを書き換えない（必ず追記）。
- API キーや `.env` をコミットしない。

## ロードマップ

### 実装済み（2026-06-29〜30 に `main` へマージ）

- **全文検索**: PostgreSQL `pg_trgm` による部分一致（title/content）。`tsvector` は日本語が分割されず不適のため不採用。`/api/search` + 検索 UI。migration 0005。
- **3ペインのリーダーレイアウト**: 左サイドバー / 中央=記事一覧 / 右=本文。scope=パス・選択記事=`?article` クエリ。ペイン幅は drag / 矢印キーで調節（`lib/resizable` + `ResizeHandle`）。
- **既読管理**: 「開いた瞬間」ではなく**滞在（約3秒）かスクロール**で既読化（`lib/read-trigger`）。未読/既読の視覚表現・「すべて / 未読のみ」フィルタ・一括既読・未読数バッジ。
- **記事本文の HTML 浄化**: DOMPurify で sanitize（`lib/sanitize`）してから描画。
- **レスポンシブ / PWA**: 二段ブレークポイント（常設サイドバー=`lg` / 一覧・本文分割=`md`）、safe-area（`viewport-fit=cover` + `@utility *-safe`）、`dvh` 高さ、タッチ44px（`pointer-coarse`）、installable PWA（manifest・アイコン、Service Worker なし）。iPhone 17 Pro Max / iPad Pro 11" 想定。
- **フィード管理画面**（`/manage`: 一覧・改名・フォルダ割当・再取得・削除・投稿統計）、**フォルダ分け**、**テーマ切替**（light / dark / graphite / sepia）、**Instapaper 連携 / 後で読む**。

### 未実装・継続

- フィード定期取得を `apalis` に移行（リトライ・バックオフ・per-feed スケジュール）。`shared/scheduler.rs` を差し替える。**← 着手中（方式B: cron + Postgres ジョブキュー）。migration は 0006 以降（0005 は検索が消費済み）**。
- 記事本文の抽出強化（readability 抽出。現状はフィードの content/summary を sanitize して表示するのみ）。
- デザインのミニマル化（視認性向上の継続的 UI 改善。主観的・随時）。

> 旧「ユーザー要望の機能バックログ（2026-06-26 起票）」の大半 — フィード管理・フォルダ分け・投稿状況表示・テーマ切替・Instapaper・後で読む・既読管理・2ペイン化（→3ペインで対応）・すべて/未読切替・フィード追加 UI の配置見直し（サイドバー下部に集約） — は上記「実装済み」で対応済み。
