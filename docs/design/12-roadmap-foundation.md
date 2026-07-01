# 12. 第2弾 横断土台（ロードマップ機能の共通方針）

> 第1弾（00–11, 2026-06-26 起票・実装済）の土台 [00-foundation-backend.md](./00-foundation-backend.md) / [00-foundation-frontend.md](./00-foundation-frontend.md) を**前提として継承**する。本書はそれに加えて、第2弾（13–33, 2026-06-30 起票のロードマップ）の機能群に共通する横断方針だけを扱う。各機能設計（13 以降）はまず 00 系と本書を読んでから着手すること。マイグレーション番号と実装順は [34-integration-notes.md](./34-integration-notes.md) に集約する。

## 1. スライス境界一覧（第2弾）

新機能 = 新スライス1枚（`domain/repository/service/handler/mod`）＋ `backend/src/features/mod.rs` に `.merge()` 1行。既存スライス拡張は「同一アグリゲートへの書き込み」に限り正当化（00-foundation 準拠）。

| # | 機能 | 形態 | 新スライス / 拡張先 |
|---|------|------|--------------------|
| 13 | 記事全文抽出 | 既存拡張 | `articles`（`full_content`/`extracted_at` 追加・`POST /api/articles/{id}/extract`） |
| 14 | 認証 | 横断＋小スライス | `shared` ミドルウェア＋ `auth` スライス（`/api/auth/*`） |
| 15 | バックアップ/復元 | 新スライス | `backup` |
| 16 | Read-on-Save | 既存拡張 | `instapaper`/`read_later`（設定＋送付時フック） |
| 17 | OPML 入出力 | 新スライス | `opml`（feeds/folders repo 再利用・テーブル無し） |
| 18 | キーボード操作 | フロントのみ | （新スライス無し。`lib/keyboard`） |
| 19 | ミュート | 新スライス | `mute_rules`（→ 将来 28 に統合） |
| 20 | フィード自動検出 | 既存拡張 | `feeds`（`POST /api/feeds/discover`） |
| 21 | フィード健全性 | 既存拡張 | `feeds`（取得結果列追加・`feed_overview` で露出） |
| 22 | Ask Claude | 新スライス＋ llm 拡張 | `article_query`／`article_notes`、`shared/llm` に `chat` 追加 |
| 23 | デイリーダイジェスト | 新スライス | `digest`（scheduler 連携） |
| 24 | タグ＋AI自動タグ | 新スライス | `tags`（★多くの前提） |
| 25 | AI 関連度スコアリング | 新スライス | `relevance`（24 依存） |
| 26 | 意味的クラスタリング | 新スライス | `clustering`（scheduler 連携） |
| 27 | スマートビュー | 新スライス | `saved_views`（search/articles 再利用） |
| 28 | ルールエンジン | 新スライス | `automation_rules`（19/24 を内包） |
| 29–33 | 同期API/ニュースレター/プッシュ/スター/TTS | スタブ | 実装前に各設計を詳細化 |

## 2. マイグレーション規約（⚠️ 採番は実装時確定）

- **執筆時点の最新は `0005_search.sql`。** 第2弾の各設計書は暫定的に `0006_*.sql` と書いているが、**実際の番号は着手・マージ時に `backend/migrations/` の最大値+1 を採る**（追記のみ・既存不編集が鉄則）。
- 複数機能を並行で進めると 0006 が衝突する。**先にマージした側が番号を取得し、他方は繰り上げる**。推奨採番順は [34-integration-notes.md](./34-integration-notes.md) の「マイグレーション番号レジスタ」に従う。
- **apalis 移行（ロードマップ別タスク）も番号を取る。** 第1弾 README と同様、apalis ジョブテーブルと衝突しうるため着手直前に最新番号を必ず確認する。
- `gen_random_uuid()`（pgcrypto 由来）を既定値に使う前例（19/24）に倣う。未導入環境では migration 先頭に `CREATE EXTENSION IF NOT EXISTS pgcrypto;`、またはアプリ側 `Uuid::new_v4()` を bind する。
- 既存アグリゲートへの列追加（13/19/21/31/33 等）は `ALTER TABLE ... ADD COLUMN`（NULL 許容 or DEFAULT 付き）で後方互換に。過去行は NULL/既定のまま（再クロール等で順次充填）。

## 3. 認証ミドルウェア（機能14・全エンドポイント共通）

機能14 がマージされた後、**第2弾の全新規エンドポイントは認証ガード配下に入る**（設計時に前提とすること）。

- 方式: 単一トークン MVP（`AUTH_TOKEN` env）。`config.rs` に `auth_token: Option<String>` を追加。未設定なら**ガード無効＝従来どおり全開**（家庭内 localhost 運用との後方互換）。設定時のみ保護。
- 実装: `shared` に `require_auth` ミドルウェア層を1つ。`features/mod.rs` の `router()` 全体に `.layer(require_auth)` を被せ、**除外**は `/api/health*` と `/api/auth/*`。同期API（29）はトークン query/header の別経路を持つため個別に通す。
- フロント: トークンを保持し、`lib/api.ts` の共通 `http()` に `Authorization`（or `x-api-token`）ヘッダを付与。401 はログイン画面へ。
- 各設計書のエンドポイントは「ガード前提・除外不要」で書いてよい（個別に認証コードを書かない＝横断ミドルウェアに委ねる）。

