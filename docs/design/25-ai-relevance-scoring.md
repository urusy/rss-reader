# 25 AI 関連度スコアリング

> 読み手前提: このリポジトリのコードは持っているが、この設計会話の文脈は知らない別セッション（effort 低め）の実装者。本書 1 ファイルだけで着手・完了できるよう、再利用資産・完全な SQL・関数シグネチャ・ルート文字列・フロント差分・TDD ケース・番号付き手順まで具体化する。
> **重要な但し書き（AI 部分）**: 本機能は Claude（`shared/llm`）に未読記事を**興味プロファイルへの関連度（0.0〜1.0）でスコアリング**させる。LLM の出力は非決定的なので、(a) プロファイルを構造化してプロンプトに渡し、(b) 出力を厳密に JSON パースして失敗は安全側に倒し、(c) 結果を DB にキャッシュしてトークンを使い回す。プロンプト本文やモデルの返却形は実装時に微調整が要る前提で、**JSON パーサ（`parse_relevance_scores`）とスコア正規化（`normalize_score`）・プロファイル指紋（`profile_fingerprint`）を純粋関数として単体テスト対象に切り出す**（§5.1）。
> **依存の明示**: 本機能は **機能 24（タグ基盤）の `tags` / `article_tags` テーブルに依存する**（興味プロファイルをタグ利用傾向から組み立てるため）。24 のマイグレーション（`0006_tags.sql` 想定）が**先に適用済み**であることが前提（§8）。

---

## 1. 概要

ユーザーの**興味プロファイル**（よく使うタグ + 直近の既読傾向）に対して、Claude が**未読記事**を関連度でスコア付けし、記事一覧を任意で**重要順（関連度の高い順）にソート**できるようにする。「未読が溜まったとき、まず読むべき記事から読む」を支援する。スコアは DB のキャッシュ列（`article_relevance_scores` テーブル）に保存し、同一プロファイル・同一記事への再要求はトークンを消費しない（要約/翻訳/タグ提案のキャッシュ方針と同型。`articles/service.rs`・`tags/service.rs` 参照）。

本機能はバックエンドに**新スライス `relevance` を 1 枚**追加する。責務は (1) 興味プロファイルの導出（`tags`/`article_tags` と `articles` を**読み取り専用**で集計）、(2) 未読記事の AI スコアリングとキャッシュ（`POST /api/relevance/score`、バッチ呼び出し）、(3) キャッシュ済みスコアの取得（`GET /api/relevance/scores`、CQRS-lite read model）、(4) 透明性のためのプロファイル可視化（`GET /api/relevance/profile`）。AI 呼び出しは唯一の抽象境界 `shared/llm` を再利用し、`LlmClient` trait に `score_relevance` を 1 つ足す（要約/翻訳/タグ提案と同じ場所・同じ流儀）。`ANTHROPIC_API_KEY` 未設定時は `AppError::NotEnabled`（503）を返す「任意機能」パターンに従う。

**記事一覧の重要順ソートは `articles` スライスを一切変更せずに実現する**。`relevance` スライスは「記事 id → スコア」の read model を返すだけで、フロントが既存 `listArticles()` の結果に id 突合でスコアを結合してクライアント側でソートする（`feed_overview` が `feed_id` 突合で統計を結合するのと同型）。これにより「タグ(24)/ダイジェスト(23)/River と相性が良い」共通土台（記事の優先度付け）を、`articles` の `list` クエリに手を入れずに載せられる。

---

## 2. スコープ / 非スコープ

### スコープ（本機能で実装する）
- マイグレーション **`0007_relevance.sql`**（番号は暫定。**着手前に必ず `ls backend/migrations/` で最新番号を確認し +1 を採る**。現状の最新は `0005_search.sql`、機能 24 が `0006_tags.sql` を取る想定なので本機能は暫定 `0007`。§4.1）。スコアキャッシュ `article_relevance_scores` テーブル 1 枚。
- 新スライス `backend/src/features/relevance/`（`domain` / `repository` / `service` / `handler` / `mod`）。
- スコアリング: `POST /api/relevance/score`（`?refresh=true` で全件再スコア）。興味プロファイルを組み立て、未読記事のうち**未スコア or プロファイルが変わった or refresh** のものを Claude にバッチ送信してスコア化し、キャッシュに upsert。`ANTHROPIC_API_KEY` 未設定なら 503。
- スコア取得: `GET /api/relevance/scores`（キャッシュ済みスコアの配列。**LLM を呼ばない**＝資格未設定でも 200）。フロントが記事一覧へ id 突合で結合・ソートするための read model。
- プロファイル可視化: `GET /api/relevance/profile`（現在のプロファイル文字列・指紋・件数。LLM を呼ばない）。
- `shared/llm` 拡張: `LlmClient` trait に `score_relevance(&self, ScoreRelevanceRequest) -> AppResult<String>`、`AnthropicClient` に実装を追加（要約/翻訳/タグ提案と同列。新スライスではなく既存の抽象境界への追記）。
- 純粋関数の単体テスト: `parse_relevance_scores`（JSON パース・id 検証・重複除去・件数制限）、`normalize_score`（0–100/0–1 両対応の正規化・clamp）、`profile_fingerprint`（FNV-1a・決定論）、`build_profile`（プロファイル文字列組み立て）。
- フロント: `lib/api.ts` に型 3・メソッド 3。`store.tsx` に `sort`（`"newest" | "relevance"`）状態 + `relevanceScores` リソース。`ArticleList` に「重要順」トグルと「スコアリング」ボタン、記事行のスコアバッジ（最小差分）。
- バックエンド自動テスト: ドメイン純粋関数（§9.1）+ リポジトリ往復（§9.2、実 DB `#[ignore]`）+ HTTP スモーク（§9.3、shell）。

### 非スコープ（本機能では実装しない）
- **サーバ側ソート**（`GET /api/articles?sort=relevance`）。`articles` スライスの `list` を変更することになるため行わない。重要順ソートは**フロントのクライアント側結合ソート**で実現（§6.4）。将来サーバ側でやるなら `relevance` スライスに「スコア順 JOIN read」エンドポイントを足す（§11）。
- **自動スコアリング（スケジューラ常駐）**。MVP はオンデマンド（ユーザーがボタンで起動）。`shared/scheduler.rs` への相乗りは将来課題（§11）。River/ダイジェスト連携時に検討。
- **既読記事のスコアリング**。スコアは「次に読むべき未読」を選ぶためのもの。既読は対象外。
- **プロファイルの手動編集 UI**（明示的な興味キーワード入力など）。MVP はタグ利用傾向 + 既読傾向から自動導出のみ。
- **ネガティブシグナル学習**（ミュート/スキップの反映）。`article_tags`/`is_read` の正シグナルのみ使う。
- 複数ユーザ / プロファイル切替。単一ユーザ前提（プロファイルはグローバルに 1 つ）。

---

## 3. 既存実装の再利用

実ファイルを確認済み。以下を **再利用し、車輪の再発明をしない**。

