# RSS Reader (self-hosted)

Rust (Axum) + SolidJS によるセルフホスト型 RSS リーダー。フィードを購読し、**指定した記事だけをオンデマンドで** Claude API により要約・翻訳し、結果を DB にキャッシュします。家庭内ネットワークで Docker Compose 起動する想定です。

- **オンデマンド AI**: 全文一括処理はしません。ユーザーが記事ごとに要求したときだけ Claude を呼び、同一言語の再要求はキャッシュを返してトークンを消費しません。
- **トークン節約**: 要約・翻訳・ダイジェストの結果はすべて DB キャッシュ。
- **自己ホスト前提**: パスワードログイン必須（初回アクセス時に設定）、鍵・秘密は環境変数で外挿。

技術選定の背景・アーキテクチャ方針は [`CLAUDE.md`](./CLAUDE.md) を参照してください（貢献前に必読）。

## 主な機能

### 読む
- **3ペインのリーダーレイアウト** — 左サイドバー / 中央=記事一覧 / 右=本文。ペイン幅はドラッグ・矢印キーで調節。
- **全文検索** — PostgreSQL `pg_trgm` によるタイトル・本文の部分一致（日本語対応）。
- **既読管理** — 「開いた瞬間」ではなく**滞在（約3秒）かスクロール**で既読化。未読/既読の視覚表現・「すべて/未読のみ」フィルタ・一括既読・未読数バッジ。
- **本文の HTML 浄化** — DOMPurify で sanitize。安全なタイポグラフィ系 inline CSS のみ許可し、レイアウト破壊系は除去。
- **Markdown 描画 + シンタックスハイライト** — 要約/翻訳/ダイジェストの Markdown を HTML 化し、コードは highlight.js で色付け（テーマ連動）。
- **全文抽出** — フィードの抜粋しか無い記事を元サイトからオンデマンドで抽出（`readability` 相当、任意）。
- **テーマ切替** — light / dark / graphite / sepia。
- **レスポンシブ / PWA** — safe-area・`dvh`・タッチ最適化。インストール可能（iPhone / iPad 想定）。

### AI（Claude）
- **要約・翻訳** — 記事ごとにオンデマンド。既定は日本語。モデル・プロンプトは個別設定可能。
- **Ask Claude** — 記事本文を文脈にした対話 Q&A。
- **AI デイリーダイジェスト** — 期間の記事をまとめて要約。
- **タグ AI 提案** — 記事内容からタグ候補を提案。
- **関連度スコアリング / クラスタリング** — 興味プロファイルによるスコア付けと類似記事のまとめ。

### 読み上げ（リッスンモード）
- ブラウザ標準 Web Speech API による TTS。本文/要約/翻訳をソース切替で読み上げ。
- 自動で最良の日本語音声を選択、発音辞書で英字誤読を補正、読み上げ位置を保存（chunk 単位で再開）。

### 整理・運用
- **フィード管理** — 追加（サイト URL からのフィード自動発見）・改名・フォルダ割当・再取得・削除・投稿統計・健全性表示。
- **フォルダ分け / スマートビュー（保存ビュー）**。
- **ミュートルール / 自動ルール** — 条件に基づく既読化・タグ付け等の自動化。
- **Instapaper 連携 / 後で読む**。
- **スター・ハイライト / 注釈** — 本文選択から引用保存＋メモ。
- **OPML インポート / エクスポート**。
- **Web Push 通知** — 優先度「高」フィードの新着を PWA へ通知（VAPID）。
- **バックアップ / リストア** — エクスポート/インポート・任意の定期 `pg_dump`。
- **Google Reader 互換同期 API** — NetNewsWire / Reeder 等のクライアントから同期バックエンドとして利用可（opt-in。下記「同期」参照）。

## 技術スタック

| 層 | 採用技術 |
|----|----------|
| バックエンド | Rust / Axum 0.8 / Tokio / sqlx 0.8（PostgreSQL・実行時クエリ） |
| フィード取得 | reqwest + feed-rs |
| LLM | Anthropic Messages API を reqwest で直接呼び出し |
| フロントエンド | SolidJS 1.9 / Vite 6 / Tailwind CSS v4 / Ark UI（複雑部品）+ 自前 Tailwind |
| 描画補助 | marked（Markdown）/ DOMPurify（浄化）/ highlight.js（コード） |
| DB | PostgreSQL 17 |
| 配信 | nginx（静的配信 + `/api` リバースプロキシ） |