## 4. AI 機能の共通パターン（13/22/23/24/25/26 ＋将来）

抽象境界は `shared/llm` のみ（trait を増やさない）。AI 機能は全てこの型に従う。

1. **クライアント取得**: `AppState.config.anthropic_api_key` を `ok_or(AppError::NotEnabled(...))` で取り出し、`AnthropicClient::new(state.http.clone(), key, state.config.anthropic_model.clone())`。モデルは必ず config（既定 `claude-sonnet-4-6`）。ハードコード禁止。
2. **未設定時**: `AppError::NotEnabled` を返す（503）。フロントは「AI 機能は無効（APIキー未設定）」を表示。要約/翻訳と同型。
3. **キャッシュ列命名規約**: 生成結果は DB にキャッシュし、再要求でトークンを消費しない。`articles` 既存の `summary`/`summary_lang`/`translation`/`translation_lang`/`processed_at` に倣い、`<feature>`／`<feature>_lang`／`<feature>_at`（例: クラスタ要約 `cluster.summary`、タグ提案 `article_tag_suggestions`、関連度 `article_relevance_scores.score`/`scored_at`）。同一入力・同一パラメータならキャッシュ返却。
4. **入力は全文優先**: 機能13（全文抽出）がある場合、要約/翻訳/Ask/ダイジェスト/クラスタリングの入力は `articles.full_content`（無ければ `content`）を使う。**13 は全 AI 機能の品質上限を決める土台**なので最優先で着手する。
5. **`shared/llm` 拡張**: Ask（22）は `LlmClient` に `chat`（messages 配列）メソッドを追加する（唯一の trait 拡張）。それ以外は既存 `summarize`/`translate` か、サービス層で `AnthropicClient` を直接呼ぶ（プロンプトは各サービスに閉じる）。
6. **バックグラウンド生成**（23 ダイジェスト・26 クラスタリング）は `shared/scheduler.rs` の tokio ループに処理を足す（apalis 移行時に一緒に移送）。失敗はログのみでループを止めない（既存 `refresh_all_feeds` と同型）。

## 5. 機能間の前提依存（最重要パス）

- **`tags`（24）が中核前提**: `digest`(23) のトピック分類、`relevance`(25) の興味プロファイル、`saved_views`(27) のタグ絞り込み、`automation_rules`(28) のタグ条件/アクション、いずれもタグ語彙を使う。**24 を AI 群の中で先に通す**。
- **`automation_rules`（28）が統合先**: `mute_rules`(19) と タグ自動付与(24) は 28 の部分集合。19/24 を単機能で先行出荷し、28 で条件/アクションへ移送して専用テーブル/UI を将来廃止する（28 設計書の「共存期」注記参照）。
- **`full_content`（13）が AI 入力の前提**（§4-4）。
- 詳細な順序・依存グラフは [34-integration-notes.md](./34-integration-notes.md)。

## 6. テスト方針（TDD・第1弾踏襲）

- **domain は単体テスト先行**（Red→理解→修正）。純粋関数（値オブジェクトの `parse`、ルール評価、クエリ組み立て、スコア正規化、OPML パース等）は `#[cfg(test)]` で先に書く。フロントの純粋ロジックは `lib/*.test.ts`（vitest、`selection.ts`/`search.ts`/`read-trigger.ts` 前例）。
- **リポジトリ往復**は `#[ignore]` 付き統合テスト（`DATABASE_URL` 指定で `cargo test -- --ignored`）。
- **HTTP スモーク**は `scripts/test/api-<feature>.sh` を追加し `scripts/test/run-all.sh` に登録（`api-search.sh` 前例）。
- AI 機能は `ANTHROPIC_API_KEY` 有/無の両系列（有=生成→2回目キャッシュ命中、無=`NotEnabled`）を確認。

## 7. 受け入れ前チェック（全機能共通の Done 条件）

- backend: `cargo fmt`（既存ファイルの再整形に注意）→ `cargo check` → `cargo test`（＋必要に応じ `--ignored`）。
- frontend: `frontend/node_modules/.bin/tsc --noEmit` → `vitest run` → `vite build`（corepack 版ガード回避のため `node_modules/.bin` 直叩き）。
- e2e: `just dev-db` + `back` + `front` で実挙動。新エンドポイントは `curl`/スモークスクリプトで疎通。
- マイグレーション: 着手直前に `backend/migrations/` の最新番号を確認し採番（§2）。
- コミット/プッシュは**ユーザーの明示指示があるまで行わない**（プロジェクト運用ルール）。

---

*生成: 2026-06-30 / 第2弾 設計ワークフロー（グラウンド → 機能設計21 → 横断土台・整合レビュー）。本書は session limit で未生成だった分を補completeしたもの。*