| 再利用資産 | 実体（確認済みファイル） | 本機能での使い方 |
|---|---|---|
| LLM 抽象境界（唯一の trait）＋ DB キャッシュ | `shared/llm/mod.rs`（`LlmClient`: `summarize`/`translate`）、`shared/llm/anthropic.rs`（`AnthropicClient::new(http,key,model)`、private `complete(system,user)`、`max_tokens:1024`、Messages API 直叩き、最初の text ブロック抽出）、`articles/service.rs::summarize_article`（キャッシュ命中で API を呼ばない） | trait に `score_relevance` を追加（**唯一許された抽象境界への追記**）。`complete()` を再利用。スコアキャッシュは `article_relevance_scores` の行＋`profile_hash` 一致で判定（summary キャッシュと同型） |
| 任意機能 = `NotEnabled` + `llm_client()` 生成 | `articles/service.rs::llm_client()`（`anthropic_api_key` 無しで `NotEnabled("ANTHROPIC_API_KEY is not set")`、有り時 `AnthropicClient::new(state.http.clone(), key, state.config.anthropic_model.clone())`） | 同じ `llm_client` 構築ロジックを `relevance/service.rs` に**コピーして**持つ（スライス自己完結。articles を import して結合しない） |
| LLM 出力 JSON パースを純関数化する前例 | `tags/domain.rs::parse_tag_suggestions`（`[` 〜 `]` 切り出し → `serde_json` → 正規化・重複除去・件数制限、`#[cfg(test)]`） | `parse_relevance_scores` を同型で新設（id 検証・clamp・重複除去・件数制限）。フェンス/前後 prose を剥がす方針も踏襲 |
| 興味プロファイルの材料（タグ） | 機能 24 の `tags` / `article_tags` テーブル（`article_tags.tag_id`→`tags.name`、`article_tags` は記事⇄タグ結合）。`tags/repository.rs::list_tags`（`LEFT JOIN article_tags ... COUNT`） | `relevance/repository.rs` が `tags`/`article_tags` を**読み取り専用 SQL** で集計し、よく使うタグ上位を取る（書き込み所有は移さない） |
| クロステーブル read を自スライス SQL で完結 | `instapaper/repository.rs::get_article_ref`、`digest/repository.rs::recent_unread`、`feed_overview`（feeds+articles JOIN read） | `relevance` から `articles`（未読・既読傾向）/`tags`/`article_tags` を**読み取り専用 SQL** で引く。`articles` の書き込みは触らない（§5.2 で正当化） |
| `articles.is_read` ＋ 未読部分インデックス | `0001_init.sql`（`idx_articles_is_read WHERE is_read=false`） | 未読抽出（スコア対象）と既読抽出（プロファイル材料）に使う |
| 値オブジェクト + 主キー newtype | `feeds/domain.rs::FeedUrl::parse`/`FeedId`、`articles/domain.rs::ArticleId`（`#[derive(...,sqlx::Type)] #[sqlx(transparent)] pub struct X(pub Uuid)`） | article 参照は `articles/domain.rs::ArticleId` を import（クロススライス domain 参照は既存前例: `articles` が `FeedId`/`FolderId` を import） |
| スライス構成 + `routes()` | `articles/mod.rs`・`feeds/`・`tags/`・`digest/`（5 ファイル、`fn routes() -> Router<AppState>`、`.route("/path", get(...).post(...))`、パスパラメータ `{id}`） | 同じ 5 ファイル構成で `relevance` を作る |
| `features/mod.rs` の合成 | `pub mod ...;` + `.merge(...::routes())` | `pub mod relevance;` と `.merge(relevance::routes())` を 1 行ずつ追加。既存スライスは触らない |
| sqlx ランタイムクエリ + upsert | `tags/repository.rs`（`ON CONFLICT (article_id) DO UPDATE`、`serde_json::Value` 束縛）、`articles/repository.rs`（`fetch_optional().ok_or(AppError::NotFound)`） | スコア保存は `ON CONFLICT (article_id) DO UPDATE`。すべて `query`/`query_as`（`query!` 禁止） |
| `AppError` 6 バリアント | `shared/error.rs`（`NotFound`/404, `Validation`/400, `NotEnabled`/503, `Upstream`/502, `Database`/500, `Other`/500、`IntoResponse` で `Json({"error":<Display>})`） | 新バリアントを足さず既存で表現（§5.7）。`error.rs` は編集しない |
| `AppState{db,config,http}` | `shared/state.rs`（`#[derive(Clone)]`、`http`: UA・30s timeout 済み） | `state.http`/`state.config`/`state.db` をそのまま使う |
| フロント API クライアント + グローバル状態 | `frontend/src/lib/api.ts`（`http<T>()`：204→`undefined`、`errorStatus()`、`動詞+リソース` 命名）、`lib/store.tsx`（`createContext`+`createResource`、`filter:"all"|"unread"`/`readIds` の前例） | `http<T>()` を再利用し型 3・メソッド 3 追加。store に `sort` 状態（`filter` と同型）と `relevanceScores` リソース（`feeds`/`folders` と同型）を足す |
| 自前 UI 部品 | `components/ui/{button,badge,card}.tsx`（`cn`+Tailwind、oklch トークン） | スコアバッジ・トグルボタンに `badge.tsx`/`button.tsx` を流用（新部品は作らない） |
| HTTP スモークの慣習 | `scripts/test/api-stats.sh`/`api-*.sh`（稼働スタックに curl、HTTP コード + JSON 形を assert） | `scripts/test/api-relevance.sh` を同型で新設（§9.3） |
| 自動マイグレーション実行 | `main.rs` → `db::run_migrations` → `sqlx::migrate!("./migrations")`（`set_ignore_missing` 無し＝out-of-order で起動が壊れる） | ファイルを置くだけで起動時適用。**番号順序に注意**（§4.1） |

> **依存追加は不要**: `uuid`/`serde`/`serde_json`/`sqlx`/`chrono`/`async_trait`/`reqwest` はすべて既存依存。`profile_fingerprint` は **FNV-1a を内製**（`std` のみ、`sha2` 等を足さない）。`article_relevance_scores.score` は `REAL`(`f32`)、`reasoning` は `TEXT`(`Option<String>`) で sqlx 既定の束縛で読み書きできる（Cargo.toml 変更不要）。

---

## 4. データモデルとマイグレーション

### 4.1 マイグレーション番号の決め方（必読）

`main.rs` の `sqlx::migrate!("./migrations").run()` は `set_ignore_missing` を呼ばないため、**適用済み最大バージョンより小さい未適用マイグレーションを後から追加すると起動時に `VersionMissing`（out-of-order）でエラー**になる（家庭内サーバの永続 DB で実害）。

**ルール**: 着手前に `ls backend/migrations/` で最新番号を確認し、**最大番号 +1** を採る。本書執筆時点の最新は `0005_search.sql`。**機能 24（タグ基盤）が `0006_tags.sql` を取る前提**なので、本機能は**暫定的に `0007_relevance.sql`** と採番する。24 より先に本機能をマージする場合や、apalis 移行等が先に番号を取った場合は、その時点の最小空き整数へ繰り上げること。既存マイグレーションは**編集しない**（追記のみ）。

> **24 との順序依存**: 本テーブルは `tags`/`article_tags` を**参照しない**（外部キー無し・読み取りのみ）。よって DB 制約上は 24 の前に適用しても起動は壊れない。ただし**機能として 24 のテーブルが無いとプロファイルのタグ材料が空になる**ため、24 を先に出すのが正しい運用順（§8）。

### 4.2 スキーマ

新規ファイル **`backend/migrations/0007_relevance.sql`**（番号は §4.1 で確認）:

```sql
-- 0007_relevance.sql
-- AI relevance scores: per-article cache of how relevant each UNREAD article is
-- to the user's interest profile (frequently-used tags + recent read history).
-- Single-user app => one global profile, one score row per article. Mirrors the
-- summary/translation caching in the articles slice (presence + profile_hash =
-- cache hit; re-score only on ?refresh=true or when the profile drifts).
CREATE TABLE IF NOT EXISTS article_relevance_scores (
    -- One score per article. CASCADE so deleting an article drops its score.
    article_id   UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    -- Relevance in [0.0, 1.0]. Higher = more relevant to the interest profile.
    score        REAL NOT NULL,
    -- Optional short rationale from the model (for transparency / debugging).
    reasoning    TEXT,
    -- Fingerprint of the profile string used to produce this score. When the
    -- current profile fingerprint differs, the score is considered stale and is
    -- recomputed on the next POST /api/relevance/score.
    profile_hash TEXT NOT NULL,
    -- Anthropic model id used, for auditing / future model migration.
    model        TEXT NOT NULL,
    scored_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- "重要順" lookups read all rows and the frontend joins by article_id, but a
-- score-desc index keeps any future server-side ordering cheap.
CREATE INDEX IF NOT EXISTS idx_article_relevance_score_desc
    ON article_relevance_scores (score DESC);
```

