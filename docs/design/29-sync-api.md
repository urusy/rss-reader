# 29 Google Reader / Fever 同期 API（設計スタブ）

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッションの実装者。
> **本書はスタブ（概略）である。** 既存の機能設計書（例: `05-instapaper-integration.md` / `24-tags-and-auto-tagging.md`）のような完全版ではない。**着手前に本書を §「詳細化の注記」に従って個別詳細化すること。**

---

## 1. 概要 / 価値

NetNewsWire・Reeder・Fluent Reader などの標準 RSS クライアントは、サーバ同期に **Google Reader API（GReader）** と **Fever API** の二大デファクト互換プロトコルを話す。本機能はそのいずれか（または両方）の互換エンドポイントを実装し、**手元のセルフホスト RSS リーダーを既存クライアントの同期バックエンドにする**。

価値:

- **1ユーザー・多デバイス**: iPhone / iPad / Mac のネイティブクライアントで購読・既読・スター状態が同期される。本リーダー独自 UI を作り込まずとも、成熟したクライアント UX をそのまま得られる。
- **既存資産へのマッピングに徹する**: フィード・記事・フォルダ・既読は既に `feeds` / `articles` / `folders` スライスと `articles.is_read` が持っている。本機能は**新しいドメインを増やさず**、これらを GReader/Fever のワイヤ形式へ変換するアダプタ層に近い。
- **token 認証**: クライアントは初回にログインしてトークンを得て、以降のリクエストに載せる。単一ユーザー前提なので機能 14（認証）の `AUTH_TOKEN` / ログイン基盤に相乗りする。

**まず GReader 互換を優先**（NetNewsWire/Reeder が主対象、フォルダ＝タグ階層を表現でき表現力が高い）。Fever は star/saved 中心で簡素だが、対応クライアントが別系統のため**第2フェーズの任意拡張**とする。

---

## 2. 想定スライス & テーブル概略

### スライス

新スライス **`backend/src/features/sync/`**（Vertical Slice。`domain` / `repository` / `service` / `handler` / `mod`）を 1 枚追加し、`features/mod.rs` に `pub mod sync;` ＋ `.merge(sync::routes())` を 1 行ずつ足す。既存スライスは触らない。

- **GReader/Fever のワイヤ形式（JSON / form-encoded）への変換は本スライス内で完結**させる。`feeds` / `articles` / `folders` の **読み取り**は、`instapaper/repository.rs::get_article_ref` の前例にならい本スライスの SQL で直接引く（クロススライスの書き込み所有は移さない）。
- 既読トグルなど **書き込みは既存 service を呼ぶか同等 SQL** を本スライスに持つ（`articles` の `is_read` 更新は `articles/service.rs::mark_read` 相当、一括既読は `mark_all_read` 相当）。スター（star/saved）は既存資産に無いため後述のテーブルが要る。

### テーブル概略（マイグレーション番号は着手直前に要確認 — 最新は `0005_search.sql`）

| テーブル | 用途 | 概略カラム |
|---|---|---|
| `sync_starred`（新規・スターを既存資産で表現できない場合のみ） | GReader の starred / Fever の saved を保持 | `article_id UUID PK FK(articles.id) ON DELETE CASCADE`, `created_at TIMESTAMPTZ NOT NULL DEFAULT now()` |
| `sync_tokens`（任意・機能14のトークン基盤で代替可なら不要） | GReader の `ClientLogin` Auth トークン / `T=` write-token の払い出し管理 | `token TEXT PK`, `created_at TIMESTAMPTZ`, `last_used_at TIMESTAMPTZ` |

> **再利用優先**: 既読は `articles.is_read`（＋未読部分インデックス）、購読は `feeds`、カテゴリ／フォルダは `folders` ＋ `feeds.folder_id` をそのまま GReader の「subscription / tag / folder」へマップする。**新テーブルはスター用の最小1枚に抑える**のが原則。トークンは機能 14 を流用できれば `sync_tokens` を作らない。新規マイグレーションは次の空き番号 `0006_*.sql` から（既存ファイルは編集しない・追記のみ）。

---

## 3. 主要エンドポイント（概略）

GReader 互換は `/api/greader/...`（または nginx で `/reader/api/0/...` を本バックエンドへプロキシ）に寄せる。代表例:

| メソッド・パス | 役割 | マップ先 |
|---|---|---|
| `POST /accounts/ClientLogin` | ログイン → Auth トークン発行 | 機能14のトークン検証 |
| `GET  /reader/api/0/token` | 書き込み用トークン取得 | 同上 |
| `GET  /reader/api/0/subscription/list` | 購読フィード一覧 | `feeds` ＋ `folders`（カテゴリ） |
| `GET  /reader/api/0/tag/list` | フォルダ（タグ）一覧 | `folders` |
| `GET  /reader/api/0/stream/contents/{id}` | 記事ストリーム取得 | `articles`（feed/folder/状態で絞り） |
| `GET  /reader/api/0/stream/items/ids` | 未読/スター ID 列 | `articles.is_read` / `sync_starred` |
| `POST /reader/api/0/edit-tag` | 既読・スターの一括トグル | `articles.is_read` 更新 / `sync_starred` upsert・delete |
| `POST /reader/api/0/subscription/edit` | 購読の追加・改名・フォルダ移動・削除 | `feeds` ＋ `feeds.folder_id` |
| `POST /reader/api/0/mark-all-as-read` | フィード/フォルダ一括既読 | `articles` 一括更新（既存 mark-all 相当） |

Fever 互換（第2フェーズ・任意）は単一エンドポイント `POST /api/fever/?api`（form-encoded、`groups` / `feeds` / `items` / `unread_item_ids` / `saved_item_ids` / `mark` 等のクエリで分岐）。

