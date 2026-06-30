# 32 スター + ハイライト / 注釈

> 読み手向けメモ: これは**設計スタブ（約1ページ）**。着手前に本書を §「詳細化の注記」に従って個別詳細化すること。確認済みの実ファイル: `backend/src/features/articles/{domain,repository,service,handler,mod}.rs`, `backend/src/features/mod.rs`, `backend/src/shared/{error,state}.rs`, `backend/migrations/`（最新 `0005_search.sql`）, `frontend/src/lib/{api.ts,store.tsx}`, `frontend/src/routes/{Reader,ArticleView}.tsx`, `frontend/src/lib/sanitize.ts`。

## 1. 概要 / 価値

記事に**スター（星）**を付け、本文の任意箇所を**ハイライト**して**注釈（メモ）**を残せるようにする。Instapaper（外部・後で読む）とは独立して、**自分の PostgreSQL にローカル保持する「知識ベース」**を作るのが狙い。

- **スター = 軽量な「重要マーク / お気に入り」**。既読/未読・後で読むとは別軸の保存フラグ。一覧から「星付きだけ」を絞り込める。
- **ハイライト = 本文中の選択範囲 + 任意のメモ**。読みながら要点・引用・自分の考えを蓄積する。サニタイズ済み本文（`lib/sanitize.ts` の出力）上での文字オフセット or アンカーで位置を保持する。
- **後続機能の素地**: エクスポート（機能15: バックアップ/復元）で星・ハイライトを書き出す、複数記事 Ask（機能22: Ask Claude）で「星付き/ハイライト群」を文脈として渡す、といった発展の土台になる。本機能自体はそれらに依存しない（被依存側）。

## 2. 想定スライス & テーブル概略

**新スライス `backend/src/features/annotations/`**（`domain` / `repository` / `service` / `handler` / `mod.rs` の5枚）。書き込み主体のローカル CRUD。`features/mod.rs` に `.merge(annotations::routes())` を1行追加。既存 `articles` スライスは触らない（記事本文の参照は `article_id` 相関キーのみ。FK で `articles.id` を参照）。

**新マイグレーション `0006_stars_highlights.sql`**（**着手時に最新番号を再確認**。現状最新は `0005`。既存は編集しない）。2テーブル:

```sql
-- スター: 記事に対する 0/1 のフラグ。記事ごと最大1行（PK=article_id）。
CREATE TABLE article_stars (
  article_id UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ハイライト: 本文中の選択範囲 + 任意メモ。記事ごと複数行。
CREATE TABLE highlights (
  id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  article_id   UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
  quote        TEXT NOT NULL,         -- 選択されたテキスト（表示・再マッチの足場）
  note         TEXT,                  -- 任意の注釈（NULL 可）
  start_offset INTEGER,               -- 本文上の位置アンカー（方式は詳細化で確定）
  end_offset   INTEGER,
  color        TEXT,                  -- ハイライト色（任意・トークン名）
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_highlights_article_id ON highlights(article_id);
```

- `ON DELETE CASCADE` で記事削除時に星・ハイライトも消える（孤児を残さない）。
- 位置アンカー（`start_offset`/`end_offset`）は「サニタイズ後プレーンテキストの文字オフセット」を第一候補とするが、本文再取得で位置がズレる懸念があるため `quote` 文字列マッチを冗長に持つ（詳細化 §2）。

## 3. 主要エンドポイント

`/api/articles/{id}` 配下に同居させ、`annotations/mod.rs` の `routes()` に追加（`articles` の既存ルートとパスは重複しない）。

- `PUT    /api/articles/{id}/star` — スターを付ける（冪等 upsert）。`204`。
- `DELETE /api/articles/{id}/star` — スターを外す。`204`。
- `GET    /api/articles/{id}/highlights` — その記事のハイライト一覧。`200` で配列。
- `POST   /api/articles/{id}/highlights` — 作成。body `{ quote, note?, start_offset?, end_offset?, color? }` → `201` で作成行。
- `PATCH  /api/highlights/{hid}` — メモ/色の更新。`200`。
- `DELETE /api/highlights/{hid}` — 削除。`204`。
- 星付き絞り込みは `articles` の `list` に `starred_only` を足す（スライス拡張）か、本スライスに `GET /api/stars`（星付き記事 id 一覧）を置く案がある。**どちらにするかは詳細化で要判断**。