設計判断:
- **`score` を `REAL`(0.0–1.0)** に固定。モデルには 0–100 の整数を出させて `normalize_score` で 0–1 に正規化する（プロンプトは整数の方が安定。§5.1・§5.8）。フロントは `Math.round(score*100)` で「％」表示する。
- **`profile_hash` 列**: スコアは「未読記事 × そのときの興味プロファイル」の関数。プロファイル（タグ傾向・既読傾向）は時間とともに変わるので、**プロファイルが変わったら古いスコアを再計算したい**。プロファイル文字列の FNV-1a 指紋を保存し、現在の指紋と不一致なら stale 扱いにする（要約の `summary_lang` 一致判定の発展形）。
- **独立テーブルにする理由**: スコアは「未承認の派生情報」で `articles` の本体ライフサイクルと別。`articles` への列追加（要約/翻訳と同型）も選べたが、**articles スライスの所有物に relevance の関心を漏らさない**ため独立テーブルにする（instapaper/tags が `articles` に列を足さず別テーブルにしたのと同じ判断）。`scored_at`＋`profile_hash` の有無がキャッシュヒット判定。
- **`reasoning` を任意列に**: 透明性のため UI に「なぜ高スコアか」を出せる。トークン節約でモデルが省略したら NULL。
- **`ON DELETE CASCADE`**: 記事が消えたらスコアも消える（孤立行を残さない）。

`feeds`/`articles`/`tags`/`article_tags` への列追加は**無い**。

---

## 5. バックエンド設計

新スライス **`backend/src/features/relevance/`**。5 ファイル構成。加えて `shared/llm` に `score_relevance` を 1 メソッド追記（§5.8）。

### 5.1 `domain.rs`（値オブジェクト + 純粋ロジック + 単体テスト対象）

```rust
use std::collections::HashSet;

use serde::Serialize;
use uuid::Uuid;

/// 1 記事ぶんのキャッシュ済みスコア。GET /api/relevance/scores がそのまま返す。
/// article_id は read model の相関キーなので、あえて ArticleId newtype を持ち込まず
/// 素の Uuid を使う（feed_overview が feed_id に素の Uuid を返すのと同方針：
/// CQRS 読み取り read model はドメイン newtype を跨いで持ち込まない）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RelevanceScore {
    pub article_id: Uuid,
    pub score: f32,
    pub reasoning: Option<String>,
    pub scored_at: chrono::DateTime<chrono::Utc>,
}

/// スコアリング実行結果（POST /api/relevance/score のレスポンス）。
/// scored_count = 今回新たに（再）スコアした件数。scores = 現在の全キャッシュ。
#[derive(Debug, Clone, Serialize)]
pub struct ScoreResult {
    pub scored_count: usize,
    pub profile_hash: String,
    pub scores: Vec<RelevanceScore>,
}

/// 興味プロファイルの可視化（GET /api/relevance/profile）。
#[derive(Debug, Clone, Serialize)]
pub struct ProfileView {
    pub profile: String,
    pub hash: String,
    pub tag_count: usize,
    pub read_count: usize,
}

/// パース・正規化済みの 1 件のスコア（service が DB に保存する形）。
#[derive(Debug, Clone, PartialEq)]
pub struct RawScore {
    pub article_id: Uuid,
    pub score: f32,
    pub reasoning: Option<String>,
}

/// Claude が返す 1 件の生スコア（JSON 要素）。score は 0–100 想定だが 0–1 も許容する。
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct LlmScore {
    id: String,
    score: f32,
    #[serde(default)]
    reason: Option<String>,
}

/// スコアを 0.0..=1.0 に正規化する純粋関数。
/// モデルが 0–100 を返したら 100 で割り、0–1 を返したらそのまま。負値や巨大値は clamp。
/// 1.0 を超え 100 以下の値は「0–100 スケール」とみなして割る（境界 >1.0）。
pub fn normalize_score(raw: f32) -> f32 {
    if !raw.is_finite() {
        return 0.0;
    }
    let v = if raw > 1.0 { raw / 100.0 } else { raw };
    v.clamp(0.0, 1.0)
}

/// Claude の生出力（JSON 文字列）を厳密にパース・正規化する純粋関数。
/// LLM はときに前後に説明文や ```json フェンスを付けるので、最初の '[' から
/// 最後の ']' までを切り出してから serde_json でパースする。失敗は Err（安全側）。
/// - id が UUID でない／valid_ids に無いものは捨てる（幻覚 id 対策）。
/// - score は normalize_score で 0..1 に。reason は trim、空なら None。
/// - 同一 article_id の重複は最初の 1 件だけ採用。
pub fn parse_relevance_scores(
    raw: &str,
    valid_ids: &HashSet<Uuid>,
) -> Result<Vec<RawScore>, String> {
    let start = raw.find('[').ok_or("no JSON array found in LLM output")?;
    let end = raw.rfind(']').ok_or("no JSON array found in LLM output")?;
    if end < start {
        return Err("malformed JSON array in LLM output".into());
    }
    let slice = &raw[start..=end];
    let parsed: Vec<LlmScore> =
        serde_json::from_str(slice).map_err(|e| format!("invalid score JSON: {e}"))?;

    let mut seen: HashSet<Uuid> = HashSet::new();
    let mut out = Vec::new();
    for s in parsed {
        let Ok(id) = Uuid::parse_str(s.id.trim()) else {
            continue; // id が UUID でない → 捨てる
        };
        if !valid_ids.contains(&id) {
            continue; // 候補に無い id（幻覚）→ 捨てる
        }
        if !seen.insert(id) {
            continue; // 重複 → 最初の 1 件のみ
        }
        let reasoning = s
            .reason
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty());
        out.push(RawScore {
            article_id: id,
            score: normalize_score(s.score),
            reasoning,
        });
    }
    Ok(out)
}

/// 興味プロファイル文字列を組み立てる純粋関数（= 単体テスト対象）。
/// tags: (タグ名, 付与記事数) を多い順に、read_titles: 直近の既読タイトル。
/// プロンプトに渡しやすい簡潔な箇条書きにする。両方空なら "(no profile yet)"。
pub fn build_profile(tags: &[(String, i64)], read_titles: &[String]) -> String {
    if tags.is_empty() && read_titles.is_empty() {
        return "(no profile yet)".to_string();
    }
    let mut s = String::new();
    if !tags.is_empty() {
        let list = tags
            .iter()
            .map(|(name, count)| format!("{name} (x{count})"))
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str("Frequently used tags (interests): ");
        s.push_str(&list);
        s.push('\n');
    }
    if !read_titles.is_empty() {
        s.push_str("Recently read article titles:\n");
        for t in read_titles {
            let t = t.trim();
            if !t.is_empty() {
                s.push_str("- ");
                s.push_str(t);
                s.push('\n');
            }
        }
    }
    s.trim_end().to_string()
}

/// プロファイル文字列の決定論的指紋（FNV-1a 64bit を 16 桁 hex に）。
/// std だけで実装。同じ入力で必ず同じ出力（DB の profile_hash 比較に使う）。
pub fn profile_fingerprint(profile: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV offset basis
    for b in profile.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
    }
    format!("{hash:016x}")
}
```

> JSON パース・正規化・指紋・プロファイル組み立てを純粋関数に切り出すのは、Claude を叩かず・DB も触らずに TDD で Red→Green を回すため（MEMORY「書いたら必ず実行」「バグ修正もテスト先行」）。幻覚 id 除去・clamp・0–100/0–1 両対応・重複除去の境界はここで完全にテストする（§9.1）。

### 5.2 `repository.rs`（`&PgPool` を取る free async fn、ランタイムクエリのみ）

```rust
use std::collections::HashSet;

use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{RawScore, RelevanceScore};
use crate::shared::error::AppResult;

/// スコア対象の未読記事（読み取り射影）。snippet は summary 優先・無ければ本文先頭。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScoreCandidate {
    pub id: Uuid,
    pub title: String,
    pub snippet: String,
}

// ---- profile materials (read-only cross-table) ----

/// 興味プロファイル用: よく使うタグ上位を (name, 付与記事数) で返す。
/// 機能 24 の tags / article_tags を読み取り専用で集計（書き込み所有は移さない）。
/// テーブルが未作成（24 未適用）の環境では呼ぶ前に存在確認する想定だが、
/// service 側は本関数のエラーを握りつぶさず伝播する（§5.3 で空プロファイル許容）。
pub async fn top_tags(pool: &PgPool, limit: i64) -> AppResult<Vec<(String, i64)>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        r#"SELECT t.name, COUNT(at.article_id) AS cnt
           FROM tags t
           JOIN article_tags at ON at.tag_id = t.id
           GROUP BY t.id
           ORDER BY cnt DESC, t.name ASC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 直近の既読タイトル（新しい順）。既読傾向のプロファイル材料。
