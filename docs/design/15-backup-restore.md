# 15 バックアップ / 復元

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッション・effort の低い実装者。本書1ファイルだけで着手・完了できるよう、再利用資産・完全な SQL・関数シグネチャ・サービス手順・API 契約・フロント変更・TDD・番号付き実装手順・リスク表まで具体化する。
> **重要な但し書き**: 本機能の核（エクスポート/インポート）は **既存テーブルへの読み書きのみ**で、スキーマ変更を必須としない。新マイグレーション `0006_backup.sql` は **任意の pg_dump スケジューラの監査ログ（`backup_runs`）専用**であり、核機能には不要。マイグレーション番号は **着手前に必ず `ls backend/migrations/` で最新番号を確認**すること（本書執筆時点の最新は `0005_search.sql`。よって暫定採番は `0006`）。

---

## 1. 概要

セルフホスト型 RSS リーダーの **全データを JSON でエクスポート / インポート** できるようにする。対象は `folders` / `feeds` / `articles` と、その派生データ（**LLM キャッシュ = `summary` / `translation`（＋ `_lang`）**、**既読状態 `is_read`**、**「後で読む」状態 `read_later_items`**）。

- **エクスポート**: `GET /api/backup/export` が全レコードを **NDJSON（改行区切り JSON）でストリーム配信**する。1行1レコードなので、巨大データでもサーバ・クライアント双方がメモリに全展開せずに処理できる。
- **インポート**: `POST /api/backup/import` が同じ NDJSON を受け取り、**自然キー（feed/article は `url`、folder/read_later は id）で冪等マージ**する。同じファイルを2回流しても結果が変わらない（重複行・トークン再消費を起こさない）。
- **任意の pg_dump スケジュール**: env で有効化したときだけ、`shared/scheduler.rs` と同型の `tokio::interval` で定期的に `pg_dump` をファイルへ吐き、結果を `backup_runs` テーブルに記録する（核機能とは独立。無効時は何もしない）。

**意図（2つの「トークン防衛」）**:
1. **データ所有権**: 自己ホストの全データをいつでも手元へ吸い出し、別インスタンスへ復元できる（ベンダーロックインの回避）。
2. **API トークン防衛（LLM 課金）**: `articles.summary` / `translation` は **オンデマンドで Claude を呼んで得たキャッシュ**（`articles/service.rs`）。バックアップがこれを**保全・復元**することで、再構築時に Claude を呼び直してトークンを再消費する事故を防ぐ。**本機能自体は LLM を一切呼ばない**（§8 でこの方針を明記）。
3. **エンドポイント・トークン防衛（秘匿）**: エクスポートは全データを露出するため、**`BACKUP_TOKEN` env を設定したときだけ有効化**し、Bearer トークン照合を通す。未設定なら任意機能の慣例どおり `AppError::NotEnabled`（503）を返す（要約/翻訳が `ANTHROPIC_API_KEY` 未設定時に取る挙動と同型）。

本機能はバックエンドに **新スライス `backup` を1枚**追加し、`features/mod.rs` に `.merge(backup::routes())` を1行足すだけで成立する（既存スライスは触らない）。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）
- 新スライス `backend/src/features/backup/`（`domain` / `repository` / `service` / `handler` / `mod`）。
- `GET /api/backup/export` … 全データを **NDJSON ストリーム**で返す（`Content-Type: application/x-ndjson`、`Content-Disposition: attachment`）。`folders → feeds → articles → read_later` の **FK 依存順**で行を吐く。
- `POST /api/backup/import` … NDJSON を**1トランザクション内で冪等マージ**。レコード種別ごとに upsert。集計サマリ JSON（各 kind の件数）を返す。
- **`BACKUP_TOKEN` env によるエンドポイント保護**: 未設定なら `NotEnabled`（503、機能ゲート）、設定済みで照合失敗なら `Validation`（400）。`config.rs` に `backup_token: Option<String>` を追加。
- 任意 pg_dump スケジューラ（**env で有効化したときのみ**）: `BACKUP_DIR` + `BACKUP_PGDUMP_INTERVAL_SECS` を設定したときだけ `tokio::interval` で `pg_dump` を実行し `backup_runs` に記録。`GET /api/backup/runs` で直近の実行履歴を返す。
- マイグレーション **`0006_backup.sql`**（暫定採番。pg_dump 監査用 `backup_runs` テーブルのみ。**核機能には不要**）。
- フロント `/settings`（`routes/Settings.tsx`）に **「バックアップ / 復元」Card** を1枚追加（エクスポートのダウンロード・ファイル選択でインポート・トークン入力欄）。
- `lib/api.ts` に **3 メソッド**（`exportBackup` / `importBackup` / `listBackupRuns`）とインポート結果型 `ImportSummary` / 実行履歴型 `BackupRun`。
- ドメイン純粋ロジック（NDJSON 1行のパース・kind 判定・自然キー抽出）の単体テスト、リポジトリ往復の `#[ignore]` 実 DB テスト、HTTP スモークスクリプト。

### 非スコープ（本機能では実装しない）
- **`instapaper_credentials` のエクスポート/インポート**（パスワード平文の秘匿情報。バックアップに含めない。§4.3）。
- **タグのエクスポート**: タグ機能は現状未実装（`tags` テーブルは存在しない）。将来タグスライスが入ったら、本スライスに kind `"tag"` を**追記**するだけで拡張できる（§11 にフォワード互換メモ）。
- 暗号化バックアップ（at-rest 暗号化）。家庭内 LAN・単一ユーザー前提の MVP では平文 NDJSON。`BACKUP_TOKEN` で転送経路（HTTP）を保護するに留める。
- 増分/差分バックアップ。MVP は毎回フルダンプ（冪等マージなので何度流しても安全）。
- 世代管理・自動ローテーション（pg_dump ファイルの保持本数管理）。スケジューラは「吐くだけ」。ローテーションは将来。
- 認可の細粒度化（複数ユーザー・ロール）。単一ユーザー前提。

---

## 3. 既存実装の調査と再利用

実ファイルを確認済み。以下を **再利用し、車輪の再発明をしない**。

