# 26 意味的クラスタリング & 重複排除

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッション（effort 低め）の実装者。本書 1 ファイルだけで着手・完了できるよう、再利用資産・完全な SQL・関数シグネチャ・ルート文字列・フロント差分・TDD まで具体化する。
> **重要な但し書き**: 本機能の「論調差の統合要約」は Claude（`shared/llm`）を使う。クラスタの計算（近接判定・グループ化）は **Claude を使わず** PostgreSQL の `pg_trgm` 類似度 + Rust 側のユニオンファインド・ヒューリスティックで行う（トークンを消費しない）。要約は **オンデマンド + DB キャッシュ**で、`ANTHROPIC_API_KEY` 未設定時は `AppError::NotEnabled`（503）を返す「任意機能」パターンに従う（要約/翻訳/ダイジェストと同型）。

---

## 1. 概要

複数フィードを購読していると、同一の出来事を各社が別々に報じた **ほぼ重複した記事**が一覧に並ぶ。本機能は **直近 N 時間の記事をタイトル近接度でグループ化**し、「同じ話題」を 1 枚のカードにまとめて表示する。グループ化はバックグラウンドジョブ（`shared/scheduler.rs` に相乗り）で定期的に再計算し、結果を `article_clusters` / `cluster_members` テーブルにキャッシュする。`GET /api/clusters` がグループ化済みカードを返す。

近接計算は **`pg_trgm` のトリグラム類似度**（`0005_search.sql` で導入済みの拡張）を SQL で計算し、しきい値を超えたペアを「辺」として Rust 側の **ユニオンファインド（純粋関数）**でクラスタにまとめる。同一クラスタ内で代表記事への類似度が **重複しきい値**を超えるメンバーは「重複（dedup）」としてマークする。これらの近接ロジックはすべて Claude を介さず動く（=費用ゼロ・決定的・テスト容易）。

各クラスタに対しては任意で、ユーザー要求時に **Claude による「各社の論調差を統合した要約」**を生成・キャッシュできる（`POST /api/clusters/{id}/summary`）。これは記事要約/翻訳/ダイジェストと同じ **オンデマンド + キャッシュ + `NotEnabled`** 方針。

実装はバックエンドに **新スライス `clustering` を 1 枚**追加し、(a) 再クラスタリング（scheduler から起動 + 手動 `POST /api/clusters/recluster`）、(b) `GET /api/clusters`・`GET /api/clusters/{id}` での取得、(c) `POST /api/clusters/{id}/summary` での統合要約生成を担う。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）
- マイグレーション **`0006_clustering.sql`**（番号は暫定。**着手前に必ず `ls backend/migrations/` で最新番号を確認し +1 を採る**。現状の最新は `0005_search.sql`。§4.1）。`article_clusters` と `cluster_members` の 2 テーブル。
- 新スライス `backend/src/features/clustering/`（`domain` / `repository` / `service` / `handler` / `mod`）。
- 再クラスタリング: `shared/scheduler.rs` に **再計算ループを 1 つ追加**。設定間隔で「直近 N 時間の記事」を読み、`pg_trgm` 類似度 + ユニオンファインドでグループ化し、クラスタ表を **総入れ替え（delete→insert）**する（冪等）。
- 手動再計算 API: `POST /api/clusters/recluster`（即時再計算。テスト容易性・即時確認用）。Claude は呼ばないので APIキー不要。
- 取得 API: `GET /api/clusters`（`size >= min_size` のクラスタをメンバー込みで返す）/ `GET /api/clusters/{id}`（単一クラスタ）。
- 統合要約 API: `POST /api/clusters/{id}/summary`（任意 body `{ "target_lang": "ja" }`）。キャッシュ命中時は再課金せず返す。`ANTHROPIC_API_KEY` 未設定なら 503、クラスタ不在なら 404。
- LLM 連携: `shared/llm` の `LlmClient` trait に **`cluster_summary` メソッドを 1 つ追加**し、`anthropic.rs` に実装（既存 private `complete` を再利用）。新 trait は作らない。
- 統合要約のキャッシュ持ち越し: クラスタは再計算で総入れ替えされるが、**メンバー集合の指紋（signature）が一致する**クラスタには旧キャッシュ要約を引き継ぐ（再課金回避。§5.3）。
- フロント `/clusters` ルート（`routes/Clusters.tsx`）: クラスタをカード表示。各カードに代表タイトル・件数/媒体数バッジ・メンバー一覧（媒体名 + リンク + 重複マーク）・「統合要約」ボタン。
- `lib/api.ts` に型 `Cluster` / `ClusterMember` / `ClusterWithMembers` と **4 メソッド**（`listClusters` / `getCluster` / `summarizeCluster` / `reclusterNow`）。
- `config.rs` に clustering 関連 env（ON/OFF・窓時間・間隔・各しきい値・最小サイズ・要約言語）を追加。
- ドメイン純粋関数の単体テスト、リポジトリ往復テスト（実 DB・`#[ignore]`）、HTTP スモークスクリプト。

### 非スコープ（本機能では実装しない）
- 埋め込みベクトル（pgvector）による意味的クラスタリング。MVP は **タイトルのトリグラム類似度**のみ（本文ベクトルは将来拡張。§11）。
- フィード/フォルダ単位のクラスタリング（MVP は全フィード横断・直近窓のみ）。
- クラスタの手動編集・メンバーの手動移動・クラスタの恒久ピン留め。
- 記事一覧（既存 `ArticleList`）へのクラスタ折りたたみ統合。本機能は専用 `/clusters` ビューに閉じる（既存スライスを触らないため。§8）。
- 既読/未読の連動（クラスタは既読状態に依存せず、窓内の全記事を対象とする）。フィルタは将来拡張。
- 統合要約の自動定期生成（コスト管理のため要約は **オンデマンドのみ**。再計算ジョブは Claude を呼ばない）。

---

## 3. 既存実装の再利用

実ファイルを確認済み。以下を **再利用し、車輪の再発明をしない**。

| 再利用資産 | 実体（確認済みファイル） | 本機能での使い方 |
|---|---|---|
| トリグラム類似度基盤 | `backend/migrations/0005_search.sql`（`CREATE EXTENSION pg_trgm` + `gin_trgm_ops` インデックス） | 近接計算に `similarity(a.title, b.title)` / `%` 演算子を使う。拡張は導入済み（0006 でも防御的に再宣言。§4.2） |
| LLM 抽象境界（唯一の trait） | `backend/src/shared/llm/mod.rs`（`LlmClient` trait + `SummarizeRequest`/`TranslateRequest`）、`anthropic.rs`（`AnthropicClient`、private `complete(system, user)`、Messages API 直叩き、`max_tokens:1024`） | trait に **`cluster_summary` メソッドを 1 つ追加**し、`anthropic.rs` で `complete` を再利用。新 trait は作らない（既存の唯一の境界を拡張） |
| 任意機能 = `NotEnabled` + `llm_client()` 生成 | `articles/service.rs::llm_client()`（`anthropic_api_key` 無し時に `NotEnabled("ANTHROPIC_API_KEY is not set")`、有り時 `AnthropicClient::new(state.http.clone(), key, model)`） | `clustering/service.rs` に同型の `llm_client(state)` を持ち、未設定なら `NotEnabled` |
| キャッシュして再課金しない方針 | `articles/service.rs::summarize_article`（同一 lang のキャッシュ命中時は API を呼ばず返す）、`digest`（date キャッシュ） | クラスタ要約を `article_clusters.summary`/`summary_lang` にキャッシュ。同一言語の再要求は API を呼ばない |
| 日次/定期バッチの置き場所 | `backend/src/shared/scheduler.rs`（`tokio::interval` で `feeds::service::refresh_all_feeds` を定期実行。`main.rs` が `scheduler::spawn(state.clone())`） | 同ファイルに **再クラスタリングループ**を 1 つ追加。`main.rs` から `scheduler::spawn_clustering(state.clone())` を 1 行で起動（§5.6） |
| `AppState { db, config, http }` | `backend/src/shared/state.rs`（`#[derive(Clone)]`） | `state.db` / `state.http` / `state.config` をそのまま使う |
| `AppError` 6 バリアント | `backend/src/shared/error.rs`（`NotFound`/404, `Validation(String)`/400, `NotEnabled(String)`/503, `Upstream(String)`/502, `Database`/500, `Other`/500、`IntoResponse` で `Json({"error": <Display>})`） | 新バリアントを足さず既存で表現（§5.9）。**`error.rs` は編集しない** |
| 主キー newtype（値オブジェクト） | `feeds/domain.rs::FeedId`、`articles/domain.rs::ArticleId`（`#[derive(... sqlx::Type)] #[sqlx(transparent)]`） | `ClusterId(Uuid)` を同型で新設（§5.1） |
| 値オブジェクト `parse() -> Result<_, String>` | `feeds/domain.rs::FeedUrl::parse`（検査 + `#[cfg(test)] mod tests`） | 言語入力等のバリデーションを同型で（本機能は主に純粋ロジック関数で TDD。§5.1） |
| スライス構成 + `routes()` | `articles/mod.rs`・`feeds/`・`digest/`（`domain/repository/service/handler/mod`、`fn routes() -> Router<AppState>`） | 同じ 5 ファイル構成で `clustering` を作る |
| `features/mod.rs` の合成 | `pub mod ...;` 群 + `router()` の `.merge(...::routes())` チェーン | `pub mod clustering;` と `.merge(clustering::routes())` を 1 行ずつ追加。既存スライスは触らない |
| sqlx ランタイムクエリ + upsert + トランザクション | `articles/repository.rs`（`fetch_optional().ok_or(AppError::NotFound)`、`INSERT ... ON CONFLICT DO UPDATE`）、`feed_overview`（feeds+articles JOIN read） | クラスタ取得は `fetch_optional`/`fetch_all`、総入れ替えは `BEGIN`→`DELETE`→`INSERT`→`COMMIT`（§5.2） |
| クロステーブル read を自スライス内 SQL で完結 | `feed_overview`（feeds+articles JOIN read）、`instapaper/repository.rs::get_article_ref`（`articles` 読み取り） | `clustering` から `articles`/`feeds` を **読み取り専用 SQL** で引く。書き込み所有は移さない（§5.2） |
| 設定の env マッピング | `backend/src/shared/config.rs`（1 field = 1 env、`Option`/`unwrap_or` パターン） | clustering 関連 env を同型で追加（§5.8） |
| フロント API クライアント | `frontend/src/lib/api.ts`（`http<T>()`：204→`undefined`、`errorStatus(e)` ヘルパ、`api` に `動詞+リソース` 命名で集約） | 既存 `http<T>()`/`errorStatus` を再利用し 4 メソッド追加 |
| 自前 UI 部品 | `frontend/src/components/ui/{button,card,badge}.tsx`（`cn`+Tailwind、oklch トークン） | `Clusters.tsx` で `card`/`button`/`badge` を流用。Ark UI 部品は不要 |
| HTTP スモークテストの慣習 | `scripts/test/api-*.sh`（稼働スタックに curl、HTTP コードと JSON キーを assert） | `scripts/test/api-clustering.sh` を同型で新設（§9.3） |

