# 29 Google Reader 同期 API（GReader 互換・完全版）

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッションの実装者。本書だけで着手できるよう、再利用資産・SQL・関数シグネチャ・ルート文字列・ワイヤフォーマット・テスト順序まで具体化する。
>
> **重要な但し書き**: GReader プロトコルは非公式仕様であり、事実上の標準は FreshRSS / Miniflux の実装。本書のワイヤ仕様は Miniflux `internal/googlereader/`・FreshRSS `greader.php`・NetNewsWire `ReaderAPICaller.swift`・Fluent Reader のソースを突き合わせて確定済みだが、最終確認は対象クライアントの実トラフィックで行うこと（§9.5）。
>
> **Fever API はスコープから完全に削除した**（旧スタブの「第2フェーズ・任意」も撤回）。理由: 主対象 NetNewsWire・副対象 Reeder は GReader で全機能が動き、Fever は読み取り専用寄り・別系統クライアント向けで、実装/保守コストに対する追加価値がない。将来必要になっても別スライスで独立に追加でき、本設計と干渉しない。

---

## 1. 概要

サードパーティ RSS クライアント（**主対象: NetNewsWire**、副対象: Reeder、ベストエフォート: ReadKit / Fluent Reader）から本リーダーを同期バックエンドとして使えるようにする Google Reader 互換 API（GReader API）を、新スライス `backend/src/features/sync/` として追加する。

- 認証は GReader 流儀（`POST /accounts/ClientLogin` → `Authorization: GoogleLogin auth=<token>`）。既存の Cookie セッション認証（migration 0022）とは**完全に別系統**で共存させる（相互に乗り入れ不可 — §3.4 の不変条件）。
- 同期対象は購読（feeds/folders）・記事（未読/既読・スター）・記事本文。**サーバー側の真実は既存テーブル**（`articles.is_read` / `article_stars` / `feeds.folder_id`）で、GReader は既存 UI と同じ状態を読む/書くだけの「別の顔」。
- エンドポイント面は **Miniflux の実証済み 13 ルート + unread-count + bare stream/contents + catch-all の 16 ルート**を一括実装する。Reeder は user-info / unread-count / mark-all-as-read を実使用し、Fluent Reader は bare `stream/contents` しか呼ばないため、「NNW 最小面 + catch-all の `[]` で吸収」戦略は採らない（mark-all-as-read は**書き込み**であり、`[]` を返すとクライアントが失敗と解釈して永久リトライする）。
- **既定は無効**。`SYNC_API_ENABLED=true` の明示 opt-in でのみルートがマージされる（CF Tunnel での外部公開方針があるため。§11）。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）

| # | メソッド | パス | 主要呼び出し元 |
|---|---|---|---|
| 1 | **POST のみ** | `/accounts/ClientLogin` | 全クライアント（トークン発行） |
| 2 | GET | `/reader/api/0/token` | NNW, Reeder |
| 3 | GET | `/reader/api/0/user-info` | Reeder, ReadKit, Fluent（ログイン確認プローブ） |
| 4 | GET | `/reader/api/0/tag/list` | 全クライアント |
| 5 | GET | `/reader/api/0/subscription/list` | 全クライアント |
| 6 | POST | `/reader/api/0/subscription/quickadd` | NNW, Reeder（フィード追加） |
| 7 | POST | `/reader/api/0/subscription/edit` | NNW（改名/削除/フォルダ移動/解除） |
| 8 | GET | `/reader/api/0/stream/items/ids` | 全クライアント（同期の中核） |
| 9 | POST | `/reader/api/0/stream/items/contents` | 全クライアント |
| 10 | GET | `/reader/api/0/stream/contents` + `/reader/api/0/stream/contents/{*stream}` | Fluent（bare 形しか呼ばない）, NewsFlash 系 |
| 11 | POST | `/reader/api/0/edit-tag` | 全クライアント（既読/スター） |
| 12 | POST | `/reader/api/0/mark-all-as-read` | Reeder, Fluent, ReadKit（NNW は不使用） |
| 13 | POST | `/reader/api/0/rename-tag` | NNW（フォルダ改名） |
| 14 | POST | `/reader/api/0/disable-tag` | NNW（フォルダ削除） |
| 15 | GET | `/reader/api/0/unread-count` | Reeder（バッジ）, ReadKit |
| 16 | ANY | `/reader/api/0/{*rest}`（catch-all） | 未知プローブ → `200` + `[]`（**認証ミドルウェアの内側**・`tracing::warn!` でログ） |

付帯: migration `0024_greader_sync.sql`（採番は §4.1）、nginx 2 location、トークン管理 API（protected 側 `GET/DELETE /api/sync/tokens*`）+ Settings 画面の最小 UI、スモークスクリプト `scripts/test/api-greader.sh`。

### 非スコープ（本機能では実装しない — カット根拠付き）

- **Fever API** — 全面削除（冒頭の但し書き参照）。
- **記事単位ラベル（`edit-tag a=user/-/label/X`）→ tags スライス連携** — 対象クライアントはラベルをほぼ購読フォルダとしてのみ使う。**受理して `OK` を返し無視**（Miniflux と同じ）。tag/list はフォルダのみを label として返し、tags スライスとの名前空間衝突を回避。
- **`subscription/import`（OPML）/ `export`** — 既存の Web UI 側 OPML（設計 17）が担う。catch-all が吸収（NNW 経由 OPML インポートは無反応になる — §11 リスクに記載）。
- **`kept-unread` の独立ストリーム列挙** — edit-tag のタグとしてのみ解釈（read の逆操作）。
- **`broadcast` / `like` / `preference/*` / `friend/*` / `related/*` / `search/items/ids` / Atom(XML) 出力** — モダンクライアント不要。`output=` は受理して無視し常に JSON。
- **複数ユーザー** — 単一ユーザー前提。`Email` は照合しない（§7.1）。
- **`it=` フィルタの実処理** — パースして無視（Miniflux 同等）。
- **`full_content`（readability 抽出結果）の配信** — UI と同じ `content` 列を配信。切替オプションは将来課題（§11）。

---

## 3. 既存実装の調査と再利用

実ファイルを確認済み。以下を**再利用し、車輪の再発明をしない**。

| 再利用資産 | 実体（確認済みファイル） | 本機能での使い方 |
|---|---|---|
| public/protected ルータ分離 | `features/mod.rs::router()` | `sync::routes()` を **public 側**に merge（`require_auth` の Cookie 検証も Origin/CSRF 検査も通らない）。トークン管理 API は protected 側に merge |
| トークンハッシュ | `shared/auth.rs::hash_token`（SHA-256 → base64url unpadded） | `sync_tokens.token_hash` の生成・照合（平文非保存。`auth_sessions` と同水準） |
| ログインレート制限 | `AppState.login_limiter`（`LoginLimiter`、グローバル指数バックオフ） | `ClientLogin` はパスワード等価の攻撃面 → **login と同一 limiter を共有**（独立させると総当たりの迂回路になる） |
| Argon2 検証 | `features/auth/service.rs::verify_password`（現状 private、spawn_blocking 済み） | **`pub(crate)` へ昇格して呼ぶ**（既存スライスへの唯一の変更・可視性 1 語。複製はパラメータ乖離バグの温床のため。CLAUDE.md「既存スライス原則不変」からの逸脱理由として明記） |
| 資格情報取得 | `features/auth/repository.rs::get_credential` | ClientLogin のパスワード照合元 |
| 読み取り射影の前例 | `features/instapaper/repository.rs::ArticleRef` | sync 専用 read-only 射影 struct（素の `Uuid`/`i64` を bind、他スライスの domain 型に依存しない） |
| 既読書き込みの意味論 | `articles/repository.rs::set_read`（素の UPDATE・副作用なし） | sync 側 short_id ベース一括 UPDATE の**等価性の基準**（§5.3 の設計判断 + §9.3 パリティテスト） |
| スター | `features/annotations/repository.rs::add_star / remove_star`（素の `Uuid` を取る）、`article_stars`（0018） | **所有スライスの関数をそのまま呼ぶ**（feeds→articles の cross-slice 呼び出し前例に従う）。新テーブル不要 |
| フィード操作 | `feeds/service.rs::create_feed / delete_feed / update_feed`（tri-state folder patch・背景初回フェッチ） | quickadd / subscription/edit から呼ぶ（副作用の濃い書き込みは必ず所有 service 経由） |
| フォルダ操作 | `folders/repository.rs::insert / update_name / delete`（delete は FK で feeds.folder_id を NULL 化 = GReader 期待動作と一致） | rename-tag / disable-tag / フォルダ移動 |
| 追加ファイルの前例 | `features/digest/email.rs` | 5 ファイル規約 +1 の `wire.rs` を正当化する前例 |
| bool 環境変数パターン | `shared/config.rs`（`matches!(..., "1"\|"true"\|"yes")`） | `SYNC_API_ENABLED` 追加 |
| インデックス | `idx_articles_created_at`（0021）、`idx_articles_is_read`（未読部分）、`article_stars` PK | ot/nt・未読・スター抽出。keyset 用 UNIQUE は 0024 で追加 |
| テストパターン | `shared/auth.rs` の connect_lazy ルータテスト、`#[ignore]` 実 DB テスト、`scripts/test/api-auth.sh` | §9 の 4 層テストの雛形 |
| nginx パターン | `frontend/nginx.conf` の resolver+変数 proxy_pass・`$http_host`（本番障害 2 件の対策コメント入り） | 新規 2 location に踏襲 |

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方