| 再利用資産 | 実体（確認済みファイル） | 本機能での使い方 |
|---|---|---|
| `AppState { db, config, http }` | `backend/src/shared/state.rs`（`#[derive(Clone)]`、`db: PgPool`） | `state.db` をエクスポート/インポートの `query`/`query_as` に渡す。新規プールは作らない |
| `AppError` 6 バリアント | `backend/src/shared/error.rs`（`NotFound`/404, `Validation(String)`/400, `NotEnabled(String)`/503, `Upstream(String)`/502, `Database(#[from] sqlx::Error)`/500, `Other(#[from] anyhow::Error)`/500。`IntoResponse` で `Json({"error": <Display>})`） | 新バリアントを足さず既存で表現（§5.7）。**`error.rs` は編集しない** |
| 任意機能 = `NotEnabled` パターン | `articles/service.rs::llm_client()` が `anthropic_api_key` 無し時に `NotEnabled("ANTHROPIC_API_KEY is not set")` | `BACKUP_TOKEN` 未設定時に同型で `NotEnabled("BACKUP_TOKEN is not set")` を返す |
| sqlx ランタイムクエリ + upsert | `articles/repository.rs`（`INSERT ... ON CONFLICT (url) DO UPDATE`、`query_as::<_,T>`、`fetch_optional`）、`instapaper/repository.rs`（`ON CONFLICT (id) DO UPDATE`、`get_article_ref` の素 Uuid bind） | エクスポートは `fetch_all`／インポートは `ON CONFLICT (...) DO UPDATE ... RETURNING id`。**`query!` コンパイル時マクロは使わない** |
| 既存ドメイン型（読み出し用） | `articles/domain.rs::Article`、`feeds/domain.rs::Feed`、`folders/domain.rs::Folder`、`instapaper/domain.rs::ReadLaterItem`（`sqlx::FromRow` 付き、列構成は §4 と一致） | エクスポートの `query_as` のターゲットにそのまま流用（再 struct 定義しない） |
| 定期タスクの spawn パターン | `backend/src/shared/scheduler.rs`（`tokio::spawn` + `interval` + `MissedTickBehavior::Skip`、最初の tick を捨てる、`tracing::error!` でログ） | pg_dump スケジューラを同型で `service::spawn_pgdump_scheduler(state)` として書く。`main.rs` で条件付き spawn |
| 設定の env 読み出し | `backend/src/shared/config.rs`（`AppConfig`、`std::env::var(...).ok()` で Option、`feed_refresh_interval_secs` の `.unwrap_or(900)`） | `backup_token` / `backup_dir` / `backup_pgdump_interval_secs` を同型で追加 |
| スライス構成 + `routes()` | `instapaper/mod.rs`・`search/mod.rs`（`domain/repository/service/handler/mod`、`fn routes() -> Router<AppState>`、`.route("/path", get(...).post(...))`） | 同じ5ファイル構成で `backup` を作る |
| `features/mod.rs` の合成 | `pub mod ...;` + `.merge(...::routes())`（既存8スライス） | `pub mod backup;` と `.merge(backup::routes())` を1行ずつ追加。既存スライスは触らない |
| ストリーミング応答 | axum 0.8 の `axum::body::Body::from_stream`（`futures` の `Stream<Item = Result<Bytes, _>>` をそのまま body にできる） | NDJSON を `try_stream!`（async-stream）または `Body::from_stream` でチャンク配信。**§11 に「全件メモリ展開フォールバック」も記載** |
| フロント API クライアント | `frontend/src/lib/api.ts`（`http<T>()` は 204→`undefined` 畳み込み済み。`api` オブジェクトに `動詞+リソース` 命名で集約） | エクスポート/インポートは Blob・FormData を扱うため `http<T>()` ではなく **専用 fetch ラッパ**を api に足す（§6.1） |
| 自前 UI 部品 | `frontend/src/components/ui/{button,card,input}.tsx`（`cn(@/lib/utils)`、`cva`） | 「バックアップ」Card を `card.tsx` + `button.tsx` + `input.tsx` で構成。新規部品は作らない |
| `/settings` ルートと Card 追記の慣習 | 機能 05 が `routes/Settings.tsx` を新設し「先着が骨格・後着は Card を足すだけ」と規定 | Instapaper Card と並べて **「バックアップ / 復元」Card を1枚追記**（非干渉に共存） |
| HTTP スモークテストの慣習 | `scripts/test/api-stats.sh`・`api-instapaper.sh`（稼働スタックに curl、HTTP コードと JSON キーを assert） | `scripts/test/api-backup.sh` を同型で新設（§9.3） |

> **依存追加の要否（確認済み）**: NDJSON のストリーミングに `async-stream`（`try_stream!`）を使うと簡潔。`backend/Cargo.toml` に未導入なら `async-stream = "0.3"` を1行足す（軽量・依存少）。`futures` は `tokio`/`sqlx` 経由で既に入っている。**ストリーム化を避ける場合は依存追加ゼロの「全件メモリ展開」フォールバック**（§11）で実装してよい。`serde_json` は既存依存（`json!` マクロを `anthropic.rs` が使用）。pg_dump スケジューラは外部コマンド `pg_dump` を `tokio::process::Command` で呼ぶだけ（新依存不要。`tokio` は既存）。

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方

`main.rs` 起動時の `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を呼ばないため、**適用済み最大バージョンより小さい未適用マイグレーションを後から発見すると `VersionMissing`（out-of-order）でエラーになり起動が壊れる**（家庭内サーバの永続 DB で実害）。

**ルール（必ず守る）**:
- **着手前に必ず `ls backend/migrations/` で最新番号を確認**し、`最大番号 + 1`（＝最小空き整数）を採番する。本書執筆時点の最新は `0005_search.sql` なので暫定で **`0006_backup.sql`**。並行開発中の他機能（apalis 等）が先に `0006` を取っていれば `0007` 以降へ繰り上げる。
- 既存マイグレーション（`0001`〜`0005`）は**編集しない**（追記のみ）。
- **核機能（export/import）はスキーマ変更不要**。`0006_backup.sql` は **pg_dump スケジューラを使う場合だけ意味を持つ**監査テーブル。スケジューラを使わないなら、このマイグレーションは「空でない CREATE だが副作用は監査行のみ」として置いておいてよい（適用しても害はない）。

### 4.2 新規マイグレーション `backend/migrations/0006_backup.sql`

```sql
-- Audit log for the OPTIONAL scheduled pg_dump task (shared/scheduler.rs と同型の
-- tokio::interval から呼ばれる)。core の export/import はこのテーブルを使わない。
-- BACKUP_DIR + BACKUP_PGDUMP_INTERVAL_SECS が未設定ならスケジューラは起動せず、
-- このテーブルは空のまま残る（実害なし）。
CREATE TABLE IF NOT EXISTS backup_runs (
    id          UUID PRIMARY KEY,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    status      TEXT NOT NULL DEFAULT 'running'
                CHECK (status IN ('running', 'succeeded', 'failed')),
    file_path   TEXT,            -- 出力先（BACKUP_DIR 配下）
    byte_size   BIGINT,          -- 生成ファイルサイズ（成功時）
    error       TEXT             -- 失敗時のメッセージ
);

-- 履歴表示（GET /api/backup/runs）は新しい順に少数件返すだけ。
CREATE INDEX IF NOT EXISTS idx_backup_runs_started_at
    ON backup_runs (started_at DESC);