> **新規依存（不要）**: `pg_trgm` は 0005 で導入済み。`chrono`/`uuid`/`serde`/`sqlx`/`reqwest`/`axum` は既存依存で足りる。ユニオンファインドは外部クレートを使わず **標準ライブラリのみ**で実装する（純粋関数・テスト容易）。フロントも新規パッケージ不要（統合要約はプレーンテキスト表示で `marked` は使わない。§6.2）。

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方

`main.rs` の `db::run_migrations` → `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を設定していないため、**適用済み最大バージョンより小さい未適用マイグレーションを後から足すと起動が壊れる**（out-of-order）。よって:

- ファイル名は **着手時点の最小空き整数**。`ls backend/migrations/` で最大番号を確認し +1。**現状の最新は `0005_search.sql` なので暫定 `0006_clustering.sql`**。
- 並行作業（apalis 移行ジョブテーブル・他の `0006_*` を狙う機能 = digest/tags/mute 等）が先に `0006` を取った場合は本機能を `0007` 以降へ繰り上げる。
- 既存マイグレーションは**編集しない**（追記のみ）。

本書では以降 **`0006_clustering.sql`** と表記する（採番は着手時に再確認）。

### 4.2 スキーマ

新規ファイル **`backend/migrations/0006_clustering.sql`**:

```sql
-- Semantic clustering & deduplication.
-- A background job groups recent articles that cover the same story (computed by
-- pg_trgm title similarity + a union-find heuristic) so the UI can show one card
-- per topic instead of N near-duplicate items. An optional Claude-generated
-- "cross-outlet" summary (how different sources frame the story) is cached per
-- cluster. The cluster tables are fully rebuilt on each recluster (delete+insert).
--
-- pg_trgm is already enabled by 0005_search.sql; re-declared defensively so this
-- migration is self-contained and order-independent w.r.t. 0005.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE IF NOT EXISTS article_clusters (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Representative (canonical) headline shown on the card.
    title          TEXT NOT NULL,
    -- Number of member articles (denormalized for cheap card rendering).
    size           INTEGER NOT NULL,
    -- Stable fingerprint of the sorted member article-id set. Lets a rebuild
    -- carry over a cached summary when the exact same group reappears (§5.3).
    signature      TEXT NOT NULL,
    -- Cached cross-outlet integrated summary (Claude). NULL until requested.
    summary        TEXT,
    summary_lang   TEXT,
    summary_model  TEXT,
    summarized_at  TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Cards are ordered largest-cluster-first; index supports that ORDER BY.
CREATE INDEX IF NOT EXISTS idx_article_clusters_size ON article_clusters (size DESC);
-- Signature is unique (member sets partition the article space) and is the
-- lookup key for summary carry-over across rebuilds.
CREATE UNIQUE INDEX IF NOT EXISTS idx_article_clusters_signature
    ON article_clusters (signature);

CREATE TABLE IF NOT EXISTS cluster_members (
    cluster_id        UUID NOT NULL REFERENCES article_clusters(id) ON DELETE CASCADE,
    -- One article belongs to at most one cluster at a time => article_id is PK.
    article_id        UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    is_representative  BOOLEAN NOT NULL DEFAULT false,
    -- True when similarity to the representative >= the dedup threshold.
    is_duplicate       BOOLEAN NOT NULL DEFAULT false,
    -- pg_trgm similarity (0..1) of this article's title to the representative.
    similarity         REAL NOT NULL DEFAULT 0,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_cluster_members_cluster_id
    ON cluster_members (cluster_id);
```

設計判断:
- **総入れ替え方式**: クラスタは「直近窓のスナップショット」。再計算のたびに `cluster_members`→`article_clusters` を全削除して作り直す（`ON DELETE CASCADE` で members は自動削除）。差分更新は複雑でバグの温床なので採らない（家庭内小規模なので全削除コストは無視できる）。
- **`article_id` を PK**: 1 記事は同時に 1 クラスタのみに属する（重複排除の意味論を DB 制約で保証）。
- **`signature` UNIQUE**: メンバー集合は記事空間を分割するので衝突しない。再計算後に「同一メンバー集合のクラスタ」を見つけて旧 `summary` を引き継ぐためのキー（§5.3）。
- **`size` 列の非正規化**: カード描画で毎回 `COUNT` しないため。`GET /api/clusters` の `ORDER BY size DESC` も index で効く。
- **`is_duplicate` / `similarity` 列**: UI が「ほぼ重複（dedup）」バッジを出すため、かつ代表記事との近さを可視化するため。
- `articles`/`feeds` への列追加は無い。材料は読み取りのみ。

---

## 5. バックエンド設計

新スライス **`backend/src/features/clustering/`**。5 ファイル構成。

### 5.1 `domain.rs`（値オブジェクト + 純粋ロジック + 単体テスト対象）

近接判定の「方針」と「グループ化アルゴリズム」を **純粋関数**に切り出し、`pg_trgm`（DB）や Claude を呼ばずに TDD で Red→Green を回せるようにする（MEMORY の「書いたら必ず実行」「TDD 必須」方針）。`pg_trgm` は **類似度の数値**だけを供給し、しきい値判定・グループ化・代表選定は Rust 純粋関数が担う。

```rust
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use serde::Serialize;
use uuid::Uuid;

/// クラスタ主キー newtype（FeedId / ArticleId と同型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct ClusterId(pub Uuid);

/// 2 記事タイトルの類似度バンド（純粋関数で分類 = 単体テスト対象）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimilarityBand {
    /// 同一クラスタ + 重複扱い（dedup バッジ）。
    Duplicate,
    /// 同一クラスタだが別記事（同じ話題の別報）。
    SameTopic,
    /// 別クラスタ。
    Unrelated,
}

/// similarity(0..1) を 2 つのしきい値でバンド分けする。
/// dup_threshold >= topic_threshold を前提（呼び出し側の config で保証）。
pub fn classify_similarity(sim: f32, topic_threshold: f32, dup_threshold: f32) -> SimilarityBand {
    if sim >= dup_threshold {
        SimilarityBand::Duplicate
    } else if sim >= topic_threshold {
        SimilarityBand::SameTopic
    } else {
        SimilarityBand::Unrelated
    }
}

/// トリグラム比較前のタイトル正規化（純粋関数）。
/// 小文字化 + 連続空白の 1 個化 + 前後トリム。pg_trgm に渡す前段で安定化する。
pub fn normalize_title(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// クラスタ化アルゴリズムの中核（純粋・ユニオンファインド）。
/// `nodes`: 対象記事 id の全集合。`edges`: しきい値以上の (a, b, sim) ペア。
/// `threshold`: この値未満の辺は無視する（topic_threshold を渡す）。
/// 返り値: 連結成分ごとの記事 id 群（単独ノードも 1 要素クラスタとして含む）。
/// 入力順に対して決定的（各クラスタ内は nodes 入力順を保つ）。
pub fn group_edges(nodes: &[Uuid], edges: &[(Uuid, Uuid, f32)], threshold: f32) -> Vec<Vec<Uuid>> {
    let index: HashMap<Uuid, usize> = nodes.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut parent: Vec<usize> = (0..nodes.len()).collect();

    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    for (a, b, sim) in edges {
        if *sim < threshold {
            continue;
        }
        let (Some(&ia), Some(&ib)) = (index.get(a), index.get(b)) else {
            continue; // 窓外のノードを指す辺は無視
        };
        let ra = find(&mut parent, ia);
        let rb = find(&mut parent, ib);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    // root -> メンバー（nodes 入力順を保持）にまとめる。
    let mut groups: HashMap<usize, Vec<Uuid>> = HashMap::new();
    let mut order: Vec<usize> = Vec::new();
    for (i, id) in nodes.iter().enumerate() {
        let r = find(&mut parent, i);
        if !groups.contains_key(&r) {
            order.push(r);
        }
        groups.entry(r).or_default().push(*id);
    }
    order.into_iter().map(|r| groups.remove(&r).unwrap()).collect()
}

/// 代表記事を選ぶための候補メタ。
#[derive(Debug, Clone)]
pub struct MemberCandidate {
    pub article_id: Uuid,
    pub title: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 代表記事を 1 つ選ぶ純粋関数。
/// 規則: published_at が最も古いもの（一次報を代表に）。NULL は最後。
/// 同点ならタイトルが長い方（情報量が多い見出し）、さらに同点なら uuid 昇順で決定的。
pub fn pick_representative(members: &[MemberCandidate]) -> Uuid {
    members
        .iter()
        .min_by(|a, b| {
            let ka = (a.published_at.is_none(), a.published_at);
            let kb = (b.published_at.is_none(), b.published_at);
            ka.cmp(&kb)
                .then(b.title.chars().count().cmp(&a.title.chars().count()))
                .then(a.article_id.cmp(&b.article_id))
        })
        .map(|m| m.article_id)
        .expect("cluster has at least one member")
}

/// メンバー集合の安定指紋（純粋関数）。順不同の article_id 集合に対し同じ値を返す。
/// 再計算をまたいで「同一メンバー集合」を突き合わせ、要約キャッシュを引き継ぐキー（§5.3）。
pub fn cluster_signature(member_ids: &[Uuid]) -> String {
    let mut ids: Vec<Uuid> = member_ids.to_vec();
    ids.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for id in &ids {
        id.hash(&mut hasher);
    }
    format!("{:016x}-{}", hasher.finish(), ids.len())
}

/// Claude へ渡す 1 メンバーぶんの材料（読み取り射影）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClusterMemberSource {
    /// 媒体名（feeds.title）。NULL なら "(unknown source)"。
    pub feed_title: Option<String>,
    pub title: String,
    /// summary 優先、無ければ本文先頭。プロンプト材料用。
    pub snippet: String,
}

/// 統合要約の LLM 入力テキストを組み立てる純粋関数（= 単体テスト対象）。
/// 各メンバーを「- 【媒体名】タイトル: 抜粋」に整形し、論調差を比較しやすくする。
pub fn build_cluster_summary_input(members: &[ClusterMemberSource]) -> String {
    members
        .iter()
        .map(|m| {
            let source = m.feed_title.as_deref().unwrap_or("(unknown source)").trim();
            format!("- 【{}】{}: {}", source, m.title.trim(), m.snippet.trim())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// API レスポンス: クラスタ本体（DB 行ミラー）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Cluster {
    pub id: Uuid,
    pub title: String,
    pub size: i32,
    pub summary: Option<String>,
    pub summary_lang: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// API レスポンス: クラスタのメンバー 1 件（articles + feeds を JOIN した読み取り射影）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ClusterMember {
    pub cluster_id: Uuid,
    pub article_id: Uuid,
    pub title: String,
    pub url: String,
    pub feed_id: Uuid,
    pub feed_title: Option<String>,
    pub is_representative: bool,
    pub is_duplicate: bool,
    pub similarity: f32,
}

/// API レスポンス: クラスタ + メンバー群（GET /api/clusters の 1 要素）。
#[derive(Debug, Clone, Serialize)]
pub struct ClusterWithMembers {
    #[serde(flatten)]
    pub cluster: Cluster,
    pub members: Vec<ClusterMember>,
}
```

### 5.2 `repository.rs`（`&PgPool` を取る free async fn、ランタイムクエリのみ）

```rust
use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{Cluster, ClusterMember, ClusterMemberSource, MemberCandidate};
use crate::shared::error::AppResult;

/// 再計算対象（直近 hours 時間）の記事ノード。代表選定と signature 用。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArticleNode {
    pub id: Uuid,
    pub title: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<ArticleNode> for MemberCandidate {
    fn from(n: ArticleNode) -> Self {
        MemberCandidate { article_id: n.id, title: n.title, published_at: n.published_at }
    }
}

/// しきい値超えのタイトル類似ペア（辺）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClusterEdge {
    pub left_id: Uuid,
    pub right_id: Uuid,
    pub sim: f32,
}

/// 構築済みクラスタ（総入れ替えで INSERT する単位）。
#[derive(Debug, Clone)]
pub struct NewCluster {
    pub title: String,
    pub signature: String,
    pub members: Vec<NewMember>,
    /// signature 一致時に引き継ぐ旧キャッシュ（§5.3）。None なら未要約。
    pub carried_summary: Option<(String, String, String)>, // (summary, lang, model)
}

#[derive(Debug, Clone)]
pub struct NewMember {
    pub article_id: Uuid,
    pub is_representative: bool,
    pub is_duplicate: bool,
    pub similarity: f32,
}

/// 直近 hours 時間の記事ノードを読む（published_at が NULL なら created_at で代替）。
/// 件数は cap 件で打ち切り（O(n^2) ペア計算の暴発防止。§11）。
pub async fn recent_nodes(pool: &PgPool, hours: i32, cap: i32) -> AppResult<Vec<ArticleNode>> {
    let rows = sqlx::query_as::<_, ArticleNode>(
        r#"SELECT id, title, published_at
           FROM articles
           WHERE COALESCE(published_at, created_at) >= now() - make_interval(hours => $1)
             AND length(title) >= 3
           ORDER BY COALESCE(published_at, created_at) DESC
           LIMIT $2"#,
    )
    .bind(hours)
    .bind(cap)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 直近 hours 時間の記事どうしで、タイトル類似度が threshold 以上のペアを返す。
/// pg_trgm の similarity() を使い、a.id < b.id で重複ペアを排除。
/// `length(title) >= 3` は trigram が機能する最低長。cap で母集合を絞る（recent_nodes と同条件）。
pub async fn similarity_edges(
    pool: &PgPool,
    hours: i32,
    cap: i32,
    threshold: f32,
) -> AppResult<Vec<ClusterEdge>> {
    let rows = sqlx::query_as::<_, ClusterEdge>(
        r#"WITH recent AS (
               SELECT id, title
               FROM articles
               WHERE COALESCE(published_at, created_at) >= now() - make_interval(hours => $1)
                 AND length(title) >= 3
               ORDER BY COALESCE(published_at, created_at) DESC
               LIMIT $2
           )
           SELECT a.id AS left_id,
                  b.id AS right_id,
                  similarity(a.title, b.title) AS sim
           FROM recent a
           JOIN recent b ON a.id < b.id
           WHERE similarity(a.title, b.title) >= $3"#,
    )
    .bind(hours)
    .bind(cap)
    .bind(threshold)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 既存クラスタの (signature -> 旧キャッシュ要約) マップ。総入れ替え前に読む。
pub async fn existing_summaries(
    pool: &PgPool,
) -> AppResult<Vec<(String, Option<String>, Option<String>, Option<String>)>> {
    let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
        "SELECT signature, summary, summary_lang, summary_model FROM article_clusters",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// クラスタ表を総入れ替えする（1 トランザクション）。
/// DELETE article_clusters（CASCADE で members も消える）→ 各 NewCluster を INSERT。
pub async fn replace_clusters(pool: &PgPool, clusters: &[NewCluster]) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM article_clusters").execute(&mut *tx).await?;

    for c in clusters {
        let cluster_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO article_clusters
                   (title, size, signature, summary, summary_lang, summary_model, summarized_at)
               VALUES ($1, $2, $3, $4, $5, $6, CASE WHEN $4 IS NULL THEN NULL ELSE now() END)
               RETURNING id"#,
        )
        .bind(&c.title)
        .bind(c.members.len() as i32)
        .bind(&c.signature)
        .bind(c.carried_summary.as_ref().map(|s| s.0.clone()))
        .bind(c.carried_summary.as_ref().map(|s| s.1.clone()))
        .bind(c.carried_summary.as_ref().map(|s| s.2.clone()))
        .fetch_one(&mut *tx)
        .await?;

        for m in &c.members {
            sqlx::query(
                r#"INSERT INTO cluster_members
                       (cluster_id, article_id, is_representative, is_duplicate, similarity)
                   VALUES ($1, $2, $3, $4, $5)"#,
            )
            .bind(cluster_id)
            .bind(m.article_id)
            .bind(m.is_representative)
            .bind(m.is_duplicate)
            .bind(m.similarity)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

/// size >= min_size のクラスタを大きい順に返す。
pub async fn list_clusters(pool: &PgPool, min_size: i32) -> AppResult<Vec<Cluster>> {
    let rows = sqlx::query_as::<_, Cluster>(
        "SELECT id, title, size, summary, summary_lang, created_at
         FROM article_clusters
         WHERE size >= $1
         ORDER BY size DESC, created_at DESC",
    )
    .bind(min_size)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 指定クラスタ群のメンバーを articles + feeds を JOIN して返す（代表→類似度降順）。
pub async fn members_for(pool: &PgPool, cluster_ids: &[Uuid]) -> AppResult<Vec<ClusterMember>> {
    let rows = sqlx::query_as::<_, ClusterMember>(
        r#"SELECT cm.cluster_id,
                  cm.article_id,
                  a.title,
                  a.url,
                  a.feed_id,
                  f.title AS feed_title,
                  cm.is_representative,
                  cm.is_duplicate,
                  cm.similarity
           FROM cluster_members cm
           JOIN articles a ON a.id = cm.article_id
           JOIN feeds f ON f.id = a.feed_id
           WHERE cm.cluster_id = ANY($1)
           ORDER BY cm.is_representative DESC, cm.similarity DESC, a.title ASC"#,
    )
    .bind(cluster_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 単一クラスタを取得。
pub async fn get_cluster(pool: &PgPool, id: Uuid) -> AppResult<Option<Cluster>> {
    let row = sqlx::query_as::<_, Cluster>(
        "SELECT id, title, size, summary, summary_lang, created_at
         FROM article_clusters WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 統合要約の材料（メンバーの 媒体名 / タイトル / snippet）を読む（読み取り専用）。
pub async fn member_sources(pool: &PgPool, cluster_id: Uuid) -> AppResult<Vec<ClusterMemberSource>> {
    let rows = sqlx::query_as::<_, ClusterMemberSource>(
        r#"SELECT f.title AS feed_title,
                  a.title AS title,
                  COALESCE(NULLIF(a.summary, ''), LEFT(a.content, 600)) AS snippet
           FROM cluster_members cm
           JOIN articles a ON a.id = cm.article_id
           JOIN feeds f ON f.id = a.feed_id
           WHERE cm.cluster_id = $1
           ORDER BY cm.is_representative DESC"#,
    )
    .bind(cluster_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 生成した統合要約を保存（キャッシュ）。
pub async fn save_summary(
    pool: &PgPool,
    id: Uuid,
    summary: &str,
    lang: &str,
    model: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE article_clusters
           SET summary = $2, summary_lang = $3, summary_model = $4, summarized_at = now()
           WHERE id = $1"#,
    )
    .bind(id)
    .bind(summary)
    .bind(lang)
    .bind(model)
    .execute(pool)
    .await?;
    Ok(())
}
```

> **`articles`/`feeds` を読むことの正当化**: クラスタリングは記事横断の集約 read であり、`feed_overview`（feeds+articles JOIN read）や `instapaper::get_article_ref`（articles 読み取り）と同じ「読み取りのクロステーブル参照」。`articles`/`feeds` の **書き込み所有は移していない**ので越境共通レイヤーには当たらない。`query!` コンパイル時マクロは使わず `query`/`query_as`/`query_scalar` のみ。

### 5.3 `shared/llm` への `cluster_summary` メソッド追加（唯一の抽象境界を拡張）

`backend/src/shared/llm/mod.rs` に型と trait メソッドを **追記**:

```rust
#[derive(Debug, Clone)]
pub struct ClusterSummaryRequest {
    /// build_cluster_summary_input() が組み立てた各社の記事一覧。
    pub items: String,
    /// 出力言語（例 "ja"）。
    pub target_lang: String,
}

// LlmClient trait に 1 メソッド追加:
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn summarize(&self, req: SummarizeRequest) -> AppResult<String>;
    async fn translate(&self, req: TranslateRequest) -> AppResult<String>;
    async fn cluster_summary(&self, req: ClusterSummaryRequest) -> AppResult<String>; // ← 追加
}
```

`backend/src/shared/llm/anthropic.rs` の `impl LlmClient for AnthropicClient` に **追記**（既存 private `complete(system, user)` を再利用）:

```rust
async fn cluster_summary(&self, req: ClusterSummaryRequest) -> AppResult<String> {
    let system = format!(
        "You are a news analyst. The following articles from different outlets \
         cover the SAME story. Write an integrated summary in {} that (1) states \
         the shared facts in 2-3 sentences, then (2) explicitly contrasts how the \
         outlets differ in framing, emphasis, or tone (use a short bulleted list, \
         naming each outlet). Be concise and neutral. Output only the summary.",
        req.target_lang
    );
    self.complete(&system, &req.items).await
}
```

> trait は **新設しない**。既存の唯一の境界 `LlmClient` に `cluster_summary` を足すだけ（要約/翻訳と同型）。CLAUDE.md「抽象境界は `shared/llm` のみ」に整合する拡張。`complete` の `max_tokens` は現状 1024 固定で、メンバーが多いクラスタの統合要約は切り詰められうる（§11 に緩和策）。

### 5.4 `service.rs`（`&AppState` を取り repository + 純粋ロジック + LLM を統合）

```rust
use std::collections::HashMap;

use uuid::Uuid;

use super::domain::{
    build_cluster_summary_input, classify_similarity, cluster_signature, group_edges,
    normalize_title, pick_representative, Cluster, ClusterWithMembers, MemberCandidate,
    SimilarityBand,
};
use super::repository::{self, ClusterEdge, NewCluster, NewMember};
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{ClusterSummaryRequest, LlmClient};
use crate::shared::state::AppState;

/// config から Anthropic クライアントを作る。未設定なら NotEnabled
/// （articles/service.rs::llm_client と同型）。
fn llm_client(state: &AppState) -> AppResult<AnthropicClient> {
    let key = state
        .config
        .anthropic_api_key
        .clone()
        .ok_or_else(|| AppError::NotEnabled("ANTHROPIC_API_KEY is not set".into()))?;
    Ok(AnthropicClient::new(
        state.http.clone(),
        key,
        state.config.anthropic_model.clone(),
    ))
}

/// 再クラスタリング: 直近窓の記事を読み、pg_trgm 類似度 + ユニオンファインドで
/// グループ化してクラスタ表を総入れ替えする。Claude は呼ばない（費用ゼロ・冪等）。
/// 返り値: 永続化したクラスタ数。
pub async fn recluster(state: &AppState) -> AppResult<usize> {
    let cfg = &state.config;
    let hours = cfg.clustering_window_hours;
    let cap = cfg.clustering_max_articles;

    let nodes = repository::recent_nodes(&state.db, hours, cap).await?;
    if nodes.is_empty() {
        repository::replace_clusters(&state.db, &[]).await?;
        return Ok(0);
    }

    // pg_trgm 類似度の辺。正規化タイトルではなく素のタイトルで計算する点に注意:
    // similarity() は内部で小文字化等を行うため、normalize_title は主に
    // 代表選定/表示の安定化に使う（DB 側 similarity の入力は素の title）。
    let edges = repository::similarity_edges(&state.db, hours, cap, cfg.cluster_topic_threshold).await?;

    // sim 参照用マップ（無向）。
    let mut sim_map: HashMap<(Uuid, Uuid), f32> = HashMap::new();
    for ClusterEdge { left_id, right_id, sim } in &edges {
        sim_map.insert(ordered(*left_id, *right_id), *sim);
    }

    let node_ids: Vec<Uuid> = nodes.iter().map(|n| n.id).collect();
    let edge_tuples: Vec<(Uuid, Uuid, f32)> =
        edges.iter().map(|e| (e.left_id, e.right_id, e.sim)).collect();
    let groups = group_edges(&node_ids, &edge_tuples, cfg.cluster_topic_threshold);

    // 旧キャッシュ要約（signature -> (summary, lang, model)）。総入れ替え前に読む。
    let carried: HashMap<String, (String, String, String)> = repository::existing_summaries(&state.db)
        .await?
        .into_iter()
        .filter_map(|(sig, s, l, m)| match (s, l, m) {
            (Some(s), Some(l), Some(m)) => Some((sig, (s, l, m))),
            _ => None,
        })
        .collect();

    let by_id: HashMap<Uuid, &super::repository::ArticleNode> =
        nodes.iter().map(|n| (n.id, n)).collect();

    let mut new_clusters: Vec<NewCluster> = Vec::new();
    for group in groups {
        if (group.len() as i32) < cfg.cluster_min_size {
            continue; // 単独/小クラスタは保存しない（GET の min_size と一致）
        }
        let candidates: Vec<MemberCandidate> = group
            .iter()
            .filter_map(|id| by_id.get(id).map(|n| MemberCandidate {
                article_id: n.id,
                title: n.title.clone(),
                published_at: n.published_at,
            }))
            .collect();
        let rep_id = pick_representative(&candidates);
        let rep_title = by_id.get(&rep_id).map(|n| normalize_title(&n.title)).unwrap_or_default();

        let members: Vec<NewMember> = group
            .iter()
            .map(|&aid| {
                let sim = if aid == rep_id {
                    1.0
                } else {
                    *sim_map.get(&ordered(aid, rep_id)).unwrap_or(&0.0)
                };
                let is_dup = matches!(
                    classify_similarity(sim, cfg.cluster_topic_threshold, cfg.cluster_dup_threshold),
                    SimilarityBand::Duplicate
                );
                NewMember {
                    article_id: aid,
                    is_representative: aid == rep_id,
                    is_duplicate: is_dup && aid != rep_id,
                    similarity: sim,
                }
            })
            .collect();

        let signature = cluster_signature(&group);
        let carried_summary = carried.get(&signature).cloned();

        new_clusters.push(NewCluster {
            title: rep_title,
            signature,
            members,
            carried_summary,
        });
    }

    let count = new_clusters.len();
    repository::replace_clusters(&state.db, &new_clusters).await?;
    Ok(count)
}

fn ordered(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b { (a, b) } else { (b, a) }
}

/// size >= min_size のクラスタをメンバー込みで返す（カード表示用）。
pub async fn list_clusters(state: &AppState) -> AppResult<Vec<ClusterWithMembers>> {
    let clusters = repository::list_clusters(&state.db, state.config.cluster_min_size).await?;
    if clusters.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<Uuid> = clusters.iter().map(|c| c.id).collect();
    let members = repository::members_for(&state.db, &ids).await?;

    let mut by_cluster: HashMap<Uuid, Vec<_>> = HashMap::new();
    for m in members {
        by_cluster.entry(m.cluster_id).or_default().push(m);
    }
    Ok(clusters
        .into_iter()
        .map(|c| {
            let members = by_cluster.remove(&c.id).unwrap_or_default();
            ClusterWithMembers { cluster: c, members }
        })
        .collect())
}

/// 単一クラスタをメンバー込みで返す。
pub async fn get_cluster(state: &AppState, id: Uuid) -> AppResult<ClusterWithMembers> {
    let cluster = repository::get_cluster(&state.db, id).await?.ok_or(AppError::NotFound)?;
    let members = repository::members_for(&state.db, &[id]).await?;
    Ok(ClusterWithMembers { cluster, members })
}

/// クラスタの統合要約を生成（or キャッシュ返却）。
/// 順序: (1) クラスタ存在? なければ NotFound、(2) 同一 lang キャッシュ命中なら返す、
///       (3) APIキー有り? なければ NotEnabled、(4) Claude で生成して保存。
pub async fn summarize_cluster(
    state: &AppState,
    id: Uuid,
    target_lang: &str,
) -> AppResult<Cluster> {
    let cluster = repository::get_cluster(&state.db, id).await?.ok_or(AppError::NotFound)?;

    // 同一言語のキャッシュ命中 → 再課金しない。
    if let (Some(s), Some(l)) = (&cluster.summary, &cluster.summary_lang) {
        if l == target_lang && !s.is_empty() {
            return Ok(cluster);
        }
    }

    let client = llm_client(state)?; // 未設定なら NotEnabled
    let sources = repository::member_sources(&state.db, id).await?;
    let items = build_cluster_summary_input(&sources);
    let summary = client
        .cluster_summary(ClusterSummaryRequest {
            items,
            target_lang: target_lang.to_string(),
        })
        .await?;

    repository::save_summary(&state.db, id, &summary, target_lang, &state.config.anthropic_model)
        .await?;
    repository::get_cluster(&state.db, id).await?.ok_or(AppError::NotFound)
}
```

> `recluster` は handler（手動）と scheduler（§5.6）の両方から呼ばれるので `-D warnings` でも未使用にならない。HTTP 呼び出しは `AnthropicClient`（trait 実装）に閉じ、本スライスに新しい trait/dyn は足さない。グループ化・代表選定・signature・バンド分けはすべて `domain.rs` の純粋関数に委譲し、`service.rs` は I/O と組み立てに専念する。

### 5.5 `handler.rs`（axum ハンドラ）

```rust
use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{Cluster, ClusterWithMembers};
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<ClusterWithMembers>>> {
    Ok(Json(service::list_clusters(&state).await?))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ClusterWithMembers>> {
    Ok(Json(service::get_cluster(&state, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct SummaryBody {
    /// 省略時は config の既定（cluster_summary_lang）。
    pub target_lang: Option<String>,
}

pub async fn summarize(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SummaryBody>,
) -> AppResult<Json<Cluster>> {
    let lang = body
        .target_lang
        .unwrap_or_else(|| state.config.cluster_summary_lang.clone());
    Ok(Json(service::summarize_cluster(&state, id, &lang).await?))
}

#[derive(serde::Serialize)]
pub struct ReclusterResult {
    pub clusters: usize,
}

pub async fn recluster(State(state): State<AppState>) -> AppResult<Json<ReclusterResult>> {
    let clusters = service::recluster(&state).await?;
    Ok(Json(ReclusterResult { clusters }))
}
```

> **`POST /api/clusters/{id}/summary` のボディ任意化**: `Json<SummaryBody>` で `target_lang` が省略可能。ボディ無しの POST でも `Json` 抽出が空ボディで失敗しないよう、フロントは常に `{}` を送る（§6.1）。空ボディ対応が必要なら `Option<Json<SummaryBody>>` に変えてもよい（既存 handler の慣習に合わせる）。

### 5.6 `mod.rs`（routes）と scheduler 起動

`backend/src/features/clustering/mod.rs`:

```rust
pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/clusters", get(handler::list))
        .route("/api/clusters/recluster", post(handler::recluster))
        .route("/api/clusters/{id}", get(handler::get_one))
        .route("/api/clusters/{id}/summary", post(handler::summarize))
}
```

> ルート順は axum 0.8 のパスマッチング上、静的セグメント `/recluster` を `/{id}` より **先**に登録しておくと安全（`recluster` が UUID パスに飲まれないように）。axum 0.8 はパス記法が `{id}`（旧 `:id` ではない）。既存スライスのルート記法に合わせること。

`backend/src/shared/scheduler.rs` に **再クラスタリングループを追記**（既存 `spawn` はそのまま）:

```rust
/// Re-clustering loop. Periodically rebuilds the cluster tables from the recent
/// window. Trigram-only (no LLM call), so it is cheap and safe to run often.
pub fn spawn_clustering(state: AppState) {
    if !state.config.clustering_enabled {
        tracing::info!("clustering disabled (CLUSTERING_ENABLED is not true)");
        return;
    }
    let period = Duration::from_secs(state.config.clustering_interval_secs);
    tokio::spawn(async move {
        let mut ticker = interval(period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        ticker.tick().await; // skip immediate first tick
        loop {
            ticker.tick().await;
            tracing::info!("scheduled re-clustering starting");
            match crate::features::clustering::service::recluster(&state).await {
                Ok(n) => tracing::info!(clusters = n, "re-clustering done"),
                Err(e) => tracing::error!(error = %e, "re-clustering failed"),
            }
        }
    });
}
```

`backend/src/main.rs` に **1 行追加**（`scheduler::spawn(state.clone());` の直後）:

```rust
scheduler::spawn_clustering(state.clone());
```

`backend/src/features/mod.rs` に **2 行追加**:

```rust
pub mod clustering; // 既存 pub mod 群に追加
// router() の .merge チェーンに追加:
        .merge(clustering::routes())
```

既存スライス（feeds/articles/digest/...）は一切触らない。触れるのは横断インフラ（`shared/llm`・`shared/scheduler`・`shared/config`）と合成点（`features/mod.rs`・`main.rs`）のみ。

### 5.8 `config.rs` への追加（env マッピング）

`AppConfig` に追記（既存フィールドの並びに足す）:

```rust
    /// 再クラスタリングの定期実行を有効化するか。
    pub clustering_enabled: bool,
    /// 再クラスタリング間隔（秒）。既定 3600。
    pub clustering_interval_secs: u64,
    /// クラスタ対象とする遡及窓（時間）。既定 48。
    pub clustering_window_hours: i32,
    /// 1 回の再計算で対象にする最大記事数（O(n^2) 暴発防止）。既定 500。
    pub clustering_max_articles: i32,
    /// 同一話題とみなす最小 trigram 類似度。既定 0.3。
    pub cluster_topic_threshold: f32,
    /// 重複（dedup）とみなす類似度。既定 0.6（topic_threshold 以上であること）。
    pub cluster_dup_threshold: f32,
    /// GET /api/clusters で表示する最小メンバー数。既定 2（単独記事は出さない）。
    pub cluster_min_size: i32,
    /// 統合要約の既定出力言語。既定 "ja"。
    pub cluster_summary_lang: String,
```

`from_env` に追記:

```rust
        let clustering_enabled = std::env::var("CLUSTERING_ENABLED")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let clustering_interval_secs = std::env::var("CLUSTERING_INTERVAL_SECS")
            .ok().and_then(|v| v.parse().ok()).unwrap_or(3600);
        let clustering_window_hours = std::env::var("CLUSTERING_WINDOW_HOURS")
            .ok().and_then(|v| v.parse().ok()).unwrap_or(48);
        let clustering_max_articles = std::env::var("CLUSTERING_MAX_ARTICLES")
            .ok().and_then(|v| v.parse().ok()).unwrap_or(500);
        let cluster_topic_threshold = std::env::var("CLUSTER_TOPIC_THRESHOLD")
            .ok().and_then(|v| v.parse().ok()).unwrap_or(0.3_f32);
        let cluster_dup_threshold = std::env::var("CLUSTER_DUP_THRESHOLD")
            .ok().and_then(|v| v.parse().ok()).unwrap_or(0.6_f32);
        let cluster_min_size = std::env::var("CLUSTER_MIN_SIZE")
            .ok().and_then(|v| v.parse().ok()).unwrap_or(2);
        let cluster_summary_lang =
            std::env::var("CLUSTER_SUMMARY_LANG").unwrap_or_else(|_| "ja".to_string());
```

（`Self { ... }` の構築にも各フィールドを追加すること。）`.env.example` にも同名キーを追記する。`anthropic_api_key`/`anthropic_model` は既存フィールドを再利用。

### 5.9 AppError の使い分け（`error.rs` は不編集）

| 状況 | バリアント | HTTP | レスポンス `error` 文字列（Display） |
|---|---|---|---|
| `GET /clusters/{id}` / `summary` でクラスタが無い | `NotFound` | 404 | `resource not found` |
| `summary` 時に `ANTHROPIC_API_KEY` 未設定 | `NotEnabled` | 503 | `feature not yet enabled: ANTHROPIC_API_KEY is not set` |
| Claude API 障害・非 2xx | `Upstream` | 502 | `upstream request failed: anthropic 5xx: ...` |
| DB エラー | `Database`（`?` で自動 `From`） | 500 | `internal error` |

> `summarize_cluster` のチェック順序は **(1) クラスタ存在 → (2) キャッシュ命中 → (3) APIキー → (4) 生成**。存在しないクラスタは APIキー判定より先に 404（無意味な機能ゲートを避ける）。`recluster`/`list`/`get_one` は Claude を呼ばないので `NotEnabled` を返さない（APIキー無しでも動く）。新バリアントは追加しない。

---

## 6. フロントエンド設計

### 6.1 `lib/api.ts` への追加（型 3 + メソッド 4）

型を追加（backend JSON をミラー）:

```ts
export interface Cluster {
  id: string;
  title: string;
  size: number;
  summary: string | null;
  summary_lang: string | null;
  created_at: string;
}

export interface ClusterMember {
  cluster_id: string;
  article_id: string;
  title: string;
  url: string;
  feed_id: string;
  feed_title: string | null;
  is_representative: boolean;
  is_duplicate: boolean;
  similarity: number;
}

export interface ClusterWithMembers extends Cluster {
  members: ClusterMember[];
}
```

`api` オブジェクトにメソッド追加（既存 `http<T>()` を再利用、命名は `動詞+リソース`）:

```ts
  listClusters: () => http<ClusterWithMembers[]>("/api/clusters"),
  getCluster: (id: string) => http<ClusterWithMembers>(`/api/clusters/${id}`),
  // ボディは常に {} を送る（target_lang 省略時はサーバ既定 "ja"）。
  summarizeCluster: (id: string, targetLang?: string) =>
    http<Cluster>(`/api/clusters/${id}/summary`, {
      method: "POST",
      body: JSON.stringify(targetLang ? { target_lang: targetLang } : {}),
    }),
  reclusterNow: () =>
    http<{ clusters: number }>("/api/clusters/recluster", { method: "POST" }),
```

> 統合要約の 503（APIキー未設定）/502（障害）は `http<T>()` が throw する。`Clusters.tsx` 側で `errorStatus(e)`（`api.ts` の既存ヘルパ）で振り分け、メッセージ表示する。

### 6.2 統合要約のレンダリング

統合要約はプレーンテキスト（先頭 2-3 文 + 箇条書き）。**新規依存を増やさず**、記事要約と同様に `prose` + `whitespace-pre-wrap` で表示する（Markdown パーサ `marked` は使わない）。LLM 出力をそのまま `innerHTML` に入れず、**テキストノードとして描画**する（`<p class="whitespace-pre-wrap">{cluster.summary}</p>`）。これで XSS 経路を作らない（記事本文と違い HTML 化しないので sanitize 不要）。

### 6.3 新規ルート `routes/Clusters.tsx`

クラスタをカード表示。状態は **ローカル**（`createResource`）。グローバルストア変更は不要。

骨子:
- `const [clusters, { refetch }] = createResource(api.listClusters);`
- ヘッダ: 「話題のまとまり」見出し + `button.tsx`「再計算」（`await api.reclusterNow()` → `refetch()`。実行中 disabled）。Claude を呼ばないので APIキー不要。
- 一覧: `<For each={clusters()}>` で各クラスタを `card.tsx` 1 枚:
  - 代表タイトル（`cluster.title`、`text-base font-semibold`）。
  - `badge.tsx` で「{size} 件」、媒体数（`new Set(members.map(m => m.feed_id)).size` 件の媒体）。
  - メンバー一覧（`<For each={cluster.members}>`）: 媒体名（`feed_title`）+ タイトルの外部リンク（`<a href={m.url} target="_blank" rel="noreferrer">`）。`m.is_representative` なら「代表」バッジ、`m.is_duplicate` なら「重複」バッジ（`badge.tsx` の variant 差で表現）。
  - 「統合要約」`button.tsx`: クリックで `const updated = await api.summarizeCluster(cluster.id)` → 取得した `summary` をローカル signal（`Record<clusterId, string>`）に格納して表示。実行中は per-cluster の `busy` で disabled。`errorStatus(e) === 503` なら「Claude API キーが未設定です」、502 なら「要約生成に失敗しました」を表示。
  - 要約が既にある（`cluster.summary` が非 null）クラスタは初期表示で `prose` に出す（キャッシュ済み）。
- 空状態: `clusters()?.length === 0` のとき「まとまった話題はまだありません。『再計算』で作成できます」。

### 6.4 ルーティング `index.tsx`

既存 `<Router>` 内に 1 ルート追加:

```tsx
import Clusters from "./routes/Clusters";
// ...
<Route path="/clusters" component={Clusters} />
```

ナビ導線（Sidebar/ヘッダのリンク）は二ペインレイアウト（機能 10）の置き場所に 1 リンク足すか、暫定で `App.tsx` ヘッダに `/clusters` リンクを 1 つ足す（任意）。`/clusters` を直接開けば使える状態であれば足りる。

### 6.5 Ark UI について

本機能で必要な UI は card / button / badge / リンク / `prose` 表示のみで、いずれも自前 Tailwind + 既存部品で賄える。**Ark UI 部品は不要**。

---

## 7. API 契約

> すべて `/api` プレフィックス。`{id}` はクラスタ UUID。

### 7.1 `GET /api/clusters` — クラスタ一覧（メンバー込み）
レスポンス（200、`size >= cluster_min_size` のクラスタを大きい順）:
```json
[
  {
    "id": "9b1c...",
    "title": "中央銀行が利上げを発表",
    "size": 4,
    "summary": null,
    "summary_lang": null,
    "created_at": "2026-06-30T12:00:03Z",
    "members": [
      {
        "cluster_id": "9b1c...",
        "article_id": "1f1c...",
        "title": "中央銀行、0.25%の利上げを発表",
        "url": "https://a.example.com/x",
        "feed_id": "aaaa...",
        "feed_title": "A 経済新聞",
        "is_representative": true,
        "is_duplicate": false,
        "similarity": 1.0
      },
      {
        "cluster_id": "9b1c...",
        "article_id": "2f2c...",
        "title": "中央銀行が利上げ、0.25%引き上げ",
        "url": "https://b.example.com/y",
        "feed_id": "bbbb...",
        "feed_title": "B 通信",
        "is_representative": false,
        "is_duplicate": true,
        "similarity": 0.71
      }
    ]
  }
]
```
クラスタが無ければ `[]`（空配列、200）。

### 7.2 `GET /api/clusters/{id}` — 単一クラスタ
レスポンス（200）: 7.1 の 1 要素と同形。
エラー:
- 404 `{ "error": "resource not found" }`（id 不在）

### 7.3 `POST /api/clusters/{id}/summary` — 各社論調差の統合要約（Claude）
リクエスト（任意、省略時はサーバ既定言語）:
```json
{ "target_lang": "ja" }
```
レスポンス（200、生成 or キャッシュ）:
```json
{
  "id": "9b1c...",
  "title": "中央銀行が利上げを発表",
  "size": 4,
  "summary": "各社とも0.25%の利上げを報じた。\n- A 経済新聞: 物価抑制を強調\n- B 通信: 住宅ローンへの影響を前面に\n...",
  "summary_lang": "ja",
  "created_at": "2026-06-30T12:00:03Z"
}
```
エラー:
- 404 `{ "error": "resource not found" }`（id 不在）
- 503 `{ "error": "feature not yet enabled: ANTHROPIC_API_KEY is not set" }`（APIキー未設定）
- 502 `{ "error": "upstream request failed: anthropic 500 ..." }`（Claude 障害）

### 7.4 `POST /api/clusters/recluster` — 即時再計算（Claude 不使用）
リクエスト: ボディ無し
レスポンス（200）:
```json
{ "clusters": 7 }
```
> APIキー不要（トリグラムのみ）。総入れ替え後の永続クラスタ数を返す（`cluster_min_size` 未満は除外後の数）。

---

## 8. 依存関係

- **本機能が依存する機能**: 機能上の **ハード依存は無い**（`clustering` スライスは自己完結）。読み取りで `articles`/`feeds` テーブルを参照するが、これは既存。`pg_trgm` 拡張は `0005_search.sql` で導入済み（0006 でも防御的に再宣言するため、0005 適用前でも単独で動く）。横断インフラ（`shared/llm`・`shared/scheduler`・`shared/config`）を拡張する。
- **ソフトな協調**:
  - 機能 10（二ペイン）/ ナビ: `/clusters` への導線をナビに足せると良い（無くても直接 URL で動く）。
  - 機能 04（ダークテーマ）: `prose dark:prose-invert` / oklch トークンで整合（追加作業不要）。
  - 記事要約（`articles`）: 記事に `summary` があれば統合要約の材料 `snippet` に優先採用される（無くても本文先頭で代替）。
  - 全文検索（`0005`）: `pg_trgm` 基盤を共有する（インデックスも流用される）。
- **本機能をブロックする機能**: 無し。
- 既存スライスへの変更は無し。接触点は `features/mod.rs`（2 行）・`main.rs`（1 行）・`shared/scheduler.rs`（ループ追記）・`shared/llm/{mod,anthropic}.rs`（`cluster_summary` メソッド追記）・`shared/config.rs`（env 追記）。

---

## 9. テスト計画（TDD）

> 配置方針は既存前例に合わせる: 純粋ロジックは各 `.rs` の `#[cfg(test)] mod tests`、DB を触る往復は `repository.rs` 内の `#[ignore]` テスト（binary crate で `lib.rs` 無しのため `backend/tests/` から内部関数は呼べない）、HTTP 表面は shell スクリプト。本機能は **クラスタ化アルゴリズムを純粋関数（`group_edges` 等）に切り出してある**ので、DB/Claude 無しで中核ロジックを網羅できる。

### 9.1 単体テスト（`#[cfg(test)] mod tests` in `domain.rs`、外部 API も DB も不要）

| テスト | 意図 |
|---|---|
| `normalize_title_lowercases_and_collapses_ws` | `"  Hello   World "` → `"hello world"` |
| `normalize_title_handles_japanese` | 日本語タイトルが壊れず（小文字化はそのまま）連続空白だけ畳まれる |
| `classify_similarity_bands` | `0.7,0.3,0.6` → `Duplicate` / `0.4,0.3,0.6` → `SameTopic` / `0.1,...` → `Unrelated`、境界（`==threshold`）が下側バンドに入ること |
| `group_edges_single_component` | 3 ノード a-b, b-c の辺で 1 クラスタ（a,b,c）にまとまる（推移的連結） |
| `group_edges_two_components` | a-b と c-d の 2 群に分かれる |
| `group_edges_singletons` | 辺が無ければ各ノードが単独クラスタ（nodes 入力順を保持） |
| `group_edges_ignores_below_threshold` | `sim < threshold` の辺は無視される（別クラスタのまま） |
| `group_edges_ignores_unknown_nodes` | nodes に無い id を指す辺は無視（パニックしない） |
| `pick_representative_prefers_oldest_published` | published_at 最古を代表に選ぶ |
| `pick_representative_null_published_last` | 全員 NULL ならタイトル長 → uuid で決定的に選ぶ |
| `pick_representative_tiebreak_longest_title` | published_at 同点でタイトルが長い方を選ぶ |
| `cluster_signature_order_independent` | 同じ id 集合なら順序が違っても同一 signature |
| `cluster_signature_distinct_sets_differ` | 異なる集合は異なる signature |
| `build_cluster_summary_input_formats_lines` | 2 メンバーが `- 【媒体】タイトル: 抜粋` の 2 行に整形、`feed_title` NULL は `(unknown source)` |

### 9.2 リポジトリ往復テスト（`#[cfg(test)] mod tests` in `repository.rs`、実 DB / `#[ignore]`）

`DATABASE_URL`（`just dev-db` の DB、マイグレーション適用済み）で実 DB に接続。`#[tokio::test]` + `#[ignore]`。`cargo test -- --ignored` で実行。テスト用に `feeds`/`articles` を最小挿入してから検証し、後片付けする（または専用 feed を使う）。

雛形:
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
    #[ignore = "requires a running Postgres (DATABASE_URL) with migrations applied"]
    async fn replace_and_list_roundtrip() {
        let pool = pool().await;
        // 前提: feeds/articles に既知の 2 記事 (a1, a2, 同一 feed) を用意済みとする。
        // ここでは構造のみ示す（実テストは挿入→検証→クリーンアップ）。
        let a1 = /* 既存 article id */ Uuid::nil();
        let a2 = /* 既存 article id */ Uuid::nil();

        let cluster = NewCluster {
            title: "rep title".into(),
            signature: "sig-test".into(),
            members: vec![
                NewMember { article_id: a1, is_representative: true, is_duplicate: false, similarity: 1.0 },
                NewMember { article_id: a2, is_representative: false, is_duplicate: true, similarity: 0.7 },
            ],
            carried_summary: None,
        };
        replace_clusters(&pool, std::slice::from_ref(&cluster)).await.unwrap();

        let listed = list_clusters(&pool, 2).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].size, 2);

        let members = members_for(&pool, &[listed[0].id]).await.unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.iter().any(|m| m.is_representative));

        // 総入れ替え（空）で消える
        replace_clusters(&pool, &[]).await.unwrap();
        assert!(list_clusters(&pool, 2).await.unwrap().is_empty());
    }
}
```

| テスト | 意図 |
|---|---|
| `replace_and_list_roundtrip` | `replace_clusters`（INSERT）→ `list_clusters`（size フィルタ）→ `members_for`（JOIN）→ 空入れ替えで消える、を network 抜きで自動カバー |
| `similarity_edges_finds_near_duplicates`（任意） | 似たタイトルの記事 2 件を挿入し、`similarity_edges` がしきい値超えで辺を返すことを検証（pg_trgm の動作確認・後片付け込み） |
| `replace_clusters_below_min_size_excluded_by_list`（任意） | size=1 のクラスタは `list_clusters(_, 2)` で返らない |

### 9.3 HTTP スモークテスト（稼働スタックへの shell スクリプト）

`scripts/test/api-clustering.sh` を新設（`scripts/test/api-*.sh` と同型。nginx 経由）。**Claude を叩かない範囲**を決定的に検証:

| 手順 / アサーション | 意図 |
|---|---|
| `POST /api/clusters/recluster` → 200 かつ JSON に `clusters` キー | 再計算配線（Claude 不使用なので APIキー無しでも 200） |
| `GET /api/clusters` → 200 かつ配列（`[]` 可） | スライス合成 + 一覧経路 |
| `GET /api/clusters/00000000-0000-0000-0000-000000000000` → 404 | NotFound 配線 |
| `ANTHROPIC_API_KEY` 未設定環境で `POST /api/clusters/{存在するid}/summary` → 503、存在しない id なら 404 | 機能ゲート（クラスタ存在 → APIキー）の順序確認 |

> `POST /api/clusters/{id}/summary` の成功パス（実 Claude 呼び出し）は CI 自動化しない（ライブ APIキーが必要）。手動手順は §10 step 11。

### 9.4 フロント（手動 + 型）
- `tsc`（`just lint`）で `api.ts` / `Clusters.tsx` の型整合を確認。
- 手動: `/clusters` を開く → 「再計算」でクラスタ生成 → 似た記事が 1 カードにまとまる/重複バッジ表示 → 「統合要約」で各社論調差の要約が `prose` 表示、再クリックでキャッシュ即返、ダークでも可読。

---

## 10. 実装手順（順序付きチェックリスト）

1. **マイグレーション採番**: `ls backend/migrations/` で最大番号を確認（現状 `0005_search.sql`）。`0006_clustering.sql` を §4.2 の SQL で新規作成（既存は触らない）。並行機能が `0006` を取っていれば繰り上げ。
2. **shared/llm 拡張（Red 先行可）**: `shared/llm/mod.rs` に `ClusterSummaryRequest` と trait メソッド `cluster_summary` を追加、`anthropic.rs` に実装（§5.3）。`complete` 再利用。
3. **config 拡張**: `shared/config.rs` に clustering 関連フィールドと `from_env` 解析を追加（§5.8）。`.env.example` も更新。
4. **ドメイン（Red 先行）**: `features/clustering/domain.rs` を §5.1 で作成 + §9.1 の `#[cfg(test)] mod tests`。落ちる→実装で Green（特に `group_edges` のユニオンファインドを先にテスト）。`cargo test` で実行。
5. **repository**: `repository.rs` を §5.2（`query`/`query_as`/`query_scalar` のみ、トランザクションは `pool.begin()`）。§9.2 の `#[ignore]` テストも書く。
6. **service**: `service.rs` を §5.4。`recluster`（純粋関数へ委譲）・`list_clusters`・`get_cluster`・`summarize_cluster`（キャッシュ → NotEnabled）。
7. **handler + mod + 合成**: `handler.rs`（§5.5）、`mod.rs`（§5.6、ルート順注意）。`features/mod.rs` に `pub mod clustering;` と `.merge(clustering::routes())`。`shared/scheduler.rs` に `spawn_clustering`、`main.rs` に `scheduler::spawn_clustering(state.clone());`（§5.6）。
8. **ビルド & lint**: `just lint`（clippy `-D warnings` / tsc）。Cargo.toml 変更は不要の見込み。
9. **DB & テスト**: `just dev-db` → 起動で自動 migrate（または `just migrate`）→ `cargo test`（単体）→ `DATABASE_URL=... cargo test -- --ignored`（往復）。`scripts/test/api-clustering.sh` を作成・`chmod +x`・実行。
10. **再計算の動作確認**: `CLUSTERING_ENABLED=true` + 短い `CLUSTERING_INTERVAL_SECS` で起動し、ログに `re-clustering done clusters=N`。または `POST /api/clusters/recluster` を手動。`GET /api/clusters` でカードを確認。
11. **手動 E2E（要約）**: `ANTHROPIC_API_KEY` を設定して起動 → `POST /api/clusters/{id}/summary` → `summary` に各社論調差が入ることを確認。再実行でキャッシュ即返（同一言語）。
12. **フロント**: `lib/api.ts`（型 3 + メソッド 4、§6.1）、`routes/Clusters.tsx`（§6.3）、`index.tsx` にルート、任意でナビリンク。`just lint` の tsc を通す。
13. **コミット**: マイグレーション・スライス・shared 拡張・スクリプト・フロントをまとめて。`.env`/秘密はコミットしない。

---

## 11. リスク・未決事項・代替案

- **タイトルのみの類似度は精度が限定的**: MVP は **タイトルのトリグラム類似度**のみ。見出しの言い回しが大きく異なる同一話題は取りこぼし、無関係でも語が被ると誤結合しうる。**緩和策**: `cluster_topic_threshold` を運用で調整（既定 0.3 は緩め）。将来は (a) 本文の先頭も類似度に加味、(b) pgvector による埋め込み類似度へ拡張（`similarity_edges` の SQL を差し替えるだけで `group_edges` 以降は不変）。本書のアルゴリズム境界（SQL=類似度、Rust=グループ化）はこの差し替えを容易にする設計。
- **O(n^2) ペア計算のコスト**: `similarity_edges` は窓内記事の自己結合。`clustering_max_articles`（既定 500）で母集合を cap し、`similarity() >= threshold` で early filter。500 件なら ~12.5 万ペアで家庭内 PostgreSQL なら問題なし。さらに重ければ `pg_trgm` の `%` 演算子 + `gin_trgm_ops` インデックス（0005 で title に作成済み）で候補を絞る形に変更できる（`WHERE a.title % b.title`、`set_limit()` でしきい値設定）。本書は決定的な明示しきい値版を既定とする。
- **クラスタ ID の不安定性と要約キャッシュ無効化**: 再計算は総入れ替えなのでクラスタ `id` は毎回変わる。要約は `id` ではなく **メンバー集合の `signature`** に紐づけて引き継ぐ（§5.3）。メンバーが 1 件でも増減すると signature が変わり要約は破棄され、次回要求で再生成される（=コスト発生）。これは意図的（メンバーが変われば論調統合もやり直すべき）。頻繁な記事流入で signature が揺れ続けるなら、要約対象を「サイズが安定した（is_duplicate 主体の）クラスタ」に限る運用も可。
- **`max_tokens` による統合要約の切り詰め**: `anthropic.rs::complete` は `max_tokens:1024` 固定。メンバーが多いクラスタは要約が途中で切れうる。**緩和策**: `complete` を `complete_with(system, user, max_tokens)` に小改修し、`cluster_summary` だけ 2048 を渡す（既存 `summarize`/`translate` は 1024 のまま）。あるいは材料の `snippet` 長（`LEFT(content, 600)`）やメンバー数を絞る。MVP は 1024 で開始し運用で調整。
- **`shared/llm` trait 拡張の影響**: `LlmClient` にメソッドを足すと、もし他にモック実装があれば追従が必要。現状 trait 実装は `AnthropicClient` のみ（テストモックは未存在）なので影響なし。新規モックを作る場合は `cluster_summary` も実装すること。
- **スケジューラのオーバーラップ**: `spawn_clustering` は `tokio::interval` 単一ループで、前回の `recluster` 完了後に次 tick が走る（並行実行しない）。総入れ替えはトランザクションなので、`GET /api/clusters` が再計算中に読んでも一貫したスナップショット（旧 or 新）を返す。`MissedTickBehavior::Skip` で遅延時の tick 積み上がりを防ぐ。
- **重複（dedup）判定の境界**: メンバーの `is_duplicate` は「代表への類似度 >= `cluster_dup_threshold`」で決める。代表と直接の辺が無い（推移的にのみ連結した）メンバーは `sim_map` に値が無く `similarity=0.0` 扱いになり、重複判定されない。これは UI 表示の品質問題であり機能破壊ではない。厳密化するなら代表タイトルと各メンバーの `similarity()` を追加 SQL で取得する（`member_sources` と同経路で 1 クエリ追加）。MVP は辺の値で近似する。
- **`gen_random_uuid()` の利用可否**: PostgreSQL 17 はコアに `gen_random_uuid()` を持つ（拡張不要）。`0001_init.sql` の慣習に合わせること（もし `0001` が `uuid-ossp` 等を使っていれば同じ関数に揃える）。着手時に `0001_init.sql` を確認。
- **マイグレーション番号衝突（apalis / 他機能）**: §4.1 のとおり out-of-order は起動を壊す。**着手直前に最新番号を再確認**し、`0006` が埋まっていれば繰り上げる。
- **空ボディ POST の抽出**: `POST /api/clusters/{id}/summary` は `Json<SummaryBody>` を取るため、完全に空のボディだと axum の `Json` 抽出が失敗する。フロントは常に `{}` を送る（§6.1）。curl 等で手動実行する際も `-d '{}'` を付けること。必要なら handler を `Option<Json<SummaryBody>>` にして空ボディ許容に変更可。
</content>
</invoke>