不正入力（空 `quote` 等）は `AppError::Validation`（400）、対象記事/ハイライト不在は `AppError::NotFound`（404）。新バリアントは追加しない。

## 4. 主なリスク / ops 考慮

- **ハイライト位置の安定性**: フィードの content が再取得・抽出強化（機能13）で変わると文字オフセットがズレて復元できない。`quote` 文字列の再マッチをフォールバックに持ち、完全一致しなければ「位置不明ハイライト」としてメモだけ残す。アンカー方式（offset / DOM range シリアライズ / quote+前後文脈）の選択は詳細化の最重要論点。
- **サニタイズとの整合**: 表示は `lib/sanitize.ts` 後の DOM。ハイライト描画は sanitize 後テキストへ後付けで `<mark>` を挿入するため、サニタイズ規則変更の影響を受ける。オフセットも sanitize 後基準で統一する。
- **全文検索（search / 0005）との関係**: `quote`/`note` を pg_trgm 検索対象に含めるかは将来課題。本チケットでは含めない（既存 search スライスは触らない）。
- **ローカル保持の方針明確化**: 星・ハイライトは**Instapaper へは送らない**ローカル知識ベース。外部同期は機能15/29（sync-api）側の責務。本機能はクラウド送信・鍵管理・従量コストを一切持ち込まない（DB 行のみ）。
- **ops**: 追加テーブル2枚・インデックス1本のみ。バックアップは既存 PostgreSQL ダンプに自然に含まれる。

## 5. 依存（先に必要な機能）

- **ハード依存: なし**。`articles` テーブル（`0001_init.sql`）と本文表示（`ArticleView` + `sanitize.ts`）だけで成立。
- **相補（あると望ましい）**: 機能10（二ペインリーダー）配下の `ArticleView` にハイライト UI を載せると自然。星の絞り込みは機能11（未読フィルタ）/09（既読管理）の一覧フィルタ基盤に相乗りできる。
- **被依存（このスタブが素地になる）**: 機能15（バックアップ/復元・エクスポート）、機能22（複数記事 Ask Claude の文脈源）。いずれも本機能が先にあると活きるが、本機能はそれらを待たない。

## 6. 工数感

- **スター単体: S**。`article_stars` 1テーブル + upsert/delete 2エンドポイント + 一覧フィルタ + UI の星トグル。半日〜1日級。
- **ハイライト/注釈: M〜L**。位置アンカー方式の確定・本文上の選択 UI・`<mark>` 描画・メモ編集・CRUD 4エンドポイント・`scripts/test/api-annotations.sh` の TDD。フロントの選択範囲取得とサニタイズ整合が読みにくい要素。
- 推奨: **スターを先に単独出荷**（即価値・低リスク）し、ハイライト/注釈を同スライス内で第2弾として追加する。

## 7. 詳細化の注記

本書はスタブ。**実装着手前に本書を個別詳細化**し、CHEATSHEET / 既存設計書（特に `03-feed-stats.md`・`05-instapaper-integration.md` の章立て）に揃えて以下を確定すること:

1. 出荷スコープ分割（スター単独 → ハイライト/注釈、の2段を既定とする）。
2. ハイライト位置アンカー方式（文字オフセット / quote+前後文脈 / DOM range）と、本文変化時の復元フォールバック。
3. 星付き絞り込みを `articles` スライス拡張（`?starred=true`）にするか本スライスの独立エンドポイントにするか。
4. フロント: 本文選択 → ハイライト作成 UI、`<mark>` 描画とサニタイズ整合、メモ編集 UI（自前 Tailwind / 必要なら Ark UI）。`lib/api.ts` 型・メソッド追加、`store.tsx` の星状態管理要否。
5. 最新マイグレーション番号の再確認（`0006_…`）、`article_stars` / `highlights` スキーマ確定、`ON DELETE CASCADE` の挙動確認。
6. TDD: `scripts/test/api-annotations.sh`（star upsert 冪等 / highlight CRUD / Validation / NotFound）と純粋ロジックの `#[cfg(test)]`。
7. README.md のマイグレーション登録表（0006）・機能マトリクス・依存グラフ・リスク表への追記。