```

設計判断:
- **`backup_runs` のみで、`feeds`/`articles`/`folders`/`read_later_items` への列追加は無い**。バックアップは既存テーブルを**読み書きするだけ**で、関心を既存テーブルに漏らさない。
- **status は CHECK 制約付き TEXT**（既存 `read_later_items.status` の `pending|added|failed` と同じ流儀）。
- pg_dump を使わない運用ではこのテーブルは空。`GET /api/backup/runs` は空配列を返すだけで害は無い。

### 4.3 エクスポート対象テーブルと列（読み取り対象の確定）

| テーブル | 出力する列 | 出力しない列 / 理由 |
|---|---|---|
| `folders` | `id, name, position, created_at` | 全列出力 |
| `feeds` | `id, url, title, folder_id, created_at, last_fetched_at` | 全列出力 |
| `articles` | `id, feed_id, url, title, content, published_at, is_read, summary, summary_lang, translation, translation_lang, processed_at, created_at` | **全列出力（LLM キャッシュ・既読を保全 = トークン防衛の核）** |
| `read_later_items` | `article_id, status, instapaper_added_at, last_error, created_at, updated_at` | 全列出力 |
| `instapaper_credentials` | **出力しない** | パスワード平文の秘匿情報。バックアップに含めない（§2 非スコープ） |
| `backup_runs` | **出力しない** | 運用メタ。データ本体ではない |

---

## 5. バックエンド設計

新スライス **`backend/src/features/backup/`**。5ファイル構成。

### 5.1 NDJSON フォーマット仕様（契約の中核）

エクスポート/インポートの共通フォーマット。**1行 = 1 JSON オブジェクト**（NDJSON）。各行は `kind` フィールドで種別を判別する。FK 依存順（folder → feed → article → read_later）で出力し、インポートは到着順に1パスで処理する（feed を先に upsert してから、その id 解決済みで article を処理できる）。

```
{"v":1,"kind":"meta","exported_at":"2026-06-30T12:00:00Z","app":"rss-reader"}
{"kind":"folder","id":"...","name":"Tech","position":0,"created_at":"..."}
{"kind":"feed","id":"...","url":"https://a.example/feed.xml","title":"A","folder_id":"...","created_at":"...","last_fetched_at":"..."}
{"kind":"article","id":"...","feed_id":"...","url":"https://a.example/p/1","title":"...","content":"...","published_at":"...","is_read":true,"summary":"...","summary_lang":"ja","translation":null,"translation_lang":null,"processed_at":"...","created_at":"..."}
{"kind":"read_later","article_id":"...","status":"added","instapaper_added_at":"...","last_error":null,"created_at":"...","updated_at":"..."}
```

- **1行目は必ず `meta`**（バージョン `v` とエクスポート時刻）。`v` が未知なら import は `Validation` で拒否（前方互換の番兵）。
- `folder_id` / `published_at` 等の nullable は JSON の `null`。
- 未知の `kind` は import で**スキップ**（前方互換: 将来 `"tag"` 等が増えても古いインポータが壊れない）。

### 5.2 `domain.rs`（純粋ロジック + DTO。外部 I/O なし = 単体テスト対象）

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// NDJSON 先頭行。インポート互換性チェックに使う。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMeta {
    pub v: u32,
    #[serde(default)]
    pub exported_at: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
}

/// 現在のフォーマットバージョン。import はこれより新しい v を拒否する。
pub const FORMAT_VERSION: u32 = 1;

/// 1行をパースした結果。`kind` で分岐する。未知 kind は Unknown に落とす（前方互換）。
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Meta(BackupMeta),
    Folder(FolderRow),
    Feed(FeedRow),
    Article(ArticleRow),
    ReadLater(ReadLaterRow),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct FolderRow {
    pub id: Uuid,
    pub name: String,
    pub position: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeedRow {
    pub id: Uuid,
    pub url: String,
    pub title: Option<String>,
    pub folder_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ArticleRow {
    pub id: Uuid,
    pub feed_id: Uuid,
    pub url: String,
    pub title: String,
    pub content: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_read: bool,
    pub summary: Option<String>,
    pub summary_lang: Option<String>,
    pub translation: Option<String>,
    pub translation_lang: Option<String>,
    pub processed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReadLaterRow {
    pub article_id: Uuid,
    pub status: String,
    pub instapaper_added_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// インポート集計サマリ（レスポンス body）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportSummary {
    pub folders: u64,
    pub feeds: u64,
    pub articles: u64,
    pub read_later: u64,
    pub skipped: u64, // 未知 kind / 空行
}

/// NDJSON 1行 → Record（serde_json でタグ無しに自前ディスパッチ）。
/// 空行は Ok(None)。kind 欠落や JSON 不正は Err(理由)。未知 kind は Unknown。
pub fn parse_line(line: &str) -> Result<Option<Record>, String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let v: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("invalid json line: {e}"))?;
    let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
    let rec = match kind {
        "meta" => Record::Meta(
            serde_json::from_value(v).map_err(|e| format!("bad meta: {e}"))?,
        ),
        "folder" => Record::Folder(
            serde_json::from_value(v).map_err(|e| format!("bad folder: {e}"))?,
        ),
        "feed" => {
            Record::Feed(serde_json::from_value(v).map_err(|e| format!("bad feed: {e}"))?)
        }
        "article" => Record::Article(
            serde_json::from_value(v).map_err(|e| format!("bad article: {e}"))?,
        ),
        "read_later" => Record::ReadLater(
            serde_json::from_value(v).map_err(|e| format!("bad read_later: {e}"))?,
        ),
        _ => Record::Unknown,
    };
    Ok(Some(rec))
}

/// import の互換性ゲート。FORMAT_VERSION より新しいファイルは拒否。
pub fn check_version(meta: &BackupMeta) -> Result<(), String> {
    if meta.v > FORMAT_VERSION {
        return Err(format!(
            "backup format v{} is newer than supported v{}",
            meta.v, FORMAT_VERSION
        ));
    }
    Ok(())
}

/// 1レコードを NDJSON 1行（末尾改行付き）へ。export のシリアライズ単位。
pub fn to_line(value: &serde_json::Value) -> String {
    let mut s = value.to_string();
    s.push('\n');
    s
}
```

> `FolderRow`/`FeedRow`/`ArticleRow`/`ReadLaterRow` に `sqlx::FromRow` を付けるのは、エクスポートの `query_as::<_, FeedRow>` で**そのまま行を読める**ようにするため（既存ドメイン型 `Feed` は `id: FeedId` newtype なので NDJSON の素 `Uuid` 表現と分けたい。バックアップ専用 DTO を1セット持つことで、フォーマットを既存ドメインの変更から疎結合に保つ）。

### 5.3 `repository.rs`（`&PgPool` / `&mut PgConnection` を取る free async fn、ランタイムクエリのみ）

```rust
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::domain::{ArticleRow, BackupRunRow, FeedRow, FolderRow, ReadLaterRow};
use crate::shared::error::AppResult;

// ---- エクスポート（読み取り。FK 依存順に呼ぶ） ----

pub async fn all_folders(pool: &PgPool) -> AppResult<Vec<FolderRow>> {
    let rows = sqlx::query_as::<_, FolderRow>(
        "SELECT id, name, position, created_at FROM folders ORDER BY position, created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn all_feeds(pool: &PgPool) -> AppResult<Vec<FeedRow>> {
    let rows = sqlx::query_as::<_, FeedRow>(
        "SELECT id, url, title, folder_id, created_at, last_fetched_at \
         FROM feeds ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn all_articles(pool: &PgPool) -> AppResult<Vec<ArticleRow>> {
    let rows = sqlx::query_as::<_, ArticleRow>(
        "SELECT id, feed_id, url, title, content, published_at, is_read, summary, \
                summary_lang, translation, translation_lang, processed_at, created_at \
         FROM articles ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn all_read_later(pool: &PgPool) -> AppResult<Vec<ReadLaterRow>> {
    let rows = sqlx::query_as::<_, ReadLaterRow>(
        "SELECT article_id, status, instapaper_added_at, last_error, created_at, updated_at \
         FROM read_later_items ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ---- インポート（トランザクション内 upsert。冪等） ----
// すべて &mut Transaction を取り、handler/service が1トランザクションでまとめる。

pub async fn upsert_folder(tx: &mut Transaction<'_, Postgres>, r: &FolderRow) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO folders (id, name, position, created_at)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (id) DO UPDATE
             SET name = EXCLUDED.name, position = EXCLUDED.position"#,
    )
    .bind(r.id)
    .bind(&r.name)
    .bind(r.position)
    .bind(r.created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// feeds は url が UNIQUE。url で衝突したら既存行を採用し、その実 id を返す。
/// 返った id を呼び出し側の old_feed_id -> actual_id マップに使う（article の feed_id 再マップ）。
pub async fn upsert_feed(tx: &mut Transaction<'_, Postgres>, r: &FeedRow) -> AppResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO feeds (id, url, title, folder_id, created_at, last_fetched_at)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (url) DO UPDATE
             SET title = EXCLUDED.title,
                 folder_id = COALESCE(EXCLUDED.folder_id, feeds.folder_id),
                 last_fetched_at = GREATEST(feeds.last_fetched_at, EXCLUDED.last_fetched_at)
           RETURNING id"#,
    )
    .bind(r.id)
    .bind(&r.url)
    .bind(&r.title)
    .bind(r.folder_id)
    .bind(r.created_at)
    .bind(r.last_fetched_at)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// articles も url が UNIQUE。feed_id は呼び出し側が再マップ済みの値を渡す。
/// LLM キャッシュ（summary/translation）と is_read は COALESCE で「非 null を保全」。
/// 既存に要約があり import 側が null でも消さない（トークン防衛）。逆もまた然り。
pub async fn upsert_article(
    tx: &mut Transaction<'_, Postgres>,
    r: &ArticleRow,
    mapped_feed_id: Uuid,
) -> AppResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO articles
             (id, feed_id, url, title, content, published_at, is_read,
              summary, summary_lang, translation, translation_lang, processed_at, created_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
           ON CONFLICT (url) DO UPDATE
             SET title = EXCLUDED.title,
                 content = EXCLUDED.content,
                 published_at = COALESCE(EXCLUDED.published_at, articles.published_at),
                 is_read = articles.is_read OR EXCLUDED.is_read,
                 summary = COALESCE(EXCLUDED.summary, articles.summary),
                 summary_lang = COALESCE(EXCLUDED.summary_lang, articles.summary_lang),
                 translation = COALESCE(EXCLUDED.translation, articles.translation),
                 translation_lang = COALESCE(EXCLUDED.translation_lang, articles.translation_lang),
                 processed_at = COALESCE(EXCLUDED.processed_at, articles.processed_at)
           RETURNING id"#,
    )
    .bind(r.id)
    .bind(mapped_feed_id)
    .bind(&r.url)
    .bind(&r.title)
    .bind(&r.content)
    .bind(r.published_at)
    .bind(r.is_read)
    .bind(&r.summary)
    .bind(&r.summary_lang)
    .bind(&r.translation)
    .bind(&r.translation_lang)
    .bind(r.processed_at)
    .bind(r.created_at)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// read_later は article_id が PK。記事が存在しない場合は FK 違反になるため、
/// 呼び出し側で「対応する article を import 済み」のものだけ渡す（service で再マップ）。
pub async fn upsert_read_later(
    tx: &mut Transaction<'_, Postgres>,
    r: &ReadLaterRow,
    mapped_article_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO read_later_items
             (article_id, status, instapaper_added_at, last_error, created_at, updated_at)
           VALUES ($1,$2,$3,$4,$5,$6)
           ON CONFLICT (article_id) DO UPDATE
             SET status = EXCLUDED.status,
                 instapaper_added_at = COALESCE(EXCLUDED.instapaper_added_at, read_later_items.instapaper_added_at),
                 last_error = EXCLUDED.last_error,
                 updated_at = now()"#,
    )
    .bind(mapped_article_id)
    .bind(&r.status)
    .bind(r.instapaper_added_at)
    .bind(&r.last_error)
    .bind(r.created_at)
    .bind(r.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ---- pg_dump スケジューラ用（任意） ----

pub async fn insert_run_started(pool: &PgPool, id: Uuid) -> AppResult<()> {
    sqlx::query("INSERT INTO backup_runs (id, status) VALUES ($1, 'running')")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn finish_run_ok(pool: &PgPool, id: Uuid, path: &str, bytes: i64) -> AppResult<()> {
    sqlx::query(
        "UPDATE backup_runs SET status='succeeded', finished_at=now(), file_path=$2, byte_size=$3 WHERE id=$1",
    )
    .bind(id)
    .bind(path)
    .bind(bytes)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_run_err(pool: &PgPool, id: Uuid, err: &str) -> AppResult<()> {
    sqlx::query("UPDATE backup_runs SET status='failed', finished_at=now(), error=$2 WHERE id=$1")
        .bind(id)
        .bind(err)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn recent_runs(pool: &PgPool, limit: i64) -> AppResult<Vec<BackupRunRow>> {
    let rows = sqlx::query_as::<_, BackupRunRow>(
        "SELECT id, started_at, finished_at, status, file_path, byte_size, error \
         FROM backup_runs ORDER BY started_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
```