## アーキテクチャ

**Vertical Slice Architecture + 局所的 DDD**。機能は `backend/src/features/<slice>/` に縦割りで閉じ、1スライス = `domain.rs` / `repository.rs` / `service.rs` / `handler.rs` / `mod.rs`。新機能の追加は「新スライス1枚 + `features/mod.rs` に `.merge()` 1行」で、既存スライスは原則触りません。抽象化（trait）は「差し替える具体的理由がある境界」（現状 `shared/llm` のみ）に限定します。詳細は [`CLAUDE.md`](./CLAUDE.md)。

```
rss-reader/
├── backend/            # Rust / Axum / sqlx
│   ├── src/
│   │   ├── main.rs             # 起動・DI 組み立て・serve
│   │   ├── shared/             # config / db / error / state / scheduler / llm(唯一の抽象境界)
│   │   └── features/           # 縦割りスライス群（feeds, articles, search, digest, ...）
│   └── migrations/             # sqlx マイグレーション（追記のみ）
├── frontend/           # SolidJS / Vite / Tailwind v4 / Ark UI
│   └── src/
│       ├── components/         # ui/（共通部品）・article/ ほか
│       ├── lib/                # api / sanitize / markdown / highlight / tts / ...
│       └── routes/             # 画面
├── scripts/            # build.sh / push-images.sh / gen-vapid.sh
├── docs/               # design（設計書）/ review（レビュー成果物）
├── docker-compose.yml
├── justfile            # 開発タスクランナー
└── CLAUDE.md           # アーキテクチャ方針（必読）
```

## 必要なもの