執筆時点の最新は `0023_usage_tracking.sql`（**並行セッションが消費済み・未コミット**。スタブの「次は 0006」は完全に陳腐化）。本書は **`0024_greader_sync.sql`** を仮番とするが、ツリーは動いている — **実装着手時に必ず `ls backend/migrations/` を再実行し、マージ時点の最小空き整数で採番すること**（実装手順 §10 の先頭項目）。既存マイグレーションは編集しない（追記のみ）。

### 4.2 スキーマ

追加は「articles に 1 カラム」+「トークンテーブル 1 枚」。旧スタブの `sync_starred` は `article_stars`（0018）が既存のため**不要**。

```sql
-- 0024_greader_sync.sql

-- (1) GReader 互換の 64bit item id。クライアントは item id を int64 として hex/dec
--     変換するため UUID は使えない。既存行は created_at 順に採番し、
--     「short_id の大小 = クロール時系列」を全行で成立させる
--     （keyset continuation と ot/nt フィルタの整合のため。物理順 backfill 不可）。
-- 注意: この UPDATE は articles 全行を書き換える（テーブルロック相当）。
--       家庭スケール（数万行）では数秒。適用は稼働の谷間に。
ALTER TABLE articles ADD COLUMN short_id BIGINT;

UPDATE articles a SET short_id = t.rn
FROM (SELECT id, row_number() OVER (ORDER BY created_at ASC, id ASC) AS rn
      FROM articles) t
WHERE a.id = t.id;

CREATE SEQUENCE articles_short_id_seq AS BIGINT;
SELECT setval('articles_short_id_seq',
              COALESCE((SELECT max(short_id) FROM articles), 0) + 1, false);

ALTER TABLE articles
    ALTER COLUMN short_id SET DEFAULT nextval('articles_short_id_seq'),
    ALTER COLUMN short_id SET NOT NULL;
ALTER SEQUENCE articles_short_id_seq OWNED BY articles.short_id;

CREATE UNIQUE INDEX idx_articles_short_id ON articles (short_id);

-- (2) GReader クライアント用トークン（恒久・ハッシュのみ保存）。
--     auth_sessions（30日TTL・sliding）とは寿命が違うため相乗りせず別テーブル。
CREATE TABLE sync_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash TEXT NOT NULL UNIQUE,
    label TEXT,                                   -- ClientLogin の Email 値（クライアント識別用）
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ
);
```

**設計判断:**

- **順序厳守**: backfill UPDATE → `setval` → `SET DEFAULT` → `SET NOT NULL`。`GENERATED AS IDENTITY` を使わないのは、identity の自動 backfill が物理スキャン順で採番され keyset の並びが壊れるため。
- **値域 `[1, 2^63)` を構造的に保証** — NNW の `Int(hex, radix:16)` は 2^63 以上で失敗し、Fluent Reader は MSB 立ちを負の 10 進で送り返す。連番なら永久に踏まない。`upsert_batch` の `ON CONFLICT` でシーケンス欠番が出るが ID に連続性は不要（無害）。
- **既存コード無変更** — `articles::domain::Article` に `short_id` は足さない。sqlx の `FromRow` は未マップ列を無視するため既存 `SELECT *` 系は全て無影響。`articles::repository::upsert_batch`（UNNEST 列挙 INSERT）には DEFAULT が効くため articles スライスも無変更（§9.3 で裏取りテスト）。
- **トークンはランダム値のハッシュ保存**（`auth_sessions` ミラー）。ステートレス HMAC 導出案（パスワードハッシュを鍵に毎回再計算）は**不採用**: DB 読み取り/NAS バックアップ漏えいから無クラックで実用トークンが導出でき、失効・一覧・ラベルの運用可視性も失う。既存コードの確立パターンに従う。
- **feeds に数値 ID は足さない** — フィードの streamId は `feed/<uuid>`（不透明文字列）。GReader の feed id は Google 本家が `feed/<url>` だった通り仕様上不透明で、NNW/Fluent は文字列一致でしか扱わない（コード確認済み）。Reeder のみ未読解 → §9.5 で早期検証し、問題が出た場合の退路（`feeds.short_id` 追加 + `StreamId::feed_output` のみ差し替え）を §11 に明記。
- **GReader ラベル = folders（1 フィード 1 フォルダ = categories が高々 1 要素）**。tags スライスとは接続しない。

---

## 5. バックエンド設計

新スライス `backend/src/features/sync/`。規約の 5 ファイル + `wire.rs` の 6 ファイル構成（`digest/email.rs` の +1 前例に倣う。認証ミドルウェアは handler.rs 内に置き、ファイルはこれ以上増やさない）。

```
backend/src/features/sync/
  mod.rs         # routes(&AppState) / protected_routes() の export
  domain.rs      # ★純関数層1: ItemId / StreamId / epoch 変換 / paginate / plan_edits / SyncToken
  wire.rs        # ★純関数層2: serde 出力構造体（型で文字列/数値を固定）・レスポンスヘルパ・multi-key form パーサ
  repository.rs  # 読み取り射影 + short_id 起点バッチ書き込み + sync_tokens CRUD
  service.rs     # ストリーム解決・edit-tag 意味論・cross-slice 呼び出し・ClientLogin
  handler.rs     # HTTP 境界: require_sync_auth ミドルウェア + 16 ハンドラ（AppError を JSON にせず GReader 形式へ変換）
```

設計の核: **プロトコルの罠（4 形式の ID、文字列/数値の使い分け、epoch 単位、continuation、edit-tag 意味論）を全部 domain.rs / wire.rs の純関数層に閉じ込め、DB・Axum・時計なしの単体テストで固める**。handler は「パース → service → 整形」に痩せさせる。

新規依存: `form_urlencoded = "1"` のみ（`url` crate の分離クレート。reqwest 経由で既にツリー内に存在するものの直接依存化）。

### 5.1 domain.rs — 純関数層（値オブジェクト）