`BackupRunRow` は `domain.rs` に追加（`id: Uuid, started_at, finished_at: Option<..>, status: String, file_path: Option<String>, byte_size: Option<i64>, error: Option<String>`、`Serialize + sqlx::FromRow`）。

> **`query!` コンパイル時マクロは使わない**（ビルドに DB 接続が要るため禁止）。すべて `query`/`query_as`/`query_scalar` のランタイムクエリ。`&mut **tx` は sqlx 0.8 でトランザクション内 executor を渡す定石。

### 5.4 `service.rs`（`&AppState` を取りエクスポート/インポート/スケジューラを統合）

```rust
use std::collections::HashMap;

use axum::body::Body;
use uuid::Uuid;

use super::domain::{
    check_version, parse_line, to_line, ImportSummary, Record, FORMAT_VERSION,
};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// BACKUP_TOKEN ゲート。未設定なら NotEnabled、設定済みで照合失敗なら Validation。
/// handler が抽出した Authorization の Bearer 値（Option）を渡す。
pub fn check_token(state: &AppState, presented: Option<&str>) -> AppResult<()> {
    let expected = state
        .config
        .backup_token
        .as_deref()
        .ok_or_else(|| AppError::NotEnabled("BACKUP_TOKEN is not set".into()))?;
    match presented {
        Some(t) if constant_time_eq(t.as_bytes(), expected.as_bytes()) => Ok(()),
        _ => Err(AppError::Validation("invalid or missing backup token".into())),
    }
}

/// タイミング攻撃を避ける単純な定数時間比較（外部 crate 不要）。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 全データを NDJSON ストリームの axum Body にして返す。
/// FK 依存順（meta → folders → feeds → articles → read_later）。
/// 注: 大規模化したら all_articles を keyset ページングに置換できる（§11）。
pub async fn export_ndjson(state: &AppState) -> AppResult<Body> {
    use serde_json::json;

    let folders = repository::all_folders(&state.db).await?;
    let feeds = repository::all_feeds(&state.db).await?;
    let articles = repository::all_articles(&state.db).await?;
    let read_later = repository::all_read_later(&state.db).await?;

    let mut buf = String::new();
    buf.push_str(&to_line(&json!({
        "v": FORMAT_VERSION,
        "kind": "meta",
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "app": "rss-reader",
    })));
    for f in &folders {
        buf.push_str(&to_line(&{ let mut v = serde_json::to_value(f)?; v["kind"] = "folder".into(); v }));
    }
    for f in &feeds {
        buf.push_str(&to_line(&{ let mut v = serde_json::to_value(f)?; v["kind"] = "feed".into(); v }));
    }
    for a in &articles {
        buf.push_str(&to_line(&{ let mut v = serde_json::to_value(a)?; v["kind"] = "article".into(); v }));
    }
    for r in &read_later {
        buf.push_str(&to_line(&{ let mut v = serde_json::to_value(r)?; v["kind"] = "read_later".into(); v }));
    }
    Ok(Body::from(buf))
}

/// NDJSON 本文（全文 String）を冪等マージ。1トランザクション。
/// id 再マップ: feed は url 衝突で実 id が変わりうるので old->actual を覚え、
/// article の feed_id を差し替える。article も同様に old->actual を覚え read_later に使う。
pub async fn import_ndjson(state: &AppState, body: &str) -> AppResult<ImportSummary> {
    let mut summary = ImportSummary::default();
    let mut feed_map: HashMap<Uuid, Uuid> = HashMap::new();
    let mut article_map: HashMap<Uuid, Uuid> = HashMap::new();
    let mut version_checked = false;

    let mut tx = state.db.begin().await?;

    for (lineno, line) in body.lines().enumerate() {
        let rec = parse_line(line)
            .map_err(|e| AppError::Validation(format!("line {}: {e}", lineno + 1)))?;
        let Some(rec) = rec else { continue };
        match rec {
            Record::Meta(m) => {
                check_version(&m).map_err(AppError::Validation)?;
                version_checked = true;
            }
            Record::Folder(f) => {
                repository::upsert_folder(&mut tx, &f).await?;
                summary.folders += 1;
            }
            Record::Feed(f) => {
                let old = f.id;
                let actual = repository::upsert_feed(&mut tx, &f).await?;
                feed_map.insert(old, actual);
                summary.feeds += 1;
            }
            Record::Article(a) => {
                // feed_id は import 済みなら再マップ、未知ならそのまま（自己整合データ想定）。
                let mapped_feed = feed_map.get(&a.feed_id).copied().unwrap_or(a.feed_id);
                let old = a.id;
                let actual = repository::upsert_article(&mut tx, &a, mapped_feed).await?;
                article_map.insert(old, actual);
                summary.articles += 1;
            }
            Record::ReadLater(r) => {
                let mapped = article_map.get(&r.article_id).copied().unwrap_or(r.article_id);
                repository::upsert_read_later(&mut tx, &r, mapped).await?;
                summary.read_later += 1;
            }
            Record::Unknown => summary.skipped += 1,
        }
    }

    if !version_checked {
        return Err(AppError::Validation(
            "missing meta header (first line must be kind=meta)".into(),
        ));
    }

    tx.commit().await?;
    Ok(summary)
}
```