- Docker / Docker Compose
- 開発時のみ: Rust 1.82+、Node 22（`corepack enable` で pnpm）、[`just`](https://github.com/casey/just)
- 補助ツール: `cargo install cargo-watch sqlx-cli`

## クイックスタート（フルスタック / Docker）

```bash
cp .env.example .env
# .env を編集（最低限 POSTGRES_PASSWORD を変更。Claude 機能を使うなら ANTHROPIC_API_KEY も）

just up            # = docker compose up -d --build
```

- Web UI: http://localhost:8081 （`.env` の `WEB_PORT` で変更可）
- API: フロントの nginx 経由で `/api/*` がバックエンドにプロキシされます

停止: `just down` / ログ: `just logs`

## ローカル開発（ホットリロード）

3つのターミナルで:

```bash
just dev-db        # PostgreSQL だけ Docker で起動
just back          # バックエンド（cargo watch、:8080）
just front         # フロント（Vite、:5173 → /api を :8080 にプロキシ）
```

`.env` の `DATABASE_URL` はローカル実行時 `localhost:5432` を指す設定にしてください（`.env.example` 既定どおり）。

品質チェック:

```bash
just lint          # clippy -D warnings / tsc 型チェック
just test          # cargo test / vitest
just fmt           # cargo fmt / prettier
```

## 設定（`.env`）

| 変数 | 既定 | 説明 |
|------|------|------|
| `DATABASE_URL` | `postgres://rss:rss@localhost:5432/rssreader` | 接続先。Docker 内は compose が `db:5432` を注入 |
| `BIND_ADDR` | `0.0.0.0:8080` | バックエンドの bind |
| `WEB_PORT` | `8081` | 公開する Web UI ポート（compose） |
| `ANTHROPIC_API_KEY` | （空） | 未設定なら要約/翻訳/Ask/ダイジェスト等は `503 not enabled` |
| `ANTHROPIC_MODEL` | `claude-sonnet-4-6` | 既定モデル |
| `COOKIE_SECURE` | `false` | セッション Cookie に `Secure` 属性を付ける。HTTPS 終端がある構成でのみ `true`（http だけの LAN で `true` にすると Cookie が保存されずログイン不能） |
| `FEED_REFRESH_INTERVAL_SECS` | `900` | フィード定期取得の間隔 |
| `EXTRACT_ON_CRAWL` | `false` | クロール時に全文抽出するか（true は相手サイトへ GET が飛ぶ） |
| `EXTRACT_MAX_BYTES` | `3000000` | 抽出時に取得する最大バイト数 |
| `EXTRACT_MIN_CHARS` | `200` | 「意味のある本文」とみなす最小文字数 |
| `BACKUP_TOKEN` | （空） | `/api/backup/*` を有効化するトークン（未設定なら 503） |
| `BACKUP_DIR` / `BACKUP_PGDUMP_INTERVAL_SECS` | （空） | 両方設定時のみ定期 `pg_dump`（`pg_dump` がコンテナに必要） |
| `VAPID_PUBLIC_KEY` / `VAPID_PRIVATE_KEY` | （空） | Web Push 通知。両方セットで有効（片方欠けで `/api/push/*` は 503）。生成は `scripts/gen-vapid.sh` |
| `SYNC_API_ENABLED` | `false` | Google Reader 互換同期 API（`/accounts/ClientLogin`・`/reader/api/0/*`）。無認証到達面を持つため明示 opt-in |

> `.env` や API キー・VAPID/BACKUP トークンは**コミットしないこと**。

## Claude API（要約・翻訳）

`.env` に `ANTHROPIC_API_KEY` を設定すると、記事画面の各 AI 機能が有効になります。未設定でも RSS リーダーとして動作します（該当機能のみ `503 not enabled`）。

- 既定モデル: `claude-sonnet-4-6`（`ANTHROPIC_MODEL` で変更可、記事ごと/機能ごとの個別設定も可）
- 処理結果は DB にキャッシュされ、同一記事・同一言語の再要求ではトークンを消費しません（強制再生成も可能）

## 認証（パスワードログイン）

`/api` は**パスワードログイン必須**です（health と `/api/auth/*` のみ公開）。

- **初回セットアップ**: 初回アクセス時に画面からパスワード（8〜128文字）を設定します。設定するまで全 API は 401 を返します。
- **セッション**: サーバー側セッション + `HttpOnly; SameSite=Strict` Cookie（30日スライディング）。パスワードは Argon2id、セッショントークンは SHA-256 ハッシュのみ DB 保存します。
- **保護機構**: ログイン失敗の指数バックオフ（5連続失敗で30秒〜最大15分）、state-changing リクエストの Origin 検証（CSRF）、パスワード変更時の他セッション全失効。
- **デバイス管理**: 設定画面からログイン中デバイスの一覧・個別失効・パスワード変更ができます。
- **パスワードを忘れたら**（サーバーへの物理アクセス＝所有者とみなします）:

  ```bash
  docker compose exec db psql -U rss -d rssreader -c "DELETE FROM auth_credential;"
  ```

  次回アクセスでセットアップ画面が再表示されます（既存セッションも失効させるなら `DELETE FROM auth_sessions;` も実行）。

## 同期（Google Reader 互換 API）

`.env` に `SYNC_API_ENABLED=true` を設定すると、GReader 互換 API が有効になり、サードパーティの RSS クライアント（動作確認対象: **NetNewsWire**。副対象: Reeder）から購読・未読/既読・スターを同期できます。

**クライアント設定**（NetNewsWire の例）:

- アカウント種別: **FreshRSS**
- URL: `http://<サーバーの LAN IP>:8081`
- ユーザー名: 任意（設定画面の「同期クライアント」一覧に識別ラベルとして表示）
- パスワード: この Web UI のログインパスワード

仕組みと注意:

- 認証はログインパスワードによる `ClientLogin` → 無期限トークン（DB には SHA-256 ハッシュのみ保存）。トークンの一覧・失効は Web UI の設定 →「同期クライアント」から。
- `ClientLogin` は Web ログインと**レート制限を共有**します（総当たり対策。攻撃者が連打すると正規の Web ログインも一時ロックされ得る点に注意）。
- GReader 側の既読・スターは Web UI と同じデータを読み書きします（別コピーなし）。
- **インターネット公開時は Cloudflare Access の前置を推奨**。GReader クライアントは対話 SSO を通れないため、`/accounts/ClientLogin`・`/reader/` に掛ける Access ポリシーは Service Token / mTLS / IP 許可等の**非対話手段**で構成する必要があります（LAN 内同期のみなら不要）。

## セキュリティ / 公開時の注意

このアプリは**家庭内 LAN での利用を前提**にしています。インターネットに公開する前に少なくとも:

- リバースプロキシで TLS 終端し、`COOKIE_SECURE=true` を設定する。
- `POSTGRES_PASSWORD` を既定から変更する。
- CORS は既定で同一オリジンのみ。別オリジンを許可する場合だけ `CORS_ALLOWED_ORIGINS` を最小限に設定する。
- `SYNC_API_ENABLED=true` で公開する場合は Cloudflare Access 等を前置する（上記「同期」の注意参照）。

（既知の未対応項目・優先順位は `docs/review/` および開発メモを参照。）

## イメージのビルドと配布（NAS など）

```bash
# .env に REGISTRY（例: ghcr.io/your-name または nas.local:5000）と TAG を設定
just docker-build  # backend / frontend イメージをビルド
just docker-push   # レジストリへ push
```

NAS 側では push したイメージを参照する compose を用意するか、`docker-compose.yml` をそのままビルドさせます。

## API 概要

すべて `/api` 以下。health と `/api/auth/status`・`/api/auth/setup`・`/api/auth/login` を除き、有効なセッション Cookie が必要です。代表的なエンドポイント:

| 分類 | 代表エンドポイント |
|------|--------------------|
| ヘルス | `GET /api/health` `GET /api/health/db` |
| 認証 | `GET /api/auth/status` `POST /api/auth/setup` `POST /api/auth/login` `POST /api/auth/logout` `PUT /api/auth/password` `GET/DELETE /api/auth/sessions` |
| フィード | `GET/POST /api/feeds` `DELETE /api/feeds/{id}` `POST /api/feeds/{id}/refresh` `POST /api/feeds/discover` |
| フィード運用 | `GET /api/feeds/overview` `GET /api/feeds/health` |
| 記事 | `GET /api/articles` `GET /api/articles/{id}` `POST /api/articles/{id}/read` `POST /api/articles/read-all` |
| AI（記事） | `POST /api/articles/{id}/summarize` `POST /api/articles/{id}/translate` `POST /api/articles/{id}/extract` `POST /api/articles/{id}/ask` |
| 検索 | `GET /api/search?q=` |
| 注釈 | `GET /api/stars` `GET /api/articles/{id}/notes` |
| タグ | `/api/tags`（一覧・付与・AI 提案） |
| フォルダ / ビュー | `GET /api/folders` `GET /api/saved-views/{id}/articles` |
| ダイジェスト | `GET /api/digest` `GET /api/digest/latest` `POST /api/digest/refresh` |
| 関連度 / クラスタ | `/api/relevance/*` `/api/clusters/*` |
| ルール | `/api/rules*` `/api/mute-rules*` |
| 通知（Web Push） | `GET /api/push/public-key` `POST /api/push/subscribe` `POST /api/push/unsubscribe` `POST /api/push/test` |
| OPML | `GET /api/opml/export` `POST /api/opml/import` |
| バックアップ | `GET /api/backup/export` `POST /api/backup/import` `GET /api/backup/runs` |
| Instapaper | `GET /api/instapaper/status`（+ 後で読む） |
| 統計 | `GET /api/stats` |
| 同期（GReader 互換・要 `SYNC_API_ENABLED`） | `POST /accounts/ClientLogin` `GET/POST /reader/api/0/*`（独自トークン認証・`/api` 外） `GET/DELETE /api/sync/tokens*`（トークン管理） |

（機能はそれぞれ `backend/src/features/<slice>/` に対応します。正確な入出力は各スライスの `handler.rs` を参照。）

## 開発コマンド（justfile）

```
just dev-db        # DB だけ起動
just back          # バックエンド（cargo watch）
just front         # フロント（Vite）
just build         # 両方リリースビルド
just lint          # clippy -D warnings / tsc typecheck
just test          # cargo test / vitest
just migrate       # sqlx migrate run
just migrate-add <name>  # 新規マイグレーション追加
just up / down / logs    # フルスタック compose 起動 / 停止 / ログ
just docker-build  # イメージビルド（scripts/build.sh）
just docker-push   # レジストリへ push
```

## データベース / マイグレーション

- `backend/migrations/` の sqlx マイグレーション（`0001` 〜）。起動時に自動適用されます。
- 新カラム・テーブルは**必ず新ファイルを追記**（既存マイグレーションは編集しない）。
- sqlx は**実行時クエリ**（`query` / `query_as`）を使用。`query!` 系コンパイル時マクロは使いません（ビルドに DB 接続が不要）。

## ライセンス

任意（未設定）。
