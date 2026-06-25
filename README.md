# RSS Reader (self-hosted)

Rust (Axum) + SolidJS によるセルフホスト型 RSS リーダー。フィードを購読し、指定した記事を **オンデマンドで** Claude API により要約・翻訳して DB にキャッシュします。

## 構成

```
rss-reader/
├── backend/        # Rust / Axum / sqlx (Vertical Slice + 局所DDD)
├── frontend/       # SolidJS / Vite / Tailwind v4 / Ark UI（複雑部品）
├── scripts/        # build.sh / push-images.sh
├── docker-compose.yml
├── justfile        # 開発タスクランナー
└── CLAUDE.md       # アーキテクチャ方針（必読）
```

技術選定の背景とアーキテクチャ方針は [`CLAUDE.md`](./CLAUDE.md) を参照してください。

## 必要なもの

- Docker / Docker Compose
- 開発時のみ: Rust 1.82+、Node 22（`corepack enable` で pnpm）、[`just`](https://github.com/casey/just)
- 補助ツール: `cargo install cargo-watch sqlx-cli`

## クイックスタート（フルスタック / Docker）

```bash
cp .env.example .env
# .env を編集（最低限 POSTGRES_PASSWORD を変更。Claude機能を使うなら ANTHROPIC_API_KEY も）

just up            # = docker compose up -d --build
```

- Web UI: http://localhost:8081
- API: フロントの nginx 経由で `/api/*` がバックエンドにプロキシされます

停止: `just down`

## ローカル開発（ホットリロード）

3つのターミナルで:

```bash
just dev-db        # PostgreSQL だけ Docker で起動
just back          # バックエンド（cargo watch、:8080）
just front         # フロント（Vite、:5173 → /api を :8080 にプロキシ）
```

`.env` の `DATABASE_URL` はローカル実行時 `localhost:5432` を指す設定にしてください（`.env.example` 参照）。

## Claude API（要約・翻訳）

`.env` に `ANTHROPIC_API_KEY` を設定すると、記事画面の「要約」「翻訳」ボタンが有効になります。未設定でも RSS リーダーとして動作します（該当機能のみ `503 not enabled` を返します）。

- 既定モデル: `claude-sonnet-4-6`（`.env` の `ANTHROPIC_MODEL` で変更可）
- 処理結果は DB にキャッシュされ、同一記事・同一言語の再要求ではトークンを消費しません

## イメージのビルドと配布（NAS など）

```bash
# .env に REGISTRY（例: ghcr.io/your-name または nas.local:5000）と TAG を設定
just docker-build  # backend / frontend イメージをビルド
just docker-push   # レジストリへ push
```

NAS 側では push したイメージを参照する compose を用意するか、`docker-compose.yml` をそのまま使ってビルドさせます。

## 主な API

| Method | Path | 説明 |
|--------|------|------|
| GET | `/api/health` `/api/health/db` | ヘルスチェック |
| GET/POST | `/api/feeds` | フィード一覧 / 追加 |
| DELETE | `/api/feeds/{id}` | フィード削除 |
| POST | `/api/feeds/{id}/refresh` | 取得を実行 |
| GET | `/api/articles?feed_id=&unread=` | 記事一覧 |
| GET | `/api/articles/{id}` | 記事取得 |
| POST | `/api/articles/{id}/read` | 既読/未読 |
| POST | `/api/articles/{id}/summarize` | 要約（`{"lang":"ja"}`） |
| POST | `/api/articles/{id}/translate` | 翻訳（`{"lang":"ja"}`） |

## ライセンス

任意（未設定）。