```rust
/// 定数（テスト対象。マジックナンバー禁止）
pub const DEFAULT_PAGE_SIZE: i64 = 20;        // Google 原典の既定
pub const MAX_PAGE_SIZE: i64 = 1000;          // n= の clamp 上限（Miniflux の無制限は OOM ベクタ）
pub const MAX_ITEMS_PER_REQUEST: usize = 1000; // i= の clamp（超過分は先頭 1000 件を処理 — 400 にしない）
pub const MAX_CONTENT_BYTES: usize = 500_000; // FreshRSS 互換の本文上限

/// GReader item id（articles.short_id）。値域 [1, 2^63) を前提とする newtype。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemId(pub i64);

impl ItemId {
    /// stream/items/ids 用: 符号付き 10 進文字列
    pub fn short_form(self) -> String { self.0.to_string() }
    /// items/contents 用: 長形式（ゼロ埋め 16 桁小文字 hex）
    pub fn long_form(self) -> String {
        format!("tag:google.com,2005:reader/item/{:016x}", self.0 as u64)
    }
    /// 4 形式すべてを受理:
    ///   "tag:google.com,2005:reader/item/00000000148b9369"  (long form padded)
    ///   "tag:google.com,2005:reader/item/2f2"               (NNW: unpadded hex)
    ///   "000000000000048c"                                  (Reeder: bare 16-hex)
    ///   "12345" / "-123"                                    (10 進。負値は Fluent の MSB 再解釈)
    /// hex 枝は u64 でパースして i64 へビット再解釈（i64::from_str_radix は
    /// MSB 立ち 16-hex でエラーになるため不可）。最後に正値フィルタ:
    /// 自前 ID は常に正なので、負値・0 は「存在しない ID」として None（バッチを
    /// 失敗させず黙って落とす — クライアントは削除済み記事の stale ID を平気で送る）。
    pub fn parse(s: &str) -> Option<ItemId> {
        const PREFIX: &str = "tag:google.com,2005:reader/item/";
        let v: i64 = if let Some(hex) = s.strip_prefix(PREFIX) {
            u64::from_str_radix(hex, 16).ok()? as i64
        } else if s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
            u64::from_str_radix(s, 16).ok()? as i64
        } else {
            s.parse::<i64>().ok()?
        };
        (v > 0).then_some(ItemId(v))
    }
}

/// ストリーム ID。user/-/ と user/<任意>/ を等価に受理（user-info が userId="1" を
/// 返す以上 user/1/... も来る）。出力は常に user/-/。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamId {
    ReadingList, Read, KeptUnread, Starred,
    Feed(uuid::Uuid),
    FeedUrl(String),      // feed/<http(s)://...>（ac=subscribe 入力のみ）
    Label(String),        // URL デコード済みフォルダ名（二重デコード禁止）
    Ignored(String),      // broadcast / like / 未知 state → accept-and-OK
}
impl StreamId {
    pub fn parse(raw: &str) -> StreamId;
    pub fn feed_output(feed_id: uuid::Uuid) -> String;  // "feed/<uuid>"
    pub fn label_output(name: &str) -> String;          // "user/-/label/<name>"
}

/// epoch 変換（文字列/数値・単位の罠を一点に集約）
pub fn epoch_secs(t: DateTime<Utc>) -> i64;
pub fn epoch_msec_str(t: DateTime<Utc>) -> String;   // crawlTimeMsec（JSON 文字列）
pub fn epoch_usec_str(t: DateTime<Utc>) -> String;   // timestampUsec / newestItemTimestampUsec（JSON 文字列）
pub fn parse_epoch_secs(s: &str) -> Option<DateTime<Utc>>;  // ot / nt（秒）
/// mark-all-as-read の ts: 16 桁以上 → マイクロ秒、未満 → 秒（Miniflux ヒューリス
/// ティック。Reeder は usec を送る）。欠落 → now。
pub fn parse_ts_param(s: Option<&str>) -> DateTime<Utc>;

/// keyset ページング。n+1 件フェッチした rows を受け、(返す n 件, continuation) を返す。
/// n+1 件目が存在した時だけ Some — 空ページ・ちょうど n 件のページに continuation を
/// 付けない（NNW の無限ループ防止を構造的に保証）。
pub fn paginate(rows: Vec<i64>, n: usize) -> (Vec<i64>, Option<String>);

/// edit-tag 意味論の分解（純関数）。a/r の StreamId 列 → 実行すべき操作列。
pub enum EditOp { MarkRead, MarkUnread, Star, Unstar }
pub fn plan_edits(add: &[StreamId], remove: &[StreamId]) -> Vec<EditOp>;
// a=read → MarkRead / r=read → MarkUnread
// a=kept-unread → MarkUnread / r=kept-unread → MarkRead（read と冗長ペアで来るため整合）
// a|r=starred → Star/Unstar
// Label(_) / Ignored(_) → 何も生成しない（受理して OK）

/// 同期トークン: 32 バイト OS 乱数 → base64url unpadded（43 字。'=' を含まず、
/// naive split するクライアント実装に安全）。auth の SessionToken と同形式だが
/// スライス独立のため自前実装（生成 ~5 行）。
pub struct SyncToken(String);
impl SyncToken { pub fn generate() -> Self; pub fn as_str(&self) -> &str; }

/// "GoogleLogin auth=<token>" のパース（scheme 厳密一致・auth 小文字・
/// 最初の '=' 以降全部をトークンとする）
pub fn parse_google_login_header(value: &str) -> Option<&str>;

/// 本文 500KB 切り詰め（UTF-8 の char 境界で切る。バイトスライスは panic するため
/// char_indices / floor_char_boundary 相当で境界を探す）
pub fn truncate_content(html: &str) -> &str;
```

### 5.2 wire.rs — ワイヤ形式（クライアント decoder の都合を 1 箇所に封じ込める）

```rust
/// query + form body をマージした multi-key パラメータ。
/// axum::extract::Form / serde_urlencoded は i=..&i=..&a=..&r=.. の反復キーを
/// last-wins で潰すため使用禁止。RawForm + RawQuery を form_urlencoded::parse で
/// Vec<(String,String)> に展開しマージ（FreshRSS の $_REQUEST 互換）。
pub struct Params(Vec<(String, String)>);
impl Params {
    pub fn from(query: Option<&str>, body: &[u8]) -> Self;
    pub fn first(&self, key: &str) -> Option<&str>;
    pub fn all(&self, key: &str) -> Vec<&str>;
}

// ---- 出力 serde 構造体（型レベルで文字列/数値を固定） ----
pub struct TagList { pub tags: Vec<TagEntry> }
pub struct TagEntry { pub id: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")] pub kind: Option<String> }
pub struct SubscriptionList { pub subscriptions: Vec<Subscription> }
pub struct Subscription {
    pub id: String,                     // "feed/<uuid>"
    pub title: String,                  // COALESCE(title, url)
    pub categories: Vec<Category>,      // フォルダなしは []
    pub url: String,                    // ★必ず出す（欠落時 NNW が id から URL を捏造）
    #[serde(rename = "htmlUrl")] pub html_url: String,  // サイト URL がないため feed URL で代用
    // iconUrl は省略（空文字より安全。optional 実証済み）
}
pub struct Category { pub id: String, pub label: String }  // ★両方必須・Option にしない（NNW decoder が両方 non-optional）
pub struct ItemRefs { #[serde(rename = "itemRefs")] pub item_refs: Vec<ItemRef>,
    #[serde(skip_serializing_if = "Option::is_none")] pub continuation: Option<String> }
pub struct ItemRef { pub id: String }   // ★符号付き 10 進文字列
pub struct StreamEnvelope {
    pub id: String,        // "user/-/state/com.google/reading-list" 固定（NNW decoder 必須）
    pub updated: i64,      // 現在秒・数値（NNW decoder 必須）
    pub items: Vec<Item>,
    #[serde(skip_serializing_if = "Option::is_none")] pub continuation: Option<String>, // stream/contents 用
}
pub struct Item {
    pub id: String,                                     // 長形式 %016x
    #[serde(rename = "crawlTimeMsec")] pub crawl_time_msec: String,   // ★ミリ秒・文字列（Fluent が ot 水位に実使用 → 実値必須）
    #[serde(rename = "timestampUsec")] pub timestamp_usec: String,    // ★マイクロ秒・文字列（Reeder のソートキー）
    pub published: i64,                                 // ★秒・数値
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub author: Option<String>,
    pub canonical: Vec<Href>,                           // Fluent は canonical[0].href
    pub alternate: Vec<Href>,                           // NNW は alternate[0].href。type は付けない（FreshRSS compat 同様）
    pub categories: Vec<String>,                        // reading-list + read? + starred? + label(folder)?
    pub origin: Origin,                                 // streamId は subscription/list の id と厳密一致
    pub summary: Content,                               // NNW/Fluent は summary しか読まない
    pub content: Content,                               // 同一 HTML を重複掲載（Miniflux 前例・最安全）
}
pub struct Href { pub href: String }
pub struct Origin { #[serde(rename = "streamId")] pub stream_id: String,
    pub title: String, #[serde(rename = "htmlUrl")] pub html_url: String }
pub struct Content { pub content: String }
pub struct UnreadCounts { pub max: i64, pub unreadcounts: Vec<UnreadCountEntry> }
pub struct UnreadCountEntry { pub id: String, pub count: i64,
    #[serde(rename = "newestItemTimestampUsec")] pub newest_item_timestamp_usec: String }
pub struct UserInfo { /* 全値 String: userId="1", userName="reader", userProfileId="1", userEmail="reader" */ }
pub struct QuickAddResult { /* numResults, query, streamId, streamName */ }

// ---- レスポンスヘルパ ----
pub fn ok_plain() -> Response;            // 200, text/plain; charset=UTF-8, body "OK"（書き込み成功の唯一の形）
pub fn unauthorized_sync() -> Response;   // 401 text/plain "Unauthorized"
                                          //  + Google-Bad-Token: true
                                          //  + X-Reader-Google-Bad-Token: true（★両綴り必須）
pub fn bad_auth_clientlogin() -> Response;      // 403 text/plain "Error=BadAuthentication\n"（歴史的 Google 形式に統一。ハイブリッド 401 は採らない）
pub fn client_login_ok(token: &str) -> Response; // "SID=<t>\nLSID=null\nAuth=<t>\n"
pub fn internal_error() -> Response;      // 500 text/plain "Internal Server Error"（詳細は tracing のみ）
```

### 5.3 repository.rs — 読み: 自前射影 / 書き: 2 系統

instapaper 前例に従い、素の `Uuid`/`i64` を bind し他スライスの domain 型に依存しない。