> 正確なパラメータ名・レスポンス JSON 構造・`continuation` ページングは**実装時に各プロトコル仕様で要確認**（§7）。本書は形のみを示す。

---

## 4. 主なリスク / ops 考慮

| 項目 | 内容 | 対処 |
|---|---|---|
| **プロトコル仕様の非公式性** | GReader API は Google 廃止済みで公式仕様なし。Fever も原典が消失気味。実装は FreshRSS / Miniflux など先行 OSS の挙動が事実上の標準 | 着手時に FreshRSS / Miniflux のエンドポイント実装と、対象クライアント（NetNewsWire）の実リクエストを参照。**詳細化時に必ず実機キャプチャで検証** |
| **クライアント互換のばらつき** | NetNewsWire / Reeder / Fluent でヘッダ・期待レスポンスが微妙に異なる | 対象クライアントを1つに絞って MVP を通し、HTTP スモークを `scripts/test/api-sync.sh` で固定 |
| **ID 形式の変換** | GReader は `tag:google.com,2005:reader/item/<hex16>` 等の長い stream/item ID を要求。内部は UUID | 変換規則を純粋関数（`encode_item_id` / `decode_item_id`）に切り出し単体テスト |
| **ページング / 件数** | `n=` 件数、`continuation` トークン、`xt`（除外）など。全件返すとクライアントが固まる | カーソルページングを `service` で実装。`idx_articles_published_at` を活用 |
| **書き込み認可** | GReader は read token と write token を分け CSRF 的に扱う | 単一ユーザーなので機能14のトークンで一本化可。ただし token 取得エンドポイントは形だけ満たす |
| **認証の相乗り** | 本リーダーの `Authorization: Bearer` と GReader の `Authorization: GoogleLogin auth=...` はヘッダ流儀が違う | 機能14ミドルウェアの**除外/別検証ルート**として `sync` を扱う（GReader 形式のヘッダを本スライスで検証）。機能14未実装だと無認証で同期が露出する点に注意 |
| **ops: nginx** | `/reader/api/0/...` のパスをバックエンドへ通す必要 | nginx に `location /reader/` のプロキシ追記。設定は別途 |
| **タイムゾーン / epoch** | GReader はミリ秒/マイクロ秒 epoch を多用 | 変換ユーティリティを純粋関数化しテスト |

---

## 5. 依存（先に必要な機能）

- **14 認証 / アクセス制御（ハード依存）**: token 認証の土台。`AUTH_TOKEN` / ログイン検証を流用する。機能14が無いと同期 API が無認証で露出し、課金される Anthropic キーや全データへ無防備になる。**14 を先に通すこと。**
- **02 フィードのフォルダ分け（ソフト依存）**: GReader の「tag/folder」「カテゴリ」を `folders` ＋ `feeds.folder_id` にマップする。未実装ならフォルダ無し（フラット購読）の縮退対応で MVP は成立。
- **01 フィード管理（ソフト依存）**: `subscription/edit`（改名・フォルダ移動・削除）は機能01の `PATCH/DELETE /api/feeds` と同じ書き込みを使う。再利用すると重複が減る。

既存資産（`feeds` / `articles` / `articles.is_read` ＋未読部分インデックス / `folders`）は前提として利用する。

---

## 6. 工数感

**L（大）** 〜 部分的に XL。内訳の目安:

- GReader 互換コア（login / subscription list / stream contents / item ids / edit-tag / mark-all）と ID・epoch 変換、ページング: **中〜大**。プロトコル調査と実機検証に時間が偏る。
- スター用 `sync_starred` マイグレーション 1 枚 ＋ リポジトリ: 小。
- 機能14への相乗り（ヘッダ別検証ルート）: 小〜中。
- Fever 互換（第2フェーズ）: 別途 中。**MVP では非スコープにしてよい。**

MVP の現実的な切り口は「**NetNewsWire 1 クライアント × GReader 読み取り＋既読同期**」に絞ること。subscription 編集・スター・Fever は段階的に足す。

---

## 7. 詳細化の注記

**本書はスタブである。実装着手前に、本書を起点として個別の完全版設計書へ詳細化すること。** CHEATSHEET / 既存設計書の章立て（概要 / スコープ・非スコープ / 既存実装の再利用 / データモデル / バックエンド / フロントエンド / API 契約 / 依存関係 / テスト計画(TDD) / 実装手順 / リスク）に揃え、特に以下を確定させてから着手する。

1. **対象プロトコルとクライアントの確定**（GReader のみ / Fever 併用、主対象クライアント1つ）。
2. **マイグレーション番号の確認**（`backend/migrations/` の最新 `0005_search.sql` を見て `sync_starred` / 必要なら `sync_tokens` を `0006` 以降で採番。既存ファイルは編集しない・追記のみ）。
3. **全エンドポイントの request/response 実形を実機キャプチャで確定**（FreshRSS / Miniflux 実装と NetNewsWire の実トラフィックを参照）。
4. **ID・epoch 変換規則の純粋関数化と単体テスト**（`encode_item_id` / `decode_item_id` 等）。
5. **機能14との認証統合方式の確定**（GReader 形式ヘッダの検証ルート）。
6. **nginx ルーティング**（`/reader/` プロキシ）の設定追記。

> 規約遵守: Vertical Slice（新スライス1枚＋`.merge()`1行）、sqlx は runtime クエリのみ（`query!` 禁止）、`shared/llm` 以外に trait/dyn を足さない、`AppError` 既存6バリアントで表現（`error.rs` 不編集）、マイグレーションは追記のみ。