pg_dump スケジューラ（任意・`main.rs` から条件付き spawn）:

```rust
use std::time::Duration;
use tokio::time::{interval, MissedTickBehavior};

/// BACKUP_DIR と BACKUP_PGDUMP_INTERVAL_SECS が両方設定されている時だけ起動。
/// shared/scheduler.rs と同型（最初の tick を捨てる / Skip / tracing でログ）。
pub fn spawn_pgdump_scheduler(state: AppState) {
    let (Some(dir), Some(secs)) = (
        state.config.backup_dir.clone(),
        state.config.backup_pgdump_interval_secs,
    ) else {
        tracing::info!("pg_dump scheduler disabled (BACKUP_DIR / interval not set)");
        return;
    };
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        ticker.tick().await; // 起動直後の即時発火を捨てる
        loop {
            ticker.tick().await;
            if let Err(e) = run_pgdump(&state, &dir).await {
                tracing::error!(error = %e, "scheduled pg_dump failed");
            }
        }
    });
}

async fn run_pgdump(state: &AppState, dir: &str) -> AppResult<()> {
    let id = Uuid::new_v4();
    let path = format!("{dir}/rss-{}.sql", chrono::Utc::now().format("%Y%m%dT%H%M%SZ"));
    repository::insert_run_started(&state.db, id).await?;

    // pg_dump は DATABASE_URL を直接受ける。出力をファイルへ。
    let out = tokio::process::Command::new("pg_dump")
        .arg(&state.config.database_url)
        .arg("-f")
        .arg(&path)
        .output()
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!("spawn pg_dump: {e}")))?;

    if out.status.success() {
        let bytes = tokio::fs::metadata(&path).await.map(|m| m.len() as i64).unwrap_or(0);
        repository::finish_run_ok(&state.db, id, &path, bytes).await?;
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        repository::finish_run_err(&state.db, id, &err).await?;
        Err(AppError::Other(anyhow::anyhow!("pg_dump failed: {err}")))
    }
}

pub async fn list_runs(state: &AppState) -> AppResult<Vec<super::domain::BackupRunRow>> {
    repository::recent_runs(&state.db, 20).await
}
```

> **抽象境界を足さない方針**: バックアップに trait/dyn は導入しない（`shared/llm` 以外に抽象境界を増やさない方針）。pg_dump 呼び出しも `service.rs` 内に閉じる（2つ目の実装予定が無い）。**本機能は LLM を呼ばない**ので `shared/llm` への依存はゼロ。LLM キャッシュは「列値として保全・復元するデータ」に過ぎない（§8）。

### 5.5 `handler.rs`（axum ハンドラ）

```rust
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::domain::{BackupRunRow, ImportSummary};
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

/// Authorization: Bearer <token> または X-Backup-Token から提示トークンを取り出す。
fn presented_token(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()) {
        if let Some(rest) = v.strip_prefix("Bearer ") {
            return Some(rest.trim().to_string());
        }
    }
    headers
        .get("x-backup-token")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
}

pub async fn export(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Response> {
    service::check_token(&state, presented_token(&headers).as_deref())?;
    let body: Body = service::export_ndjson(&state).await?;
    let resp = Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"rss-backup.ndjson\"",
        )
        .body(body)
        .expect("valid response");
    Ok(resp)
}

pub async fn import(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String, // NDJSON 本文をそのまま受ける（Content-Type 非依存）
) -> AppResult<Json<ImportSummary>> {
    service::check_token(&state, presented_token(&headers).as_deref())?;
    let summary = service::import_ndjson(&state, &body).await?;
    Ok(Json(summary))
}

pub async fn runs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<BackupRunRow>>> {
    service::check_token(&state, presented_token(&headers).as_deref())?;
    Ok(Json(service::list_runs(&state).await?))
}

// import の StatusCode を明示したい場合は IntoResponse で 200 + Json を返す（既定でOK）。
let _ = StatusCode::OK; // （上記 Json は 200。この行は説明用。実コードには不要）
let _ = IntoResponse::into_response; // 同上
```

> `import` は `body: String` で生本文を受ける（axum 0.8 は `String` エクストラクタで本文全体を文字列化できる）。**巨大入力対策**として、ルーター側で `DefaultBodyLimit` を緩める／明示する必要がある（既定 2MB。§11 / 実装手順 step 7）。

### 5.6 `mod.rs`（routes）

```rust
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/backup/export", get(handler::export))
        .route("/api/backup/import", post(handler::import))
        .route("/api/backup/runs", get(handler::runs))
        // import はバックアップ全文を受けるので body 上限を引き上げる（例 256MB）。
        .layer(DefaultBodyLimit::max(256 * 1024 * 1024))
}
```

### 5.7 `features/mod.rs` への追加（2行のみ）

```rust
pub mod backup; // 既存 pub mod 群（articles/feeds/...）の並びに追加
// router() の .merge チェーンに追加:
        .merge(backup::routes())
```

既存スライス（articles/feeds/folders/feed_overview/instapaper/search/stats/health）は一切触らない。

### 5.8 `shared/config.rs` への追加

```rust
// AppConfig に追加するフィールド
pub backup_token: Option<String>,            // BACKUP_TOKEN（未設定で機能無効）
pub backup_dir: Option<String>,              // BACKUP_DIR（pg_dump 出力先。任意）
pub backup_pgdump_interval_secs: Option<u64>,// BACKUP_PGDUMP_INTERVAL_SECS（任意）

// from_env() 相当の組み立てに追加（既存 .ok() パターンに合わせる）
backup_token: std::env::var("BACKUP_TOKEN").ok().filter(|s| !s.is_empty()),
backup_dir: std::env::var("BACKUP_DIR").ok().filter(|s| !s.is_empty()),
backup_pgdump_interval_secs: std::env::var("BACKUP_PGDUMP_INTERVAL_SECS")
    .ok()
    .and_then(|s| s.parse().ok()),
```

`main.rs` の起動後、既存スケジューラ spawn の近くに追加（任意機能）:

```rust
crate::features::backup::service::spawn_pgdump_scheduler(state.clone());
```

### 5.9 AppError の使い分け（`error.rs` は不編集）

| 状況 | バリアント | HTTP | レスポンス `error` 文字列（Display） |
|---|---|---|---|
| `BACKUP_TOKEN` が未設定（機能ゲート） | `NotEnabled` | 503 | `feature not yet enabled: BACKUP_TOKEN is not set` |
| トークン不一致 / 未提示 | `Validation` | 400 | `invalid input: invalid or missing backup token` |
| meta 行欠落 / フォーマット v が新しすぎ / JSON 不正 | `Validation` | 400 | `invalid input: missing meta header ...` 等 |
| DB エラー（トランザクション失敗・FK 違反等） | `Database`（`?` で自動 `From`） | 500 | `internal error` |
| pg_dump コマンド失敗（スケジューラ内・API 非公開） | `Other`（ログのみ） | — | `tracing::error!` に記録 |

> **401 を返さない理由**: `AppError` に `Unauthorized` バリアントは無く、「新バリアントを足さない（`error.rs` 不編集）」方針に従い、トークン不一致は `Validation`（400）で表現する。家庭内 LAN 単一ユーザーでは 400/401 の区別は実害が無い。厳密な 401 が必要なら将来 `AppError::Unauthorized` を足す（本機能スコープ外。§11）。
> **チェック順序**: 全ハンドラで **(1) トークンゲート → (2) 本処理** の順。`BACKUP_TOKEN` 未設定なら何より先に 503（`llm_client()` を先に判定する既存パターンと同型）。

---

## 6. フロントエンド設計

### 6.1 `lib/api.ts` への追加（型2 + メソッド3）