```rust
#[derive(sqlx::FromRow)]
pub struct SubscriptionRow { pub id: Uuid, pub url: String, pub title: Option<String>,
                             pub folder_name: Option<String> }
pub async fn list_subscriptions(pool: &PgPool) -> AppResult<Vec<SubscriptionRow>>;
// SELECT f.id, f.url, f.title, fo.name AS folder_name
//   FROM feeds f LEFT JOIN folders fo ON fo.id = f.folder_id ORDER BY f.created_at

pub async fn list_folder_names(pool) -> AppResult<Vec<String>>;
pub async fn folder_id_by_name(pool, name: &str) -> AppResult<Option<Uuid>>;

/// service が StreamId + params から組み立てる型付きフィルタ
pub struct StreamFilter {
    pub feed_id: Option<Uuid>,
    pub folder_name: Option<String>,
    pub starred_only: bool,
    pub unread_only: bool,               // xt=read
    pub read_only: bool,                 // s=.../read
    pub since: Option<DateTime<Utc>>,    // ot → created_at >=
    pub until: Option<DateTime<Utc>>,    // nt → created_at <=
    pub cursor: Option<i64>,             // continuation
    pub ascending: bool,                 // r=o
    pub limit: i64,                      // clamp 済み n + 1（n+1 フェッチ）
}
pub async fn list_item_ids(pool, f: &StreamFilter) -> AppResult<Vec<i64>>;
// SELECT a.short_id FROM articles a
// WHERE a.muted_at IS NULL
//   AND ($feed IS NULL OR a.feed_id = $feed)
//   AND ($folder IS NULL OR a.feed_id IN (SELECT id FROM feeds WHERE folder_id =
//         (SELECT id FROM folders WHERE name = $folder)))
//   AND (NOT $unread_only OR a.is_read = false)
//   AND (NOT $read_only   OR a.is_read = true)
//   AND (NOT $starred_only OR EXISTS (SELECT 1 FROM article_stars s WHERE s.article_id = a.id))
//   AND ($since IS NULL OR a.created_at >= $since)
//   AND ($until IS NULL OR a.created_at <= $until)
//   AND ($cursor IS NULL OR (CASE WHEN $asc THEN a.short_id > $cursor ELSE a.short_id < $cursor END))
// ORDER BY a.short_id DESC|ASC LIMIT $limit    -- 動的連結せず「($k IS NULL OR ...)」型 + asc/desc 2 変種

#[derive(sqlx::FromRow)]
pub struct ItemRow { pub short_id: i64, pub url: String, pub title: String, pub content: String,
    pub author: Option<String>, pub published_at: Option<DateTime<Utc>>, pub created_at: DateTime<Utc>,
    pub is_read: bool, pub starred: bool,
    pub feed_id: Uuid, pub feed_title: Option<String>, pub feed_url: String,
    pub folder_name: Option<String> }
pub async fn items_by_short_ids(pool, ids: &[i64]) -> AppResult<Vec<ItemRow>>;      // WHERE short_id = ANY($1)。存在しない ID は黙って落ちる
pub async fn list_stream_items(pool, f: &StreamFilter) -> AppResult<Vec<ItemRow>>;  // stream/contents 用（一発）
pub async fn article_ids_by_short_ids(pool, ids: &[i64]) -> AppResult<Vec<(i64, Uuid)>>; // スター用の解決

// ---- 書き込み系統 1: 既読（sync 所有の short_id キーで一括 UPDATE） ----
/// ★このクエリは articles::repository::set_read（UPDATE articles SET is_read = $2）と
///   意味論的に等価であることを維持する義務がある。articles 側の既読化に副作用が
///   付いた場合は本関数も追随すること。等価性は実 DB パリティテスト（§9.3）で固定。
pub async fn set_read_by_short_ids(pool, ids: &[i64], read: bool) -> AppResult<u64>;
// UPDATE articles SET is_read = $2 WHERE short_id = ANY($1)
pub async fn mark_all_read(pool, feed_id: Option<Uuid>, folder_name: Option<String>,
                           older_than: DateTime<Utc>) -> AppResult<u64>;
// UPDATE articles SET is_read = true
// WHERE muted_at IS NULL AND created_at <= $older_than AND <scope 条件>
// （muted 記事は配信していないので既読化しない — UI の mark_all_read と意図的差異。コメント明記）

// ---- 書き込み系統 2: スター（所有スライスの関数を呼ぶ。repository には置かない） ----
// service 層で article_ids_by_short_ids で Uuid に解決後、
// annotations::repository::add_star(pool, uuid) / remove_star(pool, uuid) を単件ループ。
// スターのバッチはユーザー操作起点で小さいため往復は問題にならない。

// ---- sync_tokens ----
pub async fn insert_token(pool, token_hash: &str, label: Option<&str>) -> AppResult<Uuid>;
pub async fn prune_tokens_for_label(pool, label: Option<&str>, keep: i64) -> AppResult<u64>;
// DELETE FROM sync_tokens WHERE label IS NOT DISTINCT FROM $1
//   AND id NOT IN (SELECT id FROM sync_tokens WHERE label IS NOT DISTINCT FROM $1
//                  ORDER BY created_at DESC LIMIT $2)
// → 再ログインをループする行儀の悪いクライアントでも行数が無限増殖しない（既定 keep=10）
pub async fn find_token(pool, token_hash: &str) -> AppResult<Option<Uuid>>;
pub async fn touch_token(pool, id: Uuid) -> AppResult<()>;   // last_used_at（1 時間スロットル — TOUCH_AFTER と同じ発想）
pub async fn list_tokens(pool) -> AppResult<Vec<TokenRow>>;  // id, label, created_at, last_used_at
pub async fn delete_token(pool, id: Uuid) -> AppResult<bool>;

pub async fn unread_counts(pool) -> AppResult<Vec<UnreadRow>>;
// SELECT a.feed_id, fo.name AS folder_name, count(*) AS cnt, max(a.created_at) AS newest
// FROM articles a JOIN feeds f ON f.id = a.feed_id LEFT JOIN folders fo ON fo.id = f.folder_id
// WHERE a.is_read = false AND a.muted_at IS NULL GROUP BY a.feed_id, fo.name
// （フォルダ集計・合計行・max は service 層で合成 — §7.15 の完全形を参照）
```

**書き込み所有権の設計判断**: スターは `annotations::repository` の既存関数（素の `Uuid` を取る）を呼ぶ — feeds→articles の cross-slice 呼び出しが認められた前例に従う。既読は (a) stale ID でバッチ全体を失敗させない、(b) 数百 ID の一括送信に N 往復は非現実的、(c) short_id は sync 所有概念で articles スライスに漏らさない、の 3 点から sync repository の一括 UPDATE とする。ただし**等価性コメント + 実 DB パリティテスト（§9.3）を必須**とする。instapaper 前例が認めるのは読み取り専用射影のみであり、この一括 UPDATE はその外側に立つ意識的な逸脱である（理由は上記）。

### 5.4 service.rs

```rust
pub enum ClientLoginOutcome { Ok(SyncToken), BadCredentials, RateLimited(Duration) }
pub async fn client_login(state: &AppState, email: Option<String>, passwd: &str)
    -> AppResult<ClientLoginOutcome>;
// login_limiter.check → auth::repository::get_credential
//   → auth::service::verify_password（pub(crate) 化。spawn_blocking 済み）
//   → 成功: SyncToken::generate → hash_token → insert_token(label=email)
//           → prune_tokens_for_label(keep=10) → record_success
//   → 失敗: record_failure → BadCredentials

pub async fn verify_sync_token(state: &AppState, presented: &str) -> AppResult<Option<Uuid>>;
// hash_token(presented) で find_token（ハッシュ索引一致なので比較器は不要 — 生値比較を
// 書く箇所が生じたら必ず shared/auth.rs::constant_time_eq）。ヒット時 touch_token。

pub async fn resolve_stream(pool, s: &StreamId, params: &Params) -> AppResult<StreamFilter>;
pub async fn item_ids(state, f: StreamFilter, n: usize) -> AppResult<(Vec<i64>, Option<String>)>;
// list_item_ids(limit = n+1) → domain::paginate

pub async fn edit_tag(state, ids: Vec<ItemId>, add: Vec<StreamId>, remove: Vec<StreamId>) -> AppResult<()>;
// plan_edits → MarkRead/MarkUnread は set_read_by_short_ids、Star/Unstar は
// article_ids_by_short_ids → annotations::repository::add_star / remove_star。
// 未知 ID・Label・Ignored が混ざっても Err にしない（常に OK 相当で返る）。

pub async fn quick_add(state, url: &str) -> AppResult<QuickAddResult>;
// "feed/" prefix を剥がして feeds::service::create_feed（201 即返し・背景初回フェッチ）。
// streamId = StreamId::feed_output(feed.id) — subscription/list と厳密一致（NNW が照合）。
// title 未確定時 streamName は URL でフォールバック（次回同期で治る仕様、と明記）。
// URL パース不能 → 200 のまま {"numResults":0}。

pub async fn subscription_edit(state, ac: &str, streams: Vec<StreamId>,
    title: Option<String>, add_label: Option<String>, remove_label: bool) -> AppResult<()>;
// ac=unsubscribe → 各 Feed(id) に feeds::service::delete_feed
// ac=edit:
//   t=       → feeds::service::update_feed(title)
//   a=label/X → folder_id_by_name / なければ folders::repository::insert（暗黙作成。
//              GReader にフォルダ作成 API はなくラベル付与が作成を兼ねる）
//              → update_feed(folder = Some(Some(id)))。r= 同時指定は a= 優先（NNW の move は r+a 同時送信）
//   r= のみ  → update_feed(folder = Some(None))   ★未分類へ。未実装だと NNW の
//              「フォルダから外す」が壊れる（Miniflux 未実装 → NNW issue #3512 の実績）
// ac=subscribe → FeedUrl(url) → create_feed（+ a= があれば続けてフォルダ割当）

pub async fn rename_folder(state, from: &str, to: &str) -> AppResult<()>;      // folders::repository::update_name
pub async fn delete_folder_by_name(state, name: &str) -> AppResult<()>;        // folders::repository::delete（FK SET NULL で所属フィードは未分類化 = GReader 期待動作）
pub async fn mark_all_as_read(state, s: StreamId, ts: DateTime<Utc>) -> AppResult<()>;
pub async fn unread_count_payload(state) -> AppResult<UnreadCounts>;           // §7.15 の合成（feed 行 + folder 行 + reading-list 合計行 + max）
```