pub async fn recent_read_titles(pool: &PgPool, limit: i64) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"SELECT title FROM articles
           WHERE is_read = true
           ORDER BY COALESCE(published_at, created_at) DESC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(t,)| t).collect())
}

// ---- scoring candidates (unread) ----

/// スコア対象の未読記事を新しい順・最大 limit 件で返す。
/// snippet は summary 優先、無ければ本文先頭 500 文字（トークン抑制）。
pub async fn unread_candidates(pool: &PgPool, limit: i64) -> AppResult<Vec<ScoreCandidate>> {
    let rows = sqlx::query_as::<_, ScoreCandidate>(
        r#"SELECT id,
                  title,
                  COALESCE(NULLIF(summary, ''), LEFT(content, 500)) AS snippet
           FROM articles
           WHERE is_read = false
           ORDER BY COALESCE(published_at, created_at) DESC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ---- score cache ----

/// 全キャッシュ済みスコアを score 降順で返す（read model）。
pub async fn list_scores(pool: &PgPool) -> AppResult<Vec<RelevanceScore>> {
    let rows = sqlx::query_as::<_, RelevanceScore>(
        r#"SELECT article_id, score, reasoning, scored_at
           FROM article_relevance_scores
           ORDER BY score DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 現在の profile_hash で既に新鮮にスコア済みの article_id 集合を返す。
/// この集合に無い未読記事だけを再スコアすれば、トークンを使い回せる。
pub async fn fresh_scored_ids(
    pool: &PgPool,
    profile_hash: &str,
) -> AppResult<HashSet<Uuid>> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT article_id FROM article_relevance_scores WHERE profile_hash = $1",
    )
    .bind(profile_hash)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// スコアを一括 upsert（無ければ挿入、有れば上書き）。1 トランザクション。
pub async fn save_scores(
    pool: &PgPool,
    scores: &[RawScore],
    profile_hash: &str,
    model: &str,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for s in scores {
        sqlx::query(
            r#"INSERT INTO article_relevance_scores
                 (article_id, score, reasoning, profile_hash, model, scored_at)
               VALUES ($1, $2, $3, $4, $5, now())
               ON CONFLICT (article_id) DO UPDATE
                 SET score = EXCLUDED.score,
                     reasoning = EXCLUDED.reasoning,
                     profile_hash = EXCLUDED.profile_hash,
                     model = EXCLUDED.model,
                     scored_at = now()"#,
        )
        .bind(s.article_id)
        .bind(s.score)
        .bind(&s.reasoning)
        .bind(profile_hash)
        .bind(model)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
```

> **`articles`/`tags`/`article_tags` を読むことの正当化**: スコアリングには未読記事本文（`unread_candidates`）と興味材料（`top_tags`/`recent_read_titles`）が要る。`digest/repository.rs::recent_unread`（articles 読み取り）・`feed_overview`（feeds+articles JOIN read）と同じ「読み取りのクロステーブル参照」の前例どおり許容。これらの**書き込み所有は移さない**ので越境共通レイヤーには当たらない。`query!` は使わず全て `query`/`query_as`。

### 5.3 `service.rs`（`&AppState` を取り repository + LLM を統合）

```rust
use std::collections::HashSet;

use super::domain::{
    build_profile, parse_relevance_scores, profile_fingerprint, ProfileView, RelevanceScore,
    ScoreResult,
};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{LlmClient, ScorableArticle, ScoreRelevanceRequest};
use crate::shared::state::AppState;

const TOP_TAGS: i64 = 20;
const READ_TITLES: i64 = 30;
const MAX_BATCH: i64 = 40; // 1 回のスコアリングで Claude に送る未読記事の上限

/// articles/service.rs と同型。スライス自己完結のため articles を import せず複製。
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

/// 現在の興味プロファイル文字列を組み立てる（タグ傾向 + 既読傾向）。
async fn current_profile(state: &AppState) -> AppResult<(String, usize, usize)> {
    let tags = repository::top_tags(&state.db, TOP_TAGS).await?;
    let read_titles = repository::recent_read_titles(&state.db, READ_TITLES).await?;
    let profile = build_profile(&tags, &read_titles);
    Ok((profile, tags.len(), read_titles.len()))
}

/// GET /api/relevance/profile 用（LLM を呼ばない）。
pub async fn profile_view(state: &AppState) -> AppResult<ProfileView> {
    let (profile, tag_count, read_count) = current_profile(state).await?;
    let hash = profile_fingerprint(&profile);
    Ok(ProfileView { profile, hash, tag_count, read_count })
}

/// GET /api/relevance/scores 用（キャッシュをそのまま返す。LLM を呼ばない）。
pub async fn list_scores(state: &AppState) -> AppResult<Vec<RelevanceScore>> {
    repository::list_scores(&state.db).await
}

/// POST /api/relevance/score 用。
/// 流れ: (1) 資格チェック（ANTHROPIC_API_KEY 無しは NotEnabled。機能ゲート優先）。
///       (2) プロファイル組み立て → 指紋。
///       (3) 未読候補を取得。
///       (4) refresh=false なら現指紋で新鮮にスコア済みの id を除外（トークン節約）。
///       (5) 残りを Claude にバッチ送信 → JSON パース → 保存。
///       (6) 全キャッシュを返す。
pub async fn score_unread(state: &AppState, refresh: bool) -> AppResult<ScoreResult> {
    let client = llm_client(state)?; // 機能ゲートを先に判定

    let (profile, _tc, _rc) = current_profile(state).await?;
    let profile_hash = profile_fingerprint(&profile);

    let candidates = repository::unread_candidates(&state.db, MAX_BATCH).await?;

    // 既に現プロファイルでスコア済みの未読は飛ばす（refresh 指定時は全件対象）。
    let fresh: HashSet<_> = if refresh {
        HashSet::new()
    } else {
        repository::fresh_scored_ids(&state.db, &profile_hash).await?
    };
    let to_score: Vec<_> = candidates
        .into_iter()
        .filter(|c| !fresh.contains(&c.id))
        .collect();

    let mut scored_count = 0usize;
    if !to_score.is_empty() {
        let valid_ids: HashSet<_> = to_score.iter().map(|c| c.id).collect();
        let articles: Vec<ScorableArticle> = to_score
            .iter()
            .map(|c| ScorableArticle {
                id: c.id.to_string(),
                title: c.title.clone(),
                snippet: c.snippet.clone(),
            })
            .collect();

        let raw = client
            .score_relevance(ScoreRelevanceRequest { profile, articles })
            .await?;

        let parsed = parse_relevance_scores(&raw, &valid_ids)
            .map_err(|e| AppError::Upstream(format!("could not parse LLM score output: {e}")))?;

        repository::save_scores(
            &state.db,
            &parsed,
            &profile_hash,
            &state.config.anthropic_model,
        )
        .await?;
        scored_count = parsed.len();
    }

    let scores = repository::list_scores(&state.db).await?;
    Ok(ScoreResult { scored_count, profile_hash, scores })
}
```

> 順序が重要: **資格 → プロファイル → 候補 → 差分抽出 → 呼び出し → 保存**。資格未設定なら未読の有無に関わらず先に 503（機能ゲート優先・テスト決定性のため。`articles/service.rs::llm_client` を先に呼ぶ既存パターンと同型）。LLM 出力が JSON として壊れていた場合は `Upstream`（502）に倒す（クライアントのせいではない上流事象）。`score_unread` は handler から呼ばれるので `-D warnings` でも未使用にならない。HTTP 呼び出しは `AnthropicClient`（trait 実装）に閉じ、本スライスに新しい trait/dyn は足さない。

### 5.4 `handler.rs`（axum ハンドラ）

```rust
use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use super::domain::{ProfileView, RelevanceScore, ScoreResult};
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn list_scores(State(state): State<AppState>) -> AppResult<Json<Vec<RelevanceScore>>> {
    Ok(Json(service::list_scores(&state).await?))
}

pub async fn profile(State(state): State<AppState>) -> AppResult<Json<ProfileView>> {
    Ok(Json(service::profile_view(&state).await?))
}

#[derive(Debug, Deserialize)]
pub struct ScoreQuery {
    #[serde(default)]
    pub refresh: bool,
}

pub async fn score(
    State(state): State<AppState>,
    Query(q): Query<ScoreQuery>,
) -> AppResult<Json<ScoreResult>> {
    Ok(Json(service::score_unread(&state, q.refresh).await?))
}
```

### 5.5 `mod.rs`（routes）

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
        .route("/api/relevance/scores", get(handler::list_scores))
        .route("/api/relevance/profile", get(handler::profile))
        .route("/api/relevance/score", post(handler::score))
}
```

> ルートはすべて `/api/relevance/*` の静的セグメントで、既存スライス（`articles`/`feeds`/`tags`/...）のどのルートとも method+path が重複しない。`.merge()` で衝突しない。

### 5.6 `features/mod.rs` への追加（2 行のみ）

```rust
pub mod relevance; // 既存 pub mod 群に追加（articles; feeds; folders; tags; ... の並びに）
// router() 内の .merge チェーンに追加:
        .merge(relevance::routes())
```

既存スライス（articles/feeds/folders/tags/instapaper/search/health 等）は一切触らない。接触点は `features/mod.rs`（2 行）と横断インフラ `shared/llm/{mod,anthropic}.rs`（`score_relevance` 追記）のみ。

### 5.7 AppError の使い分け（`error.rs` は不編集）

| 状況 | バリアント | HTTP | レスポンス `error`（Display） |
|---|---|---|---|
| `POST /score` で `ANTHROPIC_API_KEY` 未設定 | `NotEnabled` | 503 | `feature not yet enabled: ANTHROPIC_API_KEY is not set` |
| Claude 呼び出し失敗 / JSON パース不能 | `Upstream` | 502 | `upstream request failed: ...` |
| DB エラー（24 未適用で `tags`/`article_tags` 不在含む） | `Database`（`?` で自動 `From`） | 500 | `internal error` |

> 新バリアントは追加しない。`GET /scores`・`GET /profile` は LLM を呼ばないため資格未設定でも 200。`POST /score` のチェック順は**資格→プロファイル→候補→呼び出し**（§5.3）。`tags`/`article_tags` が無い（24 未適用）環境では `top_tags` の SQL が `relation "tags" does not exist` で `Database`(500) になる。これは「24 を先に適用せよ」という運用前提（§8）であり、本機能が握りつぶして偽プロファイルを作るより明示エラーの方が安全。

### 5.8 `shared/llm` の拡張（唯一許された抽象境界への追記）

`backend/src/shared/llm/mod.rs` に型 + trait メソッドを **追記**:

```rust
/// スコアリング対象 1 記事（プロバイダ非依存の入力）。id は記事 UUID の文字列。
#[derive(Debug, Clone)]
pub struct ScorableArticle {
    pub id: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Clone)]
pub struct ScoreRelevanceRequest {
    /// build_profile() が組み立てた興味プロファイル文字列。
    pub profile: String,
    /// スコア対象の未読記事群（バッチ）。
    pub articles: Vec<ScorableArticle>,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn summarize(&self, req: SummarizeRequest) -> AppResult<String>;
    async fn translate(&self, req: TranslateRequest) -> AppResult<String>;
    // 追加: 関連度スコアリング。返り値は JSON 配列文字列
    // （service 側で parse_relevance_scores に通す）。
    async fn score_relevance(&self, req: ScoreRelevanceRequest) -> AppResult<String>;
}
```

> 機能 24（タグ）も同じ trait に `suggest_tags` を足す。24 と本機能が並行着手する場合、**`LlmClient` trait と `AnthropicClient` の `impl` を同一ファイルで同居編集する**ことになる（マージ衝突に注意。追記位置を分ければ容易にマージできる）。24 が先にマージ済みなら本機能は `score_relevance` を 1 メソッド足すだけ。

`backend/src/shared/llm/anthropic.rs` の `impl LlmClient for AnthropicClient` に **追記**（既存 private `complete(system, user)` を再利用）:

```rust
use super::{LlmClient, ScorableArticle, ScoreRelevanceRequest /* , 既存 */};

// impl LlmClient for AnthropicClient 内に追加（summarize/translate はそのまま）:
async fn score_relevance(&self, req: ScoreRelevanceRequest) -> AppResult<String> {
    let system = format!(
        "You score how relevant unread articles are to a user's interest \
         profile, so the most worth-reading ones can be surfaced first. \
         The user's interest profile is:\n{}\n\n\
         For EACH article below, output an integer relevance score from 0 \
         (irrelevant) to 100 (highly relevant) and a very short reason. \
         Respond with ONLY a JSON array, no prose, no code fences, like: \
         [{{\"id\":\"<uuid>\",\"score\":80,\"reason\":\"matches rust interest\"}}]. \
         Use the exact id given for each article.",
        req.profile
    );
    // 各記事を id 付きの箇条書きにしてユーザーメッセージに渡す。
    let user = req
        .articles
        .iter()
        .map(|a| {
            let snippet = a.snippet.trim();
            let snippet = if snippet.chars().count() > 400 {
                snippet.chars().take(400).collect::<String>()
            } else {
                snippet.to_string()
            };
            format!("id: {}\ntitle: {}\nexcerpt: {}", a.id, a.title.trim(), snippet)
        })
        .collect::<Vec<_>>()
        .join("\n---\n");
    self.complete(&system, &user).await
}
```

> trait は **新設しない**。既存の唯一の境界 `LlmClient` に `score_relevance` を足すだけ（要約/翻訳/タグ提案と同列の「LLM への新しい依頼種別」）。これは CLAUDE.md「抽象境界は `shared/llm` のみ」に整合する拡張。`complete` の `max_tokens` は現状 1024 固定で、大きいバッチ（多数記事 × reason）は出力が切れうる（§11 に緩和策＝`MAX_BATCH` を小さく / `complete_with(max_tokens)` 小改修）。

---

## 6. フロントエンド設計

> 方針: 本機能の UI は「記事一覧を重要順にソートする」体験が核。**`articles` スライス（バックエンド）も既存 `listArticles()` も変更しない**。スコアは別エンドポイントから取り、フロントが `article.id` 突合で結合してクライアント側ソートする（`feed_overview` の id 突合結合と同型）。Ark UI は不要（自前 `button.tsx`/`badge.tsx` で足りる）。

### 6.1 `lib/api.ts` への追加（型 3 + メソッド 3）

型（backend JSON をミラー）:

```ts
export interface RelevanceScore {
  article_id: string;
  score: number;            // 0.0 .. 1.0
  reasoning: string | null;
  scored_at: string;
}

export interface ScoreResult {
  scored_count: number;
  profile_hash: string;
  scores: RelevanceScore[];
}

export interface RelevanceProfile {
  profile: string;
  hash: string;
  tag_count: number;
  read_count: number;
}
```

`api` オブジェクトにメソッド追加（既存 `http<T>()` を再利用、`動詞+リソース` 命名）:

```ts
  listRelevanceScores: () => http<RelevanceScore[]>("/api/relevance/scores"),
  scoreRelevance: (refresh = false) =>
    http<ScoreResult>(
      `/api/relevance/score${refresh ? "?refresh=true" : ""}`,
      { method: "POST" },
    ),
  getRelevanceProfile: () => http<RelevanceProfile>("/api/relevance/profile"),
```

> `POST /score` の 503（APIキー未設定）は `http<T>()` が throw する。呼び出し側で `errorStatus(e) === 503` を「要約と同じく `ANTHROPIC_API_KEY` 未設定」表示に振り分ける（既存 `errorStatus` ヘルパを使う）。

### 6.2 `store.tsx` への追加（ソート状態 + スコアリソース）

既存 `UiState`（`sidebarOpen`/`filter`/`readIds`）に `sort` を、`UiStore` に `relevanceScores` リソースを足す。`filter`（`"all"|"unread"`）と同型なので書き方は既存に倣う:

```ts
// UiState に追加
  sort: "newest" | "relevance";

// UiStore に追加
  setSort(s: "newest" | "relevance"): void;
  relevanceScores: Resource<RelevanceScore[]>;
  refetchRelevanceScores(): void;
```

`AppProvider` 内（`feeds`/`folders` リソースと同じ書き方）:
```tsx
const [state, setState] = createStore<UiState>({
  sidebarOpen: false,
  filter: "all",
  sort: "newest",       // ← 追加
  readIds: {},
});
const [relevanceScores, { refetch: refetchRelevanceScores }] =
  createResource(() => api.listRelevanceScores());
// setSort: (s) => setState("sort", s)
// useApp() の戻り値に sort 関連と relevanceScores, refetchRelevanceScores を含める
```

> store を使わず `ArticleList` 内で `createResource` 直接でも成立するが、ソート状態と語彙的に近い `filter` が既に store にあるため、`sort` も store に置くのが素直（11「未読フィルタ」が `filter` を store に置いたのと同じ判断）。

### 6.3 スコアバッジ（`badge.tsx` を流用）

新規 UI 部品は作らない。記事行のスコア表示は既存 `components/ui/badge.tsx` を使う:

```tsx
import { Badge } from "@/components/ui/badge";
// score: number(0..1) → 「85%」表示。色は oklch トークンのみ。
<Badge>{Math.round(score * 100)}%</Badge>
```

### 6.4 `ArticleList` への組み込み（最小差分・`articles` 不変）

`routes/ArticleList.tsx`（3 ペインの記事一覧ペイン。既存）に次を足す:

1. **ヘッダにソートトグル + スコアリングボタン**（`button.tsx` 流用）:
   - 「新着順 / 重要順」トグル → `store.setSort("newest" | "relevance")`。
   - 「スコアリング」ボタン → `await api.scoreRelevance()` 実行 → `store.refetchRelevanceScores()`。実行中は disabled。503 は「`ANTHROPIC_API_KEY` 未設定」、502 は「生成に失敗」をメッセージ表示。`scored_count` を「N 件をスコアしました」とトースト/小表示。
2. **スコアの結合とソート**（クライアント側・`listArticles()` の戻りは不変）:
   ```tsx
   import { createMemo } from "solid-js";
   const scoreById = createMemo(() => {
     const m = new Map<string, number>();
     for (const s of store.relevanceScores() ?? []) m.set(s.article_id, s.score);
     return m;
   });
   // 表示用の記事配列（既存の記事 resource を articles() とする）
   const sorted = createMemo(() => {
     const list = articles() ?? [];
     if (store.state.sort !== "relevance") return list; // 既存=新着順のまま
     const m = scoreById();
     // スコア未付与は末尾（-1）。元順を壊さない安定ソートのため slice() してから sort。
     return list.slice().sort(
       (a, b) => (m.get(b.id) ?? -1) - (m.get(a.id) ?? -1),
     );
   });
   ```
3. **各記事行にスコアバッジ**: `scoreById().get(article.id)` があれば §6.3 のバッジを出す（無ければ非表示）。

> これは `articles` スライスの `list`（サーバ）も `api.listArticles()`（フロント）も触らない純加算。重要順は「すでに取得済みの一覧（最大 200 件）」に対するクライアント側の並べ替えで、`MAX_BATCH=40` 件ぶんのスコアが付いた記事が上に来る。スコア未付与の記事は新着順の相対位置を保って末尾に並ぶ。

### 6.5 状態管理・トークン

- 新色・生 hex は持ち込まない（oklch トークン維持）。スコアバッジは `badge.tsx` の既定スタイル。
- プロファイル可視化（`GET /api/relevance/profile`）は任意。設定/デバッグ用に「現在の興味プロファイル」を小さく出すなら `getRelevanceProfile()` を呼んで `profile` を `text-xs text-muted-foreground` で表示する程度に留める（MVP では省略可）。

---

## 7. API 契約

> すべて `/api` プレフィックス。スコアは `0.0`〜`1.0` の小数。

### 7.1 `GET /api/relevance/scores` — キャッシュ済みスコア一覧（read model）
- リクエスト: クエリ・ボディなし。LLM を呼ばない（資格未設定でも 200）。
- レスポンス（200）: `RelevanceScore` の配列（score 降順）。フロントは `article_id` 突合で記事一覧に結合。

```json
[
  {
    "article_id": "7b1c0d2e-2a3b-4c5d-8e9f-0a1b2c3d4e5f",
    "score": 0.85,
    "reasoning": "matches your 'rust' and 'databases' interests",
    "scored_at": "2026-06-30T09:12:00Z"
  },
  {
    "article_id": "9f8e7d6c-5b4a-3c2d-1e0f-aabbccddeeff",
    "score": 0.12,
    "reasoning": null,
    "scored_at": "2026-06-30T09:12:00Z"
  }
]
```

### 7.2 `POST /api/relevance/score` — 未読をスコアリング（キャッシュ更新）
- リクエスト: ボディ無し。任意クエリ `?refresh=true`（全未読を現プロファイルで再スコア）。
- 挙動: 未読のうち**未スコア or プロファイルが変わった**ものを Claude にバッチ送信してスコア化・upsert。
- レスポンス（200）:

```json
{
  "scored_count": 12,
  "profile_hash": "3a1f9c0b7e5d2240",
  "scores": [
    { "article_id": "7b1c0d2e-...", "score": 0.85, "reasoning": "...", "scored_at": "2026-06-30T09:12:00Z" }
  ]
}
```

- `scored_count`: 今回新たに（再）スコアした件数（既に新鮮なら 0）。
- `scores`: 現在の全キャッシュ（7.1 と同形の配列）。フロントはこれで store を更新してもよい。
- エラー:
  - 503 `{ "error": "feature not yet enabled: ANTHROPIC_API_KEY is not set" }`（APIキー未設定）
  - 502 `{ "error": "upstream request failed: could not parse LLM score output: ..." }`（Claude 障害 / JSON 不正）
  - 500 `{ "error": "internal error" }`（DB 障害。24 未適用で `tags` 不在も含む）

### 7.3 `GET /api/relevance/profile` — 現在の興味プロファイル（透明性）
- リクエスト: なし。LLM を呼ばない。
- レスポンス（200）:

```json
{
  "profile": "Frequently used tags (interests): rust (x14), databases (x9)\nRecently read article titles:\n- ...",
  "hash": "3a1f9c0b7e5d2240",
  "tag_count": 12,
  "read_count": 30
}
```

---

## 8. 依存関係

- **本機能が依存する機能（ハード依存）: 機能 24（タグ基盤）**。興味プロファイルのタグ材料に `tags` / `article_tags` テーブルを**読み取り**で使う。24 のマイグレーション（`0006_tags.sql` 想定）が**先に適用済み**であること。未適用だと `top_tags` の SQL が `relation "tags" does not exist` で 500 になる（§5.7）。
  - 緩和: 24 をまだ出さずに本機能だけ先行したい場合は、`top_tags` を「テーブルが無ければ空を返す」防御実装（`to_regclass('tags')` で存在確認）に差し替えれば、既読傾向だけのプロファイルで動く。ただし**設計上は 24 を先に出すのが正しい運用順**なので、本書はハード依存として扱う（§11 に代替案）。
- **本機能が読み取りで参照するもの（既存）**: `articles`（未読候補・既読傾向）。書き込みはしない。
- **本機能がブロックする / 土台になる機能（将来・非スコープ）**:
  - **River / ダイジェスト連携**: 「重要な未読だけのダイジェスト」「重要順タイムライン（River）」は本スコアを材料にできる（`GET /api/relevance/scores` を結合）。
  - **サーバ側ソート**: `relevance` スライスに「スコア順 articles JOIN read」エンドポイントを足せば、一覧 API 側で重要順ページングできる（§11）。
- **横断インフラへの接触点**: `shared/llm/{mod,anthropic}.rs`（`score_relevance` 追記。24 と同居編集の可能性。§5.8）。`shared/config.rs` は**触らない**（`anthropic_api_key`/`anthropic_model` は既存）。
- 既存スライス（articles/feeds/folders/tags/...）への変更は無し。`features/mod.rs` への 2 行追加のみが既存ファイルへの接触点（`shared/llm` を除く）。

---

## 9. テスト計画（TDD）

**Red → 理解 → Green の順。書いたら必ず実行する。**

> 配置方針は既存前例に合わせる: 純粋ロジックは各 `.rs` の `#[cfg(test)] mod tests`、DB を触る往復は `repository.rs` 内の `#[ignore]` テスト（本 crate は binary crate で `lib.rs` 無し＝`backend/tests/` から内部関数を呼べない）、HTTP 表面は shell スクリプト。

### 9.1 単体テスト（`#[cfg(test)] mod tests` in `domain.rs`、外部 API も DB も不要）

`backend/src/features/relevance/domain.rs` 末尾に追加。Red を先に書く。

| テスト | 意図 |
|---|---|
| `normalize_score_passes_through_unit_range` | `0.0`/`0.5`/`1.0` はそのまま |
| `normalize_score_divides_0_100_scale` | `85.0` → `0.85`、`100.0` → `1.0`（>1.0 は 0–100 とみなす） |
| `normalize_score_clamps_out_of_range` | `-5.0`→`0.0`、`9999.0`→`1.0`、`NaN`/`inf`→`0.0` |
| `parse_scores_happy_path` | 妥当な JSON 配列を `RawScore` に変換、score 正規化済み |
| `parse_scores_strips_prose_and_fences` | ` ```json [..] ``` ` や前後 prose 付きでも `[`〜`]` を抽出 |
| `parse_scores_drops_unknown_ids` | `valid_ids` に無い id / 非 UUID 文字列を捨てる（幻覚対策） |
| `parse_scores_dedupes_by_id` | 同一 id 重複は最初の 1 件のみ |
| `parse_scores_omits_empty_reason` | `reason` が空白/欠落なら `None` |
| `parse_scores_rejects_non_array` | `[` が無い出力は `Err` |
| `build_profile_empty_is_placeholder` | tags も read も空なら `"(no profile yet)"` |
| `build_profile_includes_tags_and_titles` | タグ `name (xN)` 列挙 + 既読タイトル箇条書きが含まれる |
| `profile_fingerprint_is_deterministic` | 同一入力で同一 16 桁 hex、別入力で別値（FNV-1a 不変） |

実行: `cd backend && cargo test relevance`（DB 不要）。`just lint`（clippy `-D warnings` + tsc）も通す。

### 9.2 リポジトリ往復テスト（`#[cfg(test)] mod tests` in `repository.rs`、実 DB / `#[ignore]`）

`DATABASE_URL`（`just dev-db` の DB、マイグレーション適用済み）で実 DB に接続。`#[tokio::test]` + `#[ignore]`。`cargo test -- --ignored` で実行。**LLM は経由しない**（DB 経路のみ自動カバー）。

雛形:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL for repo tests");
        PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap()
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres (DATABASE_URL) with migrations applied"]
    async fn scores_upsert_list_fresh_roundtrip() {
        let pool = pool().await;
        // 既存記事 1 本を用意（feeds→articles を最小シード）。後片付け込み。
        let feed = Uuid::new_v4();
        let art = Uuid::new_v4();
        sqlx::query("INSERT INTO feeds (id, url, title) VALUES ($1,$2,'t')")
            .bind(feed).bind(format!("https://ex.test/{feed}")).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO articles (id, feed_id, url, title, content, is_read) \
                     VALUES ($1,$2,$3,'a','c',false)")
            .bind(art).bind(feed).bind(format!("https://ex.test/a/{art}"))
            .execute(&pool).await.unwrap();

        let scores = vec![RawScore { article_id: art, score: 0.8, reasoning: Some("r".into()) }];
        save_scores(&pool, &scores, "hash-1", "claude-test").await.unwrap();

        let listed = list_scores(&pool).await.unwrap();
        assert!(listed.iter().any(|s| s.article_id == art && (s.score - 0.8).abs() < 1e-6));

        // 現指紋で fresh、別指紋では fresh でない
        assert!(fresh_scored_ids(&pool, "hash-1").await.unwrap().contains(&art));
        assert!(!fresh_scored_ids(&pool, "hash-2").await.unwrap().contains(&art));

        // 再 upsert で上書き（score 変化）
        let scores2 = vec![RawScore { article_id: art, score: 0.2, reasoning: None }];
        save_scores(&pool, &scores2, "hash-2", "claude-test").await.unwrap();
        let listed = list_scores(&pool).await.unwrap();
        let got = listed.iter().find(|s| s.article_id == art).unwrap();
        assert!((got.score - 0.2).abs() < 1e-6);
        assert!(got.reasoning.is_none());

        // 後片付け（CASCADE で score も消える）
        sqlx::query("DELETE FROM feeds WHERE id = $1").bind(feed).execute(&pool).await.unwrap();
    }
}
```

| テスト | 意図 |
|---|---|
| `scores_upsert_list_fresh_roundtrip` | save→list（挿入）→ `fresh_scored_ids`（指紋一致/不一致）→ 再 save（上書き・reasoning NULL 化）→ CASCADE 削除を network 抜きで自動カバー |
| `unread_candidates_excludes_read`（任意） | 既読/未読を挿入し、未読のみ・新しい順で返ることを検証（後片付け込み） |
| `top_tags_orders_by_count`（任意・24 適用環境） | `tags`/`article_tags` をシードし、付与記事数の多い順で返ることを検証 |

### 9.3 HTTP スモークテスト（稼働スタックへの shell スクリプト）

`scripts/test/api-relevance.sh` を新設（`scripts/test/api-stats.sh` と同型。nginx `:8081` 経由）。**Claude を叩かない範囲**を決定的に検証:

| 手順 / アサーション | 意図 |
|---|---|
| `GET /api/relevance/scores` → 200 かつ JSON 配列 | スライス合成 + read model（LLM 非経由） |
| `GET /api/relevance/profile` → 200 かつ `profile`/`hash` キーを持つ | プロファイル可視化（LLM 非経由） |
| `ANTHROPIC_API_KEY` 未設定環境で `POST /api/relevance/score` → 503 | `NotEnabled` を **APIキー判定で先に**返す配線（未読の有無に関わらず 503） |

```bash
#!/usr/bin/env bash
# Smoke test for the relevance slice. Verifies wiring WITHOUT calling Claude.
# Requires: running stack (nginx :8081), jq. Run with ANTHROPIC_API_KEY UNSET
# so POST /score is gated to 503.
set -uo pipefail
BASE="${BASE:-http://localhost:8081}"
fail() { echo "FAIL: $1"; exit 1; }

# scores: 200 + JSON array
b="$(curl -s -m 5 -w '\n%{http_code}' "$BASE/api/relevance/scores")"
code="${b##*$'\n'}"; json="${b%$'\n'*}"
[ "$code" = "200" ] || fail "scores expected 200, got $code ($json)"
case "$json" in "["*) : ;; *) fail "scores not a JSON array: $json";; esac

# profile: 200 + has profile/hash
b="$(curl -s -m 5 -w '\n%{http_code}' "$BASE/api/relevance/profile")"
code="${b##*$'\n'}"; json="${b%$'\n'*}"
[ "$code" = "200" ] || fail "profile expected 200, got $code ($json)"
echo "$json" | jq -e 'has("profile") and has("hash")' >/dev/null \
  || fail "profile missing keys: $json"

# score (no API key): 503
code="$(curl -s -m 5 -o /dev/null -w '%{http_code}' -X POST "$BASE/api/relevance/score")"
[ "$code" = "503" ] || fail "score without API key expected 503, got $code"

echo "PASS: relevance smoke (scores 200 / profile 200 / score gated 503)"
```

> `POST /api/relevance/score` の成功パス（実 Claude 呼び出し）は CI 自動化しない（ライブ APIキーが必要）。手動手順は §10 step 10。

### 9.4 フロント（手動 + 型）
- `tsc`（`just lint`）で `api.ts`（型 3・メソッド 3）/ `store.tsx`（`sort`/`relevanceScores`）/ `ArticleList.tsx`（結合ソート）の型整合を確認。
- 手動: タグを数件付け・記事を数件既読化 → 「スコアリング」→ `scored_count` 表示 → 「重要順」トグルで高スコア記事が上に来る → 各行にスコアバッジ。`ANTHROPIC_API_KEY` 未設定では「スコアリング」が 503 文言になることも確認。

---

## 10. 実装手順（順序付きチェックリスト）

1. **前提確認**: 機能 24（タグ）のマイグレーション（`tags`/`article_tags`）が適用済みか確認（`\dt` 等）。未適用なら 24 を先に出すか、§8 の防御実装を採る。
2. **マイグレーション採番**: `ls backend/migrations/` で最大番号を確認（現状 `0005_search.sql`、24 が `0006_tags.sql` を取る想定）。本機能は `0007_relevance.sql` を §4.2 の SQL で新規作成（既存は触らない）。
3. **shared/llm 拡張（Red 先行可）**: `shared/llm/mod.rs` に `ScorableArticle`/`ScoreRelevanceRequest` と trait メソッド `score_relevance` を追加、`anthropic.rs` に実装（§5.8）。`complete` 再利用。24 と同ファイル同居編集に注意。
4. **ドメイン（Red 先行）**: `features/relevance/domain.rs` を §5.1 で作成 + §9.1 の `#[cfg(test)] mod tests`。落ちる→実装で Green。`cd backend && cargo test relevance`（DB 不要）。
5. **repository**: `repository.rs` を §5.2（`query`/`query_as` のみ、`query!` 禁止）。§9.2 の `#[ignore]` テストも書く。
6. **service**: `service.rs` を §5.3。`llm_client`（NotEnabled）・`current_profile`・`score_unread`・`list_scores`・`profile_view`。チェック順は資格→プロファイル→候補→差分→呼び出し→保存。
7. **handler + mod + 合成**: `handler.rs`（§5.4）、`mod.rs`（§5.5）。`features/mod.rs` に `pub mod relevance;` と `.merge(relevance::routes())`（§5.6）。他スライスは触らない。
8. **ビルド & lint**: `cargo build` → `just lint`（clippy `-D warnings` / tsc）。`cargo fmt`。
9. **DB & テスト**: `just dev-db` → 起動で自動 migrate（または `just migrate`）→ `cargo test relevance`（単体）→ `DATABASE_URL=... cargo test -- --ignored`（往復）。`scripts/test/api-relevance.sh` を作成・`chmod +x`・（APIキー未設定で）実行。
10. **手動 E2E**: `ANTHROPIC_API_KEY` を設定して起動 → タグ付け・既読化で材料を作る → `POST /api/relevance/score` → `GET /api/relevance/scores` でスコアを確認 → `?refresh=true` で再スコア。`GET /api/relevance/profile` でプロファイル内容を目視。
11. **フロント**: `lib/api.ts`（型 3・メソッド 3、§6.1）、`store.tsx`（`sort` + `relevanceScores`、§6.2）、`ArticleList.tsx`（ソートトグル + スコアリングボタン + 結合ソート + スコアバッジ、§6.4）。`just lint` の tsc を通し、重要順ソートを目視確認。
12. **コミット**: マイグレーション・スライス・`shared/llm` 拡張・スクリプト・フロントをまとめて（メッセージ末尾に `Co-Authored-By` 行）。`.env`/秘密はコミットしない。新規マイグレーション番号が連番であることを最終確認。

---

## 11. リスク・未決事項・代替案

- **スコアの陳腐化（プロファイルドリフト）**: スコアは「未読 × そのときのプロファイル」の関数。タグ付けや既読が進むとプロファイルが変わり、過去スコアは古くなる。本書は `profile_hash` で「現プロファイルでスコア済みか」を判定し、`POST /score` が古い/未スコアのみ再計算する設計で緩和。それでも**自動再スコアはしない**（オンデマンド）。ユーザーが「スコアリング」を押すまで古いまま。許容できなければ §下「自動スコアリング」を検討。
- **`max_tokens=1024` によるバッチ切り詰め**: `anthropic.rs::complete` は `max_tokens:1024` 固定。`MAX_BATCH=40` 件 ×（score+reason）の JSON は 1024 トークンを超え、配列途中で切れると `parse_relevance_scores` が `Err`→502 になりうる。**緩和策**: (a) `MAX_BATCH` を 20 程度に下げる、(b) プロンプトで `reason` を省略可/極短に指示する、(c) `complete` を `complete_with(system, user, max_tokens)` に小改修して本メソッドだけ 2048〜4096 を渡す（既存 summarize/translate は 1024 のまま。digest 設計と同じ手）。MVP は `MAX_BATCH=40`＋短 reason で開始し、502 が出たら調整。
- **LLM の id 幻覚・スコア妥当性**: モデルが存在しない id を返す/全件同点を返すことがある。id は `parse_relevance_scores` が `valid_ids` で弾く（幻覚は捨てる→その記事は未スコアのまま末尾）。スコアの質はプロンプト依存で**非決定的**。プロンプト本文（§5.8）は実装時に Anthropic ドキュメント（claude-api スキル / messages API）で確認しつつ調整する前提。返却形が安定しない場合は「整数 0–100 + reason」を「整数のみ」に簡素化してパーサも追従させる。
- **24（タグ）への依存**: タグが 1 つも無いとプロファイルのタグ材料が空になり、既読傾向だけで弱いスコアになる。これは機能の性質上やむなし（タグが育つほど精度が上がる）。`tags`/`article_tags` 不在（24 未適用）では 500 になる（§5.7）。**代替案**: `top_tags` を `to_regclass('tags') IS NOT NULL` で存在確認し、無ければ空 Vec を返す防御実装にすれば 24 無しでも既読傾向だけで動く（依存をソフト化）。本書はハード依存を既定とし、運用順で 24 を先に出す。
- **未読 200 件中 40 件しかスコアしない**: `MAX_BATCH=40`・`unread_candidates` の `LIMIT` により、1 回のスコアリングは新しい未読 40 件のみ。フロントの重要順ソートは「スコア付き 40 件が上、残りは新着順で末尾」。未読が多い環境では「スコアリング」を複数回押すか（毎回新しい未読 40 件を対象に upsert される）、`MAX_BATCH` を上げる（トークン費用とトレードオフ）。将来はページング/全件分割スコアを検討。
- **`profile_fingerprint` の安定性**: FNV-1a を内製し `std` の `DefaultHasher`（バージョン間で値が変わりうる）を避けている。プロファイル文字列が 1 文字でも変われば指紋が変わり再スコア対象になる。これは意図どおり（材料が変われば再評価）だが、既読が 1 件増えるたびに全スコアが stale になる点に注意。粒度を粗くしたいなら `build_profile` の既読タイトル数（`READ_TITLES`）を減らすか、指紋計算からタイトルを外しタグ傾向のみにする（タグ変化でだけ再スコア）。
- **自動スコアリング（非スコープ・将来）**: `shared/scheduler.rs` に「フィード更新後に未読を自動スコア」ループを足せばユーザー操作不要にできる（digest の `spawn_digest` と同型）。トークン費用が読めなくなるため MVP はオンデマンド。River/ダイジェスト連携時にまとめて検討。
- **サーバ側ソート（非スコープ・将来）**: 重要順を一覧 API 側でやるなら、`articles` の `list` を変えず、`relevance` スライスに `GET /api/relevance/articles`（`article_relevance_scores JOIN articles ORDER BY score DESC` の読み取り）を足す。`articles` の書き込み所有を侵さない読み取り JOIN なので Vertical Slice 方針に整合（feed_overview と同型）。MVP はクライアント側ソートで足りるため見送り。
- **`shared/llm` trait 拡張の影響**: `LlmClient` にメソッドを足すと、他にモック実装があれば追従が必要。現状実装は `AnthropicClient` のみ（テストモック未存在）なので影響なし。24 と本機能が並行で `mod.rs`/`anthropic.rs` を編集する場合のマージ衝突に注意（追記位置を分ける）。
- **テスト配置の前提**: 本 crate は binary crate（`lib.rs` 無し）で `backend/tests/` から内部関数を呼べないため、DB 往復は `repository.rs` 内 `#[ignore]`、HTTP 表面は shell スクリプトに置く（§9 冒頭）。これは 03/05/23/24 の各設計書と同じ方針。