```ts
export interface ImportSummary {
  folders: number;
  feeds: number;
  articles: number;
  read_later: number;
  skipped: number;
}

export interface BackupRun {
  id: string;
  started_at: string;
  finished_at: string | null;
  status: "running" | "succeeded" | "failed";
  file_path: string | null;
  byte_size: number | null;
  error: string | null;
}
```

エクスポート/インポートは Blob・生本文を扱うため、既存 `http<T>()`（JSON 前提・204 畳み込み）ではなく **専用 fetch** を `api` に足す。トークンは `Authorization: Bearer` で送る:

```ts
  // エクスポート: NDJSON を Blob で取得（呼び出し側がダウンロードを発火）。
  exportBackup: async (token: string): Promise<Blob> => {
    const res = await fetch("/api/backup/export", {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!res.ok) {
      throw new Error(`${res.status} ${res.statusText}: ${await res.text()}`);
    }
    return res.blob();
  },

  // インポート: NDJSON 本文（File の text）を POST。集計サマリを返す。
  importBackup: async (token: string, ndjson: string): Promise<ImportSummary> => {
    const res = await fetch("/api/backup/import", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/x-ndjson",
      },
      body: ndjson,
    });
    if (!res.ok) {
      throw new Error(`${res.status} ${res.statusText}: ${await res.text()}`);
    }
    return res.json();
  },

  listBackupRuns: async (token: string): Promise<BackupRun[]> => {
    const res = await fetch("/api/backup/runs", {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!res.ok) {
      throw new Error(`${res.status} ${res.statusText}: ${await res.text()}`);
    }
    return res.json();
  },
```

> **`http<T>()` を使わない理由**: エクスポートは `application/x-ndjson` の Blob（JSON parse すると壊れる）、インポートは生本文 POST で 200+JSON 応答という非定型 I/O。既存ヘルパは JSON 専用なので、ここだけ `fetch` を直に使う（既存ヘルパは改変しない）。

### 6.2 `routes/Settings.tsx` に「バックアップ / 復元」Card を追記

機能 05 が `Settings.tsx` を新設済み（または本機能が先着なら新規作成）。Instapaper Card と並べて **Card を1枚追記**する。状態は **ローカル**（`createSignal`）。グローバルストアは不要。

骨子:
- `const [token, setToken] = createSignal(localStorage.getItem("backupToken") ?? "");`
  入力時 `localStorage.setItem("backupToken", v)` で保持（単一ユーザー・LAN 前提の簡便策）。`<Input type="password" />` で隠す。
- **エクスポート**ボタン: `await api.exportBackup(token())` → 返った Blob を `URL.createObjectURL` → `<a download>` を生成しクリックしてダウンロード → `revokeObjectURL`。失敗は `catch` で `error()` に表示（503=トークン未設定、400=トークン不一致）。
- **インポート**: `<input type="file" accept=".ndjson,.json,application/x-ndjson">` で選択 → `await file.text()` → `await api.importBackup(token(), text)` → 返った `ImportSummary` を「フォルダ N / フィード N / 記事 N / 後で読む N / スキップ N 件を取り込みました」と表示。完了後、フィード一覧などが変わるので `useApp().refetchFeeds()` / `refetchFolders()` を呼んで再取得（`store.tsx` 既存メソッド）。
- **実行履歴**（任意・pg_dump 使用時）: `createResource(() => api.listBackupRuns(token()))` で `BackupRun[]` を取得し、新しい順に `status` バッジ + ファイルパス + サイズを一覧表示。0件なら非表示。
- `busy` シグナルで二重送信を抑止。装飾は `card.tsx` / `button.tsx` / `input.tsx` と oklch トークン（`bg-background`, `text-muted-foreground` 等）。説明文は `text-xs text-muted-foreground` で「全データ（要約・翻訳キャッシュ含む）を書き出します。資格情報は含まれません」と明記。

### 6.3 ルーティング / 導線

`/settings` ルートは機能 05 / 04 が整備済み（未存在なら本機能で `index.tsx` に `<Route path="/settings" component={Settings} />` を追加）。設定画面への導線（Sidebar / ヘッダ）は二ペイン（機能 10）が整備する。本機能は `/settings` を開けば使える状態にしておけば足りる。

### 6.4 Ark UI について

必要な UI は input / button / card / file input のみで自前 Tailwind で賄える。**Ark UI 部品は本機能では不要**。

---

## 7. API 契約

> すべて `/api` プレフィックス。全エンドポイントが `BACKUP_TOKEN` ゲート（未設定=503、不一致=400）。トークンは `Authorization: Bearer <token>` または `X-Backup-Token: <token>` で提示。

### 7.1 `GET /api/backup/export` — 全データを NDJSON ストリームで取得
リクエスト: ヘッダ `Authorization: Bearer <BACKUP_TOKEN>`
レスポンス（200）: `Content-Type: application/x-ndjson`, `Content-Disposition: attachment; filename="rss-backup.ndjson"`
本文（NDJSON 例）:
```
{"v":1,"kind":"meta","exported_at":"2026-06-30T12:00:00+00:00","app":"rss-reader"}
{"kind":"folder","id":"...","name":"Tech","position":0,"created_at":"..."}
{"kind":"feed","id":"...","url":"https://a/feed.xml","title":"A","folder_id":"...","created_at":"...","last_fetched_at":"..."}
{"kind":"article","id":"...","feed_id":"...","url":"https://a/p1","title":"...","content":"...","published_at":"...","is_read":true,"summary":"要約","summary_lang":"ja","translation":null,"translation_lang":null,"processed_at":"...","created_at":"..."}
{"kind":"read_later","article_id":"...","status":"added","instapaper_added_at":"...","last_error":null,"created_at":"...","updated_at":"..."}
```
エラー:
- 503 `{ "error": "feature not yet enabled: BACKUP_TOKEN is not set" }`
- 400 `{ "error": "invalid input: invalid or missing backup token" }`

### 7.2 `POST /api/backup/import` — NDJSON を冪等マージ
リクエスト: ヘッダ `Authorization: Bearer <token>`, `Content-Type: application/x-ndjson`、本文は §7.1 と同じ NDJSON。
レスポンス（200）:
```json
{ "folders": 3, "feeds": 12, "articles": 540, "read_later": 7, "skipped": 0 }
```
エラー:
- 503 `{ "error": "feature not yet enabled: BACKUP_TOKEN is not set" }`
- 400 `{ "error": "invalid input: missing meta header (first line must be kind=meta)" }`
- 400 `{ "error": "invalid input: line 42: bad article: ..." }`
- 400 `{ "error": "invalid input: backup format v2 is newer than supported v1" }`
- 500 `{ "error": "internal error" }`（トランザクション/FK 違反。**全体ロールバック**で部分適用は起きない）

> **冪等性の保証**: feed/article は `url` UNIQUE で upsert、folder/read_later は id PK で upsert。同一ファイルを2回 import しても件数カウントは同じだが DB 状態は変わらない。LLM キャッシュ・既読は `COALESCE`/`OR` で「失わない」マージ（§5.3）なので、トークン再消費・既読の巻き戻りが起きない。

### 7.3 `GET /api/backup/runs` — pg_dump 実行履歴（任意機能）
リクエスト: ヘッダ `Authorization: Bearer <token>`
レスポンス（200、新しい順・最大20件。スケジューラ未使用なら `[]`）:
```json
[
  { "id":"...","started_at":"...","finished_at":"...","status":"succeeded","file_path":"/backups/rss-20260630T120000Z.sql","byte_size":10485760,"error":null }
]
```

---

## 8. 依存関係

- **本機能が依存する機能（DB 上の前提）**:
  - **既存テーブル `folders`（機能 02 / `0002`）・`read_later_items`（機能 06 / `0004`）が存在すること**。本機能のエクスポート/インポートはこれらを読み書きする。02/06 がまだ無い環境では、当該テーブルが無く起動時に export クエリが失敗するため、**02・06 のマイグレーション適用後に本機能を有効化**する（ソフト依存。テーブルが無ければ該当 kind を空にする実装も可能だが、本書は「全テーブル存在」を前提とする）。
  - `feeds` / `articles`（`0001_init`）は常に存在。