### 5.5 handler.rs — 認証ミドルウェア + 16 ハンドラ

**AppError の JSON `IntoResponse` はワイヤに出さない。** ハンドラは `Response` を直接返し、`AppResult` を境界で以下の表にマップする（`fn to_sync_response<T>(r: AppResult<T>, ok: impl FnOnce(T) -> Response) -> Response`）:

| 状況 | HTTP | ボディ / ヘッダ |
|---|---|---|
| 書き込み成功 | 200 | `text/plain; charset=UTF-8` の `OK`（JSON / 204 はクライアントが失敗と解釈し永久リトライする） |
| 読み取り成功 | 200 | `application/json` |
| ClientLogin 失敗 | 403 | `text/plain` `Error=BadAuthentication\n` |
| ClientLogin レート制限 | 429 | 同上ボディ + `Retry-After`（非 200 は一律「認証失敗」扱いされるため安全） |
| トークン不正/欠落 | 401 | `text/plain` `Unauthorized` + **`Google-Bad-Token: true` と `X-Reader-Google-Bad-Token: true` の両方** |
| 内部エラー | 500 | `text/plain` `Internal Server Error`（詳細は `tracing::error!` のみ。内部情報を漏らさない） |
| 未実装パス（catch-all） | 200 | `[]` + `tracing::warn!(path)` |

```rust
/// 認証ミドルウェア。per-handler extractor でなく middleware なのは、extractor は
/// 書き忘れ = 無認証公開になるのに対し、ルーター単位の layer は secure-by-default のため
/// （require_auth と同型）。GET / POST とも Authorization ヘッダで認証（FreshRSS モデル。
/// Miniflux 式「POST は T= で認証」は T を送らない Fluent を壊す）。
/// T= は存在しても一切検証しない（Reeder の T=x・Fluent の T なし・NNW の正規 T すべて通る）。
/// Cookie は一切読まない（★不変条件: sync ルートは Cookie を読まず、GoogleLogin
/// トークンは /api/* で受理されない。§9.2 でテスト固定）。
pub async fn require_sync_auth(State(state), req, next) -> Response;

pub async fn client_login(...);          // POST のみ。Params(body 優先) から Email/Passwd。
                                         // accountType/service/client/output は無視。
                                         // GET はルート未登録 → 405/404（★Passwd がクエリ文字列
                                         // = nginx/CF アクセスログに載る事故を排除。対象 4
                                         // クライアントに GET を使うものはない）
pub async fn token(...);                 // 提示された auth トークンをそのまま text/plain + "\n" で返す
pub async fn user_info(...);
pub async fn tag_list(...);
pub async fn subscription_list(...);
pub async fn quickadd(...);
pub async fn subscription_edit(...);
pub async fn stream_items_ids(...);      // クエリ手動パース（s/n/xt/it/ot/nt/r/c。ck/client/likes/comments は無視）
pub async fn stream_items_contents(...);
pub async fn stream_contents(...);       // bare（reading-list 既定）+ /{*stream} + ?s= 変種
pub async fn edit_tag(...);
pub async fn mark_all_as_read(...);
pub async fn rename_tag(...);
pub async fn disable_tag(...);
pub async fn unread_count(...);
pub async fn catch_all(...);             // tracing::warn!("greader: unimplemented path", %path) → 200 "[]"

// protected 側（Cookie セッション保護。features/mod.rs の protected へ merge）
pub async fn list_sync_tokens(...);      // GET /api/sync/tokens
pub async fn revoke_sync_token(...);     // DELETE /api/sync/tokens/{id}
```

### 5.6 mod.rs（routes）

```rust
pub mod domain; pub mod wire; mod handler; mod repository; mod service;

use axum::{middleware, routing::{any, delete, get, post}, Router};
use crate::shared::state::AppState;

/// GReader 面（public 側に merge。SYNC_API_ENABLED=true のときだけ呼ばれる）
pub fn routes(state: &AppState) -> Router<AppState> {
    let api = Router::new()
        .route("/token", get(handler::token))
        .route("/user-info", get(handler::user_info))
        .route("/tag/list", get(handler::tag_list))
        .route("/subscription/list", get(handler::subscription_list))
        .route("/subscription/quickadd", post(handler::quickadd))
        .route("/subscription/edit", post(handler::subscription_edit))
        .route("/stream/items/ids", get(handler::stream_items_ids))
        .route("/stream/items/contents", post(handler::stream_items_contents))
        .route("/stream/contents", get(handler::stream_contents))
        .route("/stream/contents/{*stream}", get(handler::stream_contents))
        .route("/edit-tag", post(handler::edit_tag))
        .route("/mark-all-as-read", post(handler::mark_all_as_read))
        .route("/rename-tag", post(handler::rename_tag))
        .route("/disable-tag", post(handler::disable_tag))
        .route("/unread-count", get(handler::unread_count))
        .route("/{*rest}", any(handler::catch_all))   // ★ミドルウェアの内側 = 認証必須
        // 利用記録は require_sync_auth の内側（protected 側の features/mod.rs と同型の並び。
        // 未認証 401 は記録されない）。GReader トラフィックも usage 計測対象とする
        // （ユーザー決定 2026-07-07 — 外部クライアントのポーリング頻度も可視化する）。
        .layer(middleware::from_fn(crate::features::usage::track_usage))
        .layer(middleware::from_fn_with_state(state.clone(), handler::require_sync_auth));
    Router::new()
        .route("/accounts/ClientLogin", post(handler::client_login))  // ★POST のみ
        .nest("/reader/api/0", api)
}

/// トークン管理面（protected 側に merge）
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/api/sync/tokens", get(handler::list_sync_tokens))
        .route("/api/sync/tokens/{id}", delete(handler::revoke_sync_token))
}
```

### 5.7 features/mod.rs への追加

```rust
pub mod sync;
// router(state) 内:
//   protected チェーンに .merge(sync::protected_routes()) を 1 行追加
//   public 構築後:
if state.config.sync_api_enabled {
    public = public.merge(sync::routes(&state));
}
```

- **必ず public 側**（`require_auth` の Cookie 検証・Origin/CSRF 検査を通らない。GReader クライアントは Cookie も Origin も送らない）。
- **フラグ false 時はルート自体をマージしない**（`NotEnabled` 503 ではなく存在自体を隠す。無認証到達面のため）。実装時判明: このアプリでは未マッチパスは protected 側 `require_auth` の汎用 401 JSON になる（axum の `.layer` はフォールバックにも掛かる）ため、無効時の応答は 404 ではなく「任意の未知パスと同一の汎用 401」。存在秘匿の意図は満たす（§9.2 のテストは同一応答性で固定）。
- **usage 計測（決定済み 2026-07-07）**: GReader トラフィックも usage-tracking の計測対象とする。§5.6 のとおり sync の api ルータに `usage::track_usage` を `require_sync_auth` の内側に layer する（protected 側 `features/mod.rs` の並び順パターンと同型。未認証 401 は記録されない）。`/accounts/ClientLogin` は layer の外 = 非計測（UI 側の `/api/auth/login` も非計測であり整合）。
- **並行セッション注意**: `features/mod.rs`・usage スライスは usage-tracking セッションが編集中（未コミット）。マージ時に現物のルーター構成・`track_usage` のシグネチャを確認して合成すること。

### 5.8 AppError の使い分け（error.rs は不編集）

`AppError` に 401/403/429 バリアントは追加しない（既存 auth と同じ思想）。sync 内部は `AppResult` で伝播し、handler 境界で §5.5 の表に従い raw `Response` に変換。500 も `wire::internal_error()`（text/plain）に落とし、`AppError::Database` の JSON 形をワイヤに漏らさない。

### 5.9 既存スライスへの変更（唯一・1 語）

`features/auth/service.rs`: `async fn verify_password(...)` → `pub(crate) async fn verify_password(...)`。**これ以外の既存スライス（articles/annotations/feeds/folders/shared/auth.rs/error.rs）は一切変更しない。**

### 5.10 shared/config.rs への追加

```rust
pub sync_api_enabled: bool,   // SYNC_API_ENABLED（既定 false。"1"|"true"|"yes"）
```

`docker-compose.yml` に環境変数パススルー 1 行、`.env.example` に記載（§11）。

---

## 6. フロントエンド設計

最小限。protected 側トークン管理 API に対応する UI のみ:

- `lib/api.ts` に 2 メソッド: `listSyncTokens(): Promise<SyncTokenInfo[]>`（`{id, label, created_at, last_used_at}`）、`revokeSyncToken(id: string)`。
- Settings 画面に「同期クライアント」セクション（auth の「セッション一覧」UI と同型: 一覧 + 失効ボタン + 削除確認 Dialog）。接続手順の説明文を添える: アカウント種別 **FreshRSS** / URL `http://<host>:8081` / ユーザー名は任意（識別ラベルとして表示される）/ パスワード = ログインパスワード。

---

## 7. API 契約

全読み取りは `output=` を受理して無視し、常に JSON。XML は返さない。タイムスタンプ規約の総覧:

| フィールド / パラメータ | JSON 型 | 単位 | ソース |
|---|---|---|---|
| `published` / item `updated` | 数値 | 秒 | `published_at`（NULL は `created_at`） |
| `crawlTimeMsec` | **文字列** | ミリ秒 | `created_at` |
| `timestampUsec` / `newestItemTimestampUsec` | **文字列** | マイクロ秒 | `created_at` / `max(created_at)` |
| envelope `updated` | 数値 | 秒 | `now()` |
| `ot` / `nt`（入力） | — | **秒** | `created_at` と比較（クロール時刻意味論 — バックデート記事でも増分同期が漏れない） |
| `ts`（mark-all、入力） | — | 16 桁以上→usec / 未満→秒 | `created_at <=` |

### 7.1 POST /accounts/ClientLogin — トークン発行

**POST のみ**（GET 不可。`Passwd` がクエリ文字列 = nginx / Cloudflare アクセスログへの資格情報漏えいになるため。対象 4 クライアントに GET を使うものは存在しない）。form: `Email=<任意>&Passwd=<パスワード>`（`accountType`/`service`/`client`/`output` は無視）。`Email` は照合せず `sync_tokens.label` に保存。

成功 200（`text/plain; charset=UTF-8`）:
```
SID=<token>
LSID=null
Auth=<token>
```
`SID` = `Auth`（同値）、`LSID=null` はリテラル（Vienna 対策の FreshRSS 前例）。token は 43 字 base64url（`=` を含まない）。失敗 403 `Error=BadAuthentication\n`。レート制限 429（`login_limiter` 共有）+ `Retry-After`。

### 7.2 GET /reader/api/0/token

200 `text/plain`、ボディ = 提示された auth トークン + `\n`（Miniflux 方式「edit token = auth token」）。57 字 Z パディングはしない（`T=` を検証しないため長さ問題は発生しない。§11）。

### 7.3 GET /reader/api/0/user-info

```json
{"userId":"1","userName":"reader","userProfileId":"1","userEmail":"reader"}
```
全値**文字列**。`userId:"1"` を返す以上、入力側で `user/1/...` を `user/-/...` と等価受理する（§5.1 StreamId）。

### 7.4 GET /reader/api/0/tag/list

```json
{"tags":[
  {"id":"user/-/state/com.google/starred"},
  {"id":"user/-/label/Tech","type":"folder"}
]}
```
**フォルダ 0 件でも starred 行は必ず返す**（空配列で choke するクライアント対策・両リファレンス前例）。`read`/`kept-unread` は載せない。

### 7.5 GET /reader/api/0/subscription/list

```json
{"subscriptions":[{
  "id":"feed/0197a3c2-....",
  "title":"Example Feed",
  "categories":[{"id":"user/-/label/Tech","label":"Tech"}],
  "url":"https://example.org/feed.xml",
  "htmlUrl":"https://example.org/feed.xml"
}]}
```
`url` は**必ず**出す。`categories[].id/label` は**両方必須**。フォルダ未所属は `[]`。title NULL は url で代用。`iconUrl` は省略。

### 7.6 GET /reader/api/0/stream/items/ids

パラメータ: `s`（StreamId、省略時 reading-list）/ `n`（**1..=1000 に clamp、既定 20**）/ `xt=user/-/state/com.google/read` / `ot`,`nt`（秒）/ `r=o`（昇順。`r=d`/`r=n`/欠落は降順）/ `c`（continuation）。`it`/`ck`/`client` は受理して無視。

```json
{"itemRefs":[{"id":"1523"},{"id":"1522"}],"continuation":"1522"}
```
- `id` は**符号付き 10 進の文字列**。
- **n+1 フェッチ**: n+1 件目が存在した時だけ `continuation`（そのページ最後に返した short_id の 10 進）を出す。最終ページでは**キー自体を省略**。空 `itemRefs` に continuation を付けない（NNW 無限ループの構造的排除）。
- `muted_at IS NOT NULL` は全ストリームから除外（UI パリティ）。

### 7.7 POST /reader/api/0/stream/items/contents

form: `i=<4 形式いずれか>`（反復。**1000 件で clamp — 超過分は先頭 1000 件を処理し 400 にしない**）。`T` 不問。

```json
{"id":"user/-/state/com.google/reading-list","updated":1751856000,
 "items":[{
   "id":"tag:google.com,2005:reader/item/00000000000005f3",
   "crawlTimeMsec":"1751856000123",
   "timestampUsec":"1751856000123456",
   "published":1751850000,
   "title":"…","author":"…",
   "canonical":[{"href":"https://example.org/post"}],
   "alternate":[{"href":"https://example.org/post"}],
   "categories":["user/-/state/com.google/reading-list","user/-/label/Tech",
                 "user/-/state/com.google/read","user/-/state/com.google/starred"],
   "origin":{"streamId":"feed/0197a3c2-....","title":"Example Feed","htmlUrl":"https://example.org/feed.xml"},
   "summary":{"content":"<p>…</p>"},
   "content":{"content":"<p>…</p>"}
 }]}
```
要点: envelope `id`/`updated` は NNW decoder 必須。本文は `content` 列を **500,000 バイトで UTF-8 char 境界打ち切り**して `summary.content` と `content.content` の**両方**に同一掲載。`canonical` と `alternate` を**両方**（`alternate` に `type` は付けない）。`origin.streamId` は subscription/list の `id` と厳密一致。存在しない・負値・stale な `i` は**黙って除外**（エラーにしない）。

### 7.8 GET /reader/api/0/stream/contents（bare / ワイルドカード / ?s= 変種）

- bare（パスなし）= reading-list 既定（**Fluent Reader はこれしか呼ばない**）。
- `/stream/contents/{*stream}`: percent-decode 後に StreamId パース（`feed/<uuid>`・`user/-/state/...`・`user/-/label/<name>`）。
- `?s=` クエリ変種も受理（BazQux 互換、3 行）。
- レスポンスは §7.7 と同じ envelope + `continuation`（§7.6 と同じ n+1 規則）。

### 7.9 POST /reader/api/0/edit-tag

form: `i=`（反復）、`a=`/`r=`（反復）。意味論:

| タグ | `a=` | `r=` |
|---|---|---|
| `state/com.google/read` | 既読化 | 未読化 |
| `state/com.google/kept-unread` | 未読化 | 既読化 |
| `state/com.google/starred` | スター | スター解除 |
| `broadcast` / `like` / `label/*` | 受理して無視 | 同左 |

未知 ID が混じっても 200 `OK`。

### 7.10 POST /reader/api/0/mark-all-as-read

form: `s=`（StreamId: reading-list / feed / label）、`ts`（桁数ヒューリスティック、欠落は now）。`created_at <= ts AND muted_at IS NULL` の範囲を既読化 → 200 `OK`。**catch-all の `[]` で代替してはならない**（書き込みに非 `OK` を返すとクライアントが永久リトライする）。