- **本機能をブロックする機能**: 無し（`backup` スライスは自己完結。他スライスから参照されない）。
- **ソフトな協調**:
  - 機能 04（ダークテーマ）/ 05（Instapaper）と `/settings` ルート・設定画面を共有（Card 単位で非干渉。先着が `Settings.tsx` を作成、後着が Card を足す）。
  - 機能 10（二ペイン）が設定画面への導線を整備。
- **将来のタグ機能**: タグスライスが入ったら、export に kind `"tag"`／関連付け行を追加し、import の `match` に1アームを足すだけで拡張できる（フォワード互換。未知 kind は現状 skip されるので、古いバックアップとの相互運用も安全）。
- 既存スライス（articles / feeds / folders / feed_overview / instapaper / search / stats / health）への変更は無し。接触点は **`features/mod.rs` の2行**と **`config.rs` のフィールド追加**と **`main.rs` の任意 spawn 1行**のみ。

> **AI（`shared/llm`）との関係**: 本機能は **LLM を一切呼ばない**。`articles.summary` / `translation` は「オンデマンドで Claude を呼んで得た DB キャッシュ」（`articles/service.rs`）であり、バックアップは**それを列値として保全・復元するだけ**。再要約・再翻訳は行わない（行えばトークンを再消費し、本機能の目的＝トークン防衛に反する）。したがって `ANTHROPIC_API_KEY` への依存も無い。仮に将来「import 後に未要約記事を自動要約する」拡張を入れるなら、その時に `articles/service.rs` の既存 `summarize_article`（内部で `ANTHROPIC_API_KEY` 未設定なら `AppError::NotEnabled` を返す）を**再利用**し、本スライスに新規 LLM 呼び出しは足さない。これは本機能のスコープ外（§11）。

---

## 9. テスト計画（TDD）

> **配置方針**: 本クレートは **binary crate（`lib.rs` 無し）**のため、`backend/tests/` 別クレートから内部関数を呼べない。よって (1) 純粋ロジックは `domain.rs` 内の `#[cfg(test)] mod tests`、(2) DB 往復は `repository.rs` 内の `#[cfg(test)] mod tests`（実 DB・`#[ignore]`）、(3) HTTP 表面は shell スクリプト（稼働スタックに curl）の三段。機能 05 の配置と同型。

### 9.1 単体テスト（`#[cfg(test)] mod tests` in `domain.rs`、I/O 不要）

| テスト | 意図 |
|---|---|
| `parse_line_empty_returns_none` | 空行・空白行は `Ok(None)` |
| `parse_line_meta` | `kind=meta` → `Record::Meta`、`v` を読む |
| `parse_line_feed` | `kind=feed` → `Record::Feed`、url/title/folder_id を復元 |
| `parse_line_article_with_nulls` | `summary`/`translation` が `null` の記事行を復元（Option が None） |
| `parse_line_read_later` | `kind=read_later` → `Record::ReadLater` |
| `parse_line_unknown_kind_is_unknown` | 未知 kind → `Record::Unknown`（前方互換） |
| `parse_line_invalid_json_errs` | 壊れた JSON 行は `Err` |
| `parse_line_missing_kind_is_unknown` | `kind` 欠落行は `Unknown`（落ちない） |
| `check_version_accepts_current` | `v == FORMAT_VERSION` は `Ok` |
| `check_version_accepts_older` | `v < FORMAT_VERSION` は `Ok`（後方互換） |
| `check_version_rejects_newer` | `v > FORMAT_VERSION` は `Err` |
| `to_line_appends_newline` | シリアライズ行末に `\n` が付く |
| `roundtrip_feed_row_serde` | `FeedRow` を `to_value`→`parse_line` で往復一致（serde 整合） |

### 9.2 リポジトリ往復テスト（`#[cfg(test)] mod tests` in `repository.rs`、実 DB / `#[ignore]`）

`DATABASE_URL`（`just dev-db` の DB、マイグレーション適用済み）で接続。`#[tokio::test]` + `#[ignore = "requires Postgres"]`。`cargo test -- --ignored` で実行。

| テスト | 意図 |
|---|---|
| `upsert_feed_is_idempotent_on_url` | 同 url を2回 upsert → 行は1つ、返る id は同一（冪等・url 衝突で実 id 採用） |
| `upsert_article_preserves_cache_on_conflict` | 既存に `summary` 有り → import 側 `summary=null` で upsert しても要約が消えない（`COALESCE`、トークン防衛） |
| `upsert_article_is_read_monotonic` | 既存 `is_read=true` → import `is_read=false` でも `true` を維持（`OR`） |
| `import_roundtrip_export_import` | `export_*` で読んだ行を別 DB（or クリーン後）に import → 件数一致・参照整合（feed_id 再マップ込み） |
| `upsert_read_later_remaps_article_id` | feed/article を url 衝突で別 id 採用したケースで read_later の article_id が正しく再マップされる |