### 7.11 POST /reader/api/0/subscription/quickadd

form: `quickadd=<url>`（`feed/` prefix 付きも受理）。

```json
{"numResults":1,"query":"https://example.org/feed.xml","streamId":"feed/<uuid>","streamName":"https://example.org/feed.xml"}
```
`streamId` は subscription/list と**厳密一致**（NNW が直後に list を引いて照合する — 生命線）。初回フェッチは背景化されているため `streamName` はタイトル未確定時 URL（次回同期で治る）。URL 不能は 200 `{"numResults":0}`。

### 7.12 POST /reader/api/0/subscription/edit

form: `ac=edit|unsubscribe|subscribe`、`s=`（反復可）、`t=`、`a=`、`r=`。挙動は §5.4。**`r=user/-/label/X` 単独（`a=` なし）は未分類へ移動**（NNW #3512 対応必須）。200 `OK`。

### 7.13 POST /reader/api/0/rename-tag / 7.14 disable-tag

`s=user/-/label/Old&dest=user/-/label/New` / `s=user/-/label/Name`（反復可）。対象フォルダが無くても 200 `OK`（no-op）。disable-tag は folders::delete → FK で所属フィード未分類化。

### 7.15 GET /reader/api/0/unread-count（完全形 — 実装時に形を発明しないこと）

```json
{"max": 47,
 "unreadcounts": [
   {"id":"feed/0197a3c2-....","count":5,"newestItemTimestampUsec":"1751856000123456"},
   {"id":"feed/0197a3c2-....","count":12,"newestItemTimestampUsec":"1751855000123456"},
   {"id":"user/-/label/Tech","count":17,"newestItemTimestampUsec":"1751856000123456"},
   {"id":"user/-/state/com.google/reading-list","count":47,"newestItemTimestampUsec":"1751856000123456"}
 ]}
```
- **トップレベル `max`（数値）= 未読合計**（= reading-list 行の count と同値）。
- 行構成: **未読を持つフィードごとに 1 行**（`feed/<uuid>`）+ **未読を持つフォルダごとに 1 行**（`user/-/label/<name>` = 所属フィードの合計、`newest` は最大値）+ **`user/-/state/com.google/reading-list` の合計 1 行**。
- 各行 `count` は**数値**、`newestItemTimestampUsec` は**文字列・マイクロ秒**。
- muted 記事は除外。1000 キャップはしない（Reeder は生値で動く。§11）。

### 7.16 catch-all ANY /reader/api/0/{*rest}

**認証ミドルウェアの内側**（未認証の未知パスは 401 — §9.2 でテスト固定）。認証済みなら `tracing::warn!` でパスをログしてから 200 `[]`（プローブと実装漏れを黙って飲み込まない）。

---

## 8. 依存関係

- **依存する機能（すべて実装済み）**: 14 認証（Argon2id + Cookie セッション、migration 0022。旧スタブの「AUTH_TOKEN 流用」は陳腐化 — AUTH_TOKEN は全廃済みで、本設計は `sync_tokens` テーブルで置換）、01 フィード管理、02 フォルダ、18 スター（`article_stars`）。
- **ブロックする機能 / ブロックされる機能**: なし。
- **並行作業との調整**: ①migration 0023 は usage-tracking が消費済み（未コミット）→ 採番再確認必須 ②`features/mod.rs`・`shared/config.rs` は usage セッションが編集中 → マージ時に現物と合成 ③GReader トラフィックは usage 計測対象（決定済み — §5.6/5.7 参照）。

---

## 9. テスト計画（TDD — Red を先に書く）

### 9.1 純関数単体テスト（domain.rs / wire.rs 内 `#[cfg(test)]`、DB/HTTP 不要）

| テスト | 意図（対応クライアント） |
|---|---|
| `ItemId::parse` — long form padded / **unpadded**（`.../item/2f2`）/ bare 16-hex / 10 進 / **MSB 立ち 16-hex（u64 経由で失敗しない）** / **負 10 進 → None（黙って落ちる）** / ゴミ → None | NNW・Reeder・Liferea・Fluent の 4 実送信形式 + 頑健性 |
| `long_form` ゼロ埋め 16 桁小文字・`parse(long_form(x)) == x` の往復 property（境界 1, 2^62） | 可逆性 |
| `StreamId::parse` — `user/-/` と `user/1/` の等価・`feed/<uuid>`・`feed/<url>`・label の UTF-8/スペース/percent-encode（**一回だけデコード**）・broadcast→Ignored | 入力寛容性 |
| `parse_ts_param` — 10 桁秒 / 16 桁 usec / 境界 15・16 桁 / 欠落→now | Reeder（usec 送信） |
| `epoch_msec_str`/`epoch_usec_str` が文字列・`published` が数値になる wire snapshot | 型・単位の罠 |
| `paginate` — ちょうど n 件→continuation なし / n+1 件→あり（値=最後に返す要素）/ 0 件→必ずなし | NNW 無限ループ防止 |
| `plan_edits` — read/kept-unread 反転・starred・Label/Ignored 無視 | edit-tag 意味論 |
| `Params` — `i=a&i=b&a=x` の反復キー保持・query+body マージ・last-wins にならない | GReader form の要 |
| `truncate_content` — 500KB 境界がマルチバイト文字の途中でも panic しない | UTF-8 安全性 |
| `parse_google_login_header` — 正常 / scheme 違い / `AUTH=` 大文字 / 空 | ヘッダ厳密性 |
| wire snapshot — §7.5/7.7/7.15 の JSON を `serde_json::to_value` で厳密一致（`continuation` 省略時にキー自体が消える・`categories[].label` 非 Optional 含む） | クライアント decoder への防衛 |

### 9.2 ルータテスト（connect_lazy プール + `AppConfig::for_test()` + oneshot、DB 不要）

- `Authorization` なし GET `/reader/api/0/tag/list` → 401 + **両綴り Bad-Token ヘッダ** + text/plain。
- **未認証で未知パス**（`/reader/api/0/whatever`）→ **401**（catch-all がミドルウェアの内側にある証明。200 `[]` になったら退行）。
- 認証不能でも形式不正トークン → 401。スキーム違い（`Bearer x`）→ 401。
- `SYNC_API_ENABLED=false`（for_test 既定）で `/reader/api/0/*`・`/accounts/ClientLogin` が 404。
- `GET /accounts/ClientLogin` → 405（POST のみ）。
- **Cookie 不変条件**: 有効そうなセッション Cookie を付けた `/reader/api/0/*` → 401（sync は Cookie を読まない）。`Authorization: GoogleLogin auth=x` を付けた `/api/feeds` → 401（GoogleLogin は /api/* で無効）。
- ClientLogin ボディ欠落 → 403 `Error=BadAuthentication`。

### 9.3 実 DB 統合テスト（`#[ignore = "requires DATABASE_URL"]`、`DB_PORT=15432` override 慣行、UUID 入り URL で自己清掃）

- **migration 検証**: 既存行投入 → migrate → `short_id` が `created_at` 順で単調・UNIQUE・新規 INSERT（`upsert_batch` 経由）で DEFAULT 採番が続くこと（articles スライス無変更の裏取り）。
- **keyset**: n=2 で 5 件を 3 ページ・重複/欠落なし・末尾 continuation なし・`r=o` 昇順・`ot` 境界・unread/starred/feed/folder フィルタ・muted 除外。
- **既読パリティ**: `set_read_by_short_ids` と `articles::repository::set_read` が同一行を同一状態にすること（★等価性コメントの実行時保証）。
- edit-tag 往復: read→unread→read / star→unstar（annotations 経由）/ 存在しない short_id 混在で Err にならない。
- ClientLogin → `sync_tokens` ハッシュ保存 → `find_token` 往復 → **`prune_tokens_for_label` で同一 label 11 個目の発行時に最古が消える**。
- `mark_all_read` の ts 境界（秒/usec 両方）・muted 非既読化。
- quickadd → `list_subscriptions` の streamId 一致（NNW 照合の再現）。
- `unread_counts` → §7.15 の合成（feed 行 + folder 行 + 合計行 + `max`）。

### 9.4 HTTP スモーク（`scripts/test/api-greader.sh`、`api-auth.sh` 準拠、`BASE=http://localhost:8081`）

`SYNC_API_ENABLED=true` で: ClientLogin（正常 / 誤パスワード 403 / **GET → 405**）→ `Auth=` 抽出 → token → user-info → tag/list → subscription/list → quickadd → ids（`xt=read`）→ contents（返った長形式 id をそのまま `i=` に）→ edit-tag（**応答が literal `OK` かつ text/plain であることを assert**）→ ids 再取得で減少 → `GET /api/articles?unread=true`（Cookie ログイン側）でも減少 = **UI パリティ**→ unread-count → mark-all-as-read → 401 系（ヘッダなし・両 Bad-Token ヘッダ確認・未知パス 401）。`run-all.sh` に追加。

### 9.5 実クライアント検証（手動）

1. **NetNewsWire**（主対象・合格基準）: アカウント追加 → 種別 **FreshRSS** → URL `http://<LAN IP>:8081` / ユーザー名任意 / パスワード = ログインパスワード。購読・フォルダ・未読・スターの一致 → NNW 側既読/スター → Web UI 反映 → 逆方向 → フィード追加(quickadd)・改名・**フォルダ移動・フォルダから外す**・購読解除 → フォルダ改名・削除。**「変更がリフレッシュ後に巻き戻る」場合は subscription/edit 系の失敗**（NNW はエラーを黙殺する）。
2. **Reeder**（副対象）: 同手順 + **`feed/<uuid>` 非数値 streamId の受容を最初に確認**（§11 の要検証事項）+ バッジ（unread-count）+ マークオール（mark-all-as-read）。
3. Fluent Reader（任意）: bare stream/contents・T なし POST。

---

## 10. 実装手順（順序付きチェックリスト）

1. **`ls backend/migrations/` を実行し最小空き番号を確認**（0023 は usage が消費済み。本書は 0024 仮番）→ `0024_greader_sync.sql` 作成 → 実 DB backfill テスト（§9.3 先頭）Red→Green。
2. `Cargo.toml` に `form_urlencoded = "1"` 追加。
3. `domain.rs`: §9.1 のテストを **Red で先に**書き、ItemId / StreamId / epoch / paginate / plan_edits / SyncToken / truncate_content を実装。
4. `wire.rs`: serde snapshot テスト Red → 構造体 + Params + レスポンスヘルパ。
5. `features/auth/service.rs::verify_password` を `pub(crate)` 化（1 語）。
6. `repository.rs`: 射影 + StreamFilter + 書き込み + sync_tokens。§9.3 ignored テスト Red→Green（パリティテスト含む）。
7. `service.rs`: client_login（limiter 共有・prune 込み）・edit_tag・quick_add・subscription_edit・mark_all_as_read・unread_count_payload。
8. `handler.rs`: require_sync_auth + 16 ハンドラ + to_sync_response + 管理 2 ハンドラ。§9.2 ルータテスト Red→Green。
9. `mod.rs` + `features/mod.rs`（**現物確認の上で** public 条件 merge + protected 1 行）+ `shared/config.rs` に `sync_api_enabled` + compose env パススルー。
10. `just lint` → `cargo test` 全緑 → ignored テスト実 DB 実行。
11. nginx 追加（下記）+ `scripts/test/api-greader.sh` + dev 検証（Vite proxy に `/reader`・`/accounts` を追加するか backend :8080 直叩き）。
12. フロント: `lib/api.ts` 2 メソッド + Settings「同期クライアント」セクション（tsc/vitest 緑）。
13. NNW 実機検証（§9.5-1）→ Reeder（§9.5-2、streamId 受容を最初に）。
14. ドキュメント: `docs/design/README.md` 更新（行名 / スタブ注記 / 依存注記）、README.md に接続手順 + セキュリティ注意、`.env.example` に `SYNC_API_ENABLED`。
15. コミット（ユーザーの明示指示後）。

### nginx 変更（手順 11 の詳細 — `frontend/nginx.conf`）

```nginx
# GReader 互換 API（これがないと SPA フォールバックが index.html を返す）
location = /accounts/ClientLogin {          # ★完全一致 — 他の /accounts/* はプロキシしない
    resolver 127.0.0.11 valid=10s ipv6=off; # 既存 gotcha: 静的 proxy_pass は backend 再作成で 502
    set $backend_upstream http://backend:8080;
    proxy_pass $backend_upstream;
    proxy_set_header Host $http_host;       # 既存 gotcha: $host はポート落ちで Origin==Host 検査を壊す
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_read_timeout 120s;
    client_max_body_size 64k;               # Email+Passwd のみ
}
location /reader/ {
    resolver 127.0.0.11 valid=10s ipv6=off;
    set $backend_upstream http://backend:8080;
    proxy_pass $backend_upstream;
    proxy_set_header Host $http_host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_read_timeout 120s;
    client_max_body_size 8m;                 # ★/api/ の 256m（認証済みバックアップ用）を継承しない。
                                              # nginx はボディを先に受信バッファするため、無認証到達
                                              # パスに 256m は DoS ベクタ。1000 id バッチは ~60KB で 8m は十分
}
```

`docker-compose*.yml` のポート変更なし（ingress は nginx のみ、backend 非公開のまま）。

---

## 11. リスク・未決事項・代替案

- **【要確認・最重要】Reeder が `feed/<uuid>` を数値と仮定する可能性**: NNW/Fluent は文字列扱いで安全（コード確認済み）、Google 本家も `feed/<url>` だったため仕様上は不透明文字列。Reeder のみ内部実装未読解 → §9.5-2 で**最初に**検証。**退路（準備済み）**: 問題が出たら `feeds.short_id BIGINT` を後続 migration（0024 と同型 backfill）で追加し `StreamId::feed_output` だけ差し替える。ワイヤ層が StreamId に集約されているため影響は局所。
- **item id は永続不変が前提**: `short_id` は UNIQUE・再採番禁止。feed 削除 CASCADE での消滅は正常（クライアントは未知 id を無視）。バックアップ復元で採番が変わると全クライアント再同期（実害は再ダウンロードのみ）。
- **外部公開方針（決定済み 2026-07-07）= Cloudflare Access を前置して公開**。`login_limiter` は per-IP でなくグローバルのため、素で公開すると攻撃者の ClientLogin 連打で**正規ユーザーの Web UI ログインまで一時ロックされる** — Access 前置はこのロックアウト DoS への防壁を兼ねる。**注意**: GReader クライアント（NNW/Reeder）は対話 SSO も任意ヘッダ付与もできないため、`/accounts`・`/reader` に掛ける Access ポリシーは非対話手段（Service Token / mTLS / WARP デバイスポスチャ / IP 許可等）で構成する必要がある — 具体構成は運用課題（本設計のスコープ外）。README には「公開時は Cloudflare Access 前置を推奨・非対話ポリシーが必要」と明記する。
- **トークンは無期限**: GReader クライアントは再ログインを想定しないため意図的。緩和策 = `SYNC_API_ENABLED` 既定 off + 管理 UI での一覧/失効 + 同一 label 10 件 prune + README に「公開時は HTTPS（CF Tunnel は満たす）必須・失効方法」を記載。
- **書き込みの `OK` 契約**: 失敗時に JSON / 204 を返すとクライアントが無限リトライ。handler 境界の整形を `AppError::IntoResponse` で迂回しないこと（レビュー観点）。
- **NNW は subscription/edit の失敗を黙殺**: バグると「操作が巻き戻る」症状。§9.5-1 で全操作を必ず通す。
- **catch-all がバグを隠す**: `tracing::warn!` 必須（実装漏れ・プローブの検知手段）。認証内側なので未認証プローブは 401 で見える。
- **既読一括 UPDATE の意味論追随義務**: articles 側の既読化に副作用が付く変更が入ったら sync 側も追随（コード内コメント + §9.3 パリティテストで防護）。
- **mark_all_read の muted 差異**: GReader 側は `muted_at IS NULL` 条件付き（UI の既存 `mark_all_read` は無条件）。配信していないものを既読化しないのが整合的 — 意図的差異としてコメント明記。
- **`ot` の基準時刻**: `created_at`（クロール時刻）。`published_at` ではない（過去日付記事の増分同期漏れ防止）。
- **OPML インポート（NNW 経由）が無反応**: catch-all の 200 で NNW が成功と誤認しうる。頻度極小でカット。問題視されたら `subscription/import` を実装（設計 17 のパーサ流用）。
- **/token の 57 字問題**: 長さ検証するクライアントが実在しても `T=` 無検証のため機能被害なし。必要なら Z パディング 1 行で追随。
- **unread-count の 1000 キャップ**: Google 純正はキャップしていたが、Reeder は生値で動くためしない。問題が出たらキャップ追加。
- **`full_content` 配信**: 現状 UI と同じ `content` を配信。`SYNC_SERVE_FULL_CONTENT` オプションは将来課題。
- **5xx の形**: text/plain の generic 500 に統一（クライアントは一時障害としてリトライ。内部情報非開示）。
- **規約遵守サマリ**: 新規 1 スライス（5+1 ファイル、wire.rs は digest/email.rs 前例）+ features/mod.rs merge 2 行 + config 1 フィールド + `verify_password` 可視性 1 語のみ。trait 追加なし・runtime クエリのみ・migration 追記のみ・AppError バリアント追加なし・新規 crate は `form_urlencoded` のみ。