雛形（機能 05 と同型）:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL for repo tests");
        PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap()
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres (DATABASE_URL)"]
    async fn upsert_feed_is_idempotent_on_url() {
        let pool = pool().await;
        let mut tx = pool.begin().await.unwrap();
        let r = FeedRow { /* ... 固定値、url にユニークなテスト URL ... */ };
        let id1 = upsert_feed(&mut tx, &r).await.unwrap();
        let id2 = upsert_feed(&mut tx, &r).await.unwrap();
        assert_eq!(id1, id2);
        tx.rollback().await.unwrap(); // 副作用を残さない
    }
}
```

### 9.3 HTTP スモークテスト（`scripts/test/api-backup.sh`、稼働スタックへ curl）

`scripts/test/api-stats.sh` と同型（nginx 経由）。**`BACKUP_TOKEN` 未設定/設定の両系をどう検証するか**を明記:

| 手順 / アサーション | 意図 |
|---|---|
| `BACKUP_TOKEN` 未設定で `GET /api/backup/export`（トークン無し）→ 503 | 機能ゲート（`NotEnabled`）。スライス合成確認 |
| `BACKUP_TOKEN=secret` を設定して再起動 → `GET /api/backup/export`（トークン無し）→ 400 | トークン照合（`Validation`） |
| `Authorization: Bearer secret` 付き `GET /api/backup/export` → 200 かつ `Content-Type: application/x-ndjson`、1行目が `"kind":"meta"` を含む | 正常系・フォーマット |
| 上で得た NDJSON を `POST /api/backup/import`（Bearer 付き）→ 200 かつ JSON に `articles` キー | 冪等インポート配線（自身を再取り込み） |
| 同じ NDJSON をもう一度 import → 200・件数同一・DB 行数が増えない（`GET /api/stats` の総数比較） | 冪等性の実証 |
| `GET /api/backup/runs`（Bearer 付き）→ 200 かつ JSON 配列 | runs 配線（空配列でも可） |

> 環境変数の切替（未設定 ↔ 設定）はスタック再起動を伴うため、スクリプトは「現在 `BACKUP_TOKEN` が設定されている前提」の正常系＋トークン不一致(400)を主に検証し、503 系は手動 step として残してよい。

### 9.4 フロント（手動 + 型）
- `tsc` 型チェック（`just lint`）で `api.ts`（`exportBackup`/`importBackup`/`listBackupRuns`・`ImportSummary`・`BackupRun`）と `Settings.tsx` の整合を確認。
- 手動: `/settings` でトークン入力 → エクスポート（ファイルがダウンロードされる）→ そのファイルをインポート（「… 件取り込みました」表示・一覧再取得）→ 履歴表示。トークン誤りで 400 表示。

---

## 10. 実装手順（順序付きチェックリスト）

1. **マイグレーション番号を採番**: `ls backend/migrations/` で最大番号を確認（本書時点では `0005_search.sql`）。`+1` を採る（暫定 `0006_backup.sql`）。並行機能が先取りしていたら繰り上げる。**既に高い番号が永続 DB に適用済みなら、より小さい番号を新規追加しない**（§4.1 の out-of-order 注意）。
2. **マイグレーション作成**: 採番したファイルを §4.2 の SQL（`backup_runs` ＋ index）で新規作成。既存ファイルは触らない。
3. **config 追加**: `shared/config.rs` に `backup_token` / `backup_dir` / `backup_pgdump_interval_secs` フィールドと env 読み出しを追加（§5.8）。
4. **ドメイン（Red 先行）**: `features/backup/domain.rs` を作り、§5.2 の DTO・`parse_line`・`check_version`・`to_line`・`BackupRunRow` ＋ §9.1 の `#[cfg(test)] mod tests` を書く。まずテストが落ちることを確認 → 実装で Green。`cargo test`（DB 不要）で実行。
5. **repository**: `repository.rs` を §5.3 で作成（`query`/`query_as`/`query_scalar` のみ、`query!` 不可）。§9.2 の `#[cfg(test)] mod tests`（`#[ignore]`）も書く。
6. **service**: `service.rs` を §5.4 で作成（`check_token`・`export_ndjson`・`import_ndjson`・`spawn_pgdump_scheduler`・`list_runs`）。`async-stream` を使う場合は `Cargo.toml` に追加、使わないなら本書のメモリ展開実装でよい。
7. **handler + mod + 合成**: `handler.rs`（§5.5、トークン抽出・export/import/runs）、`mod.rs`（§5.6、ルート3本 ＋ `DefaultBodyLimit::max(...)`）を作成。`features/mod.rs` に `pub mod backup;` と `.merge(backup::routes())` を追加（§5.7）。`main.rs` に `spawn_pgdump_scheduler(state.clone())`（任意機能）。
8. **ビルド & lint**: `just lint`（clippy `-D warnings` / tsc）を通す。binary crate のため未使用警告に注意（全関数が handler/main から到達することを確認）。
9. **DB 起動 & マイグレーション**: `just dev-db` →（バックエンド起動で自動 migrate、または `just migrate`）。`backup_runs` 生成を確認。
10. **リポジトリ往復テスト**: `DATABASE_URL=... cargo test -- --ignored` で §9.2 を Green に（冪等・キャッシュ保全・再マップ）。
11. **HTTP スモークスクリプト**: `scripts/test/api-backup.sh` を §9.3 で作成・`chmod +x`・実行。`BACKUP_TOKEN` を設定したスタックで Bearer 付き 200 / 不一致 400 / 二重 import の件数不変を assert。
12. **フロント**: `lib/api.ts` に型2・メソッド3（§6.1）。`routes/Settings.tsx` に「バックアップ / 復元」Card を追記（§6.2、トークンは localStorage、ダウンロード/ファイル選択/履歴）。`/settings` ルートが無ければ追加。`just lint` の tsc を通す。
13. **手動 E2E**: `/settings` でトークン入力 → エクスポート（ファイル取得）→ クリーンな DB（or 同 DB）へインポート → 件数表示・一覧再取得・要約/既読/後で読むが保全されていることを確認 → （任意）`BACKUP_DIR`+interval を設定し pg_dump 実行 → `runs` に成功行が出るか確認。
14. **コミット**: マイグレーション・スライス・config・main・スクリプト・フロントをまとめて。`.env`・`BACKUP_TOKEN`・バックアップファイルはコミットしない（`.gitignore` に `BACKUP_DIR` 候補を追加検討）。

---

## 11. リスク・未決事項・代替案

- **【設計判断】NDJSON ストリーミング vs メモリ展開**: §5.4 の `export_ndjson` は実装簡潔さのため**全件を String に組み立てて `Body::from(buf)`** する（家庭内・単一ユーザーの記事数なら問題なし）。記事数が極端に増えたら (a) `async-stream` の `try_stream!` で `all_articles` を keyset ページング（`created_at` カーソル）しながらチャンク yield、(b) `Body::from_stream` に差し替える、で**メモリ一定**にできる。インポートは `body: String`（全文）受けなので、巨大入力では `DefaultBodyLimit`（§5.6 で 256MB）とメモリに注意。真にストリーミング import が要るなら axum の `Request<Body>` を取り `lines` を非同期に読む実装へ拡張（スコープ外）。
- **【要確認】マイグレーション番号の順序ハザード**: §4.1 のとおり `run_migrations` は `set_ignore_missing` を呼ばない。**着手前に必ず最新番号を確認**し、最小空き整数を取る。`0006` が他機能に取られていれば繰り上げる。
- **トークン保護の強度（401 を返さない）**: `AppError` に `Unauthorized` が無いため不一致は `Validation`(400)。LAN 単一ユーザーでは実害なしだが、外部公開するなら (a) `AppError::Unauthorized` を足して 401 化、(b) `CorsLayer::permissive()` の見直し、(c) HTTPS 終端、を検討（本機能スコープ外）。トークン比較は §5.4 の定数時間比較で簡易的にタイミング攻撃を緩和。
- **資格情報を含めない判断**: `instapaper_credentials` はパスワード平文のためエクスポートしない（§4.3）。復元後は `/settings` で Instapaper を再設定する必要がある。これは意図的なセキュリティ判断。
- **id 再マップの整合**: feed/article は `url` UNIQUE のため、別インスタンスへ復元する際に「同 url・別 id」で衝突しうる。本書は upsert の `RETURNING id` で**実 id を採用し、子レコードの外部キーを再マップ**する（§5.3/§5.4）。同一インスタンスへの自己復元では id が一致するので再マップは恒等写像になる。**注意**: NDJSON の出力順（feed→article→read_later）に依存して再マップが成立するので、**出力順を崩さないこと**（手書き NDJSON を import する場合も依存順を守る）。
- **`is_read` / キャッシュのマージ方針**: 既読は単調（`OR`）、LLM キャッシュは「非 null 保全」（`COALESCE`）。これは「バックアップは情報を失わない」原則。**もし「インポート側で完全上書きしたい」要件**が出たら、`COALESCE`/`OR` を `EXCLUDED.*` 直代入に変える（1ファイル内の SQL 変更で切替可能）。MVP は保全（トークン防衛）を既定とする。
- **read_later の FK 依存**: `read_later_items.article_id` は `articles(id)` への FK。NDJSON が article より先に read_later を持つ壊れた順序だと FK 違反で**トランザクション全体がロールバック**（部分適用なし）。出力は依存順を守るので正常系では起きない。手書き入力のリスクとして §7.2 に明記済み。
- **pg_dump の前提**: スケジューラは外部コマンド `pg_dump` がコンテナ内に存在することを要求する。バックエンドイメージに `postgresql-client` が無い場合は (a) Dockerfile に追加、(b) スケジューラを使わず API エクスポートのみ運用、のいずれか。**スケジューラは env 未設定なら起動しない**ので、入れなければ依存は発生しない。`BACKUP_DIR` はボリュームマウント先を指定する。
- **タグ未実装**: 現状 `tags` テーブルは無い。export に kind `"tag"` は出さない。タグスライス導入後に本スライスへ追記（未知 kind は import で skip されるため、新旧バックアップの相互運用は安全）。§8 のフォワード互換メモ参照。
- **同時実行**: import 中に通常のフィード取得（スケジューラ）が走ると、同じ `articles` 行に触れうる。upsert は行ロックで直列化され、トランザクション分離で整合する。大規模 import は短時間スケジューラ停止を推奨（運用メモ）。
- **AI 機能との関係（再掲・重要）**: 本機能は LLM を呼ばない。LLM キャッシュは保全対象データに過ぎない。将来「import 後の自動要約」を足すなら既存 `articles/service.rs::summarize_article`（`ANTHROPIC_API_KEY` 未設定で `AppError::NotEnabled`）を再利用し、本スライスに新規 LLM 呼び出し・新 trait を足さない（`shared/llm` 以外に抽象境界を作らない方針）。スコープ外。
