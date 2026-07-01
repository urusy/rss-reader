# 30 ニュースレター → RSS 取り込み

> ステータス: **設計スタブ（約1ページ）**。着手前に本書を §に沿って個別詳細化すること（後述「詳細化の注記」）。
> 読み手向けメモ: このドキュメントは「リポジトリは手元にあるが、この会話の文脈を知らない別セッションの実装者」が全体像を掴むための骨子。具体的なクエリ・型・テストは詳細化フェーズで本書に追記する。

## 1. 概要 / 価値

メール配信のニュースレター（Substack / Mailchimp / 個人ブログのメルマガ等）を **RSS リーダーの中に取り込む**機能。専用の受信アドレス（例: `news@<自宅ドメイン>`）を購読先として各サービスに登録し、届いたメールを parse して、**合成フィード（synthetic feed）の記事**として `articles` に流し込む。RSS を出していないニュースレターも、既存の3ペインリーダー・要約/翻訳・検索・後で読むといった全機能の対象になる。

価値:
- RSS フィードを持たない配信を一元的に読める（リーダー1箇所に集約）。
- 受信したメールはそのまま既存の `articles` パイプラインに乗るので、要約/翻訳キャッシュ・全文検索・既読管理・Instapaper 連携をそのまま再利用できる（新規 UI ほぼ不要）。
- メールを「オンデマンド処理」の入口に変える。届いた時点では DB に素の HTML 本文を保存するだけで、LLM 呼び出しは従来どおりユーザー要求時のみ。

## 2. 想定スライス & テーブル概略

新スライス `backend/src/features/newsletters/`（`domain/repository/service/handler/mod`）。`features/mod.rs` に `.merge(newsletters::routes())` を1行追加。既存スライスは触らない。

メール受信経路（ops、§4）はスライスとは分離する。受信は2方式のいずれかで、**「生メールを取り込み用エンドポイント or キューに渡す」境界までを外部に置き、parse 以降をスライス内に閉じる**。

- **方式A: IMAP ポーリング** — `shared/scheduler.rs` の tick で専用メールボックスを IMAP で取得 → 未処理メールを parse → `articles` へ upsert → サーバ側で既読/削除フラグ。追加依存に IMAP クライアント crate が要る。自宅完結で外部 webhook 不要。
- **方式B: SMTP/インバウンド webhook** — メール受信を MTA（Postfix 等）または外部インバウンドメールサービスが受け、HTTP で `POST /api/newsletters/inbound` に raw MIME を渡す。スライスは HTTP ハンドラだけで済むが、受信インフラの運用が増える。

いずれの方式でも parse は共通（MIME → 件名/本文HTML/送信元/日時を抽出）。**まず方式A（IMAP ポーリング）を第一候補とする**（自宅ネットワーク完結・既存 scheduler に乗る・外部公開不要）。方式Bは将来オプション。

テーブル（新規マイグレーション = **暫定 `0006_newsletters.sql`**。着手直前に `backend/migrations/` の最新番号を必ず確認し繰り下げる。現状の最新は `0005_search.sql`）:

- **`newsletter_sources`** — 合成フィードの定義。送信元アドレス（または購読名）→ 1つの論理フィード。
  - `id UUID PK` / `from_address TEXT`（マッチキー、正規化して UNIQUE）/ `title TEXT` / `feed_id UUID FK(feeds.id)` / `created_at TIMESTAMPTZ`。
  - 各 source は **既存 `feeds` の1行（合成フィード）に対応付ける**。これにより `articles.feed_id` をそのまま使え、フィード一覧・フォルダ分け・統計・既読管理が無改修で効く。合成フィードは `feeds.url` に `newsletter:<source-id>` のような擬似 URL を入れて通常フィードと区別する。
- **`newsletter_messages`**（取り込み台帳・冪等性用）— 受信済みメールの `message_id` を記録し、重複取り込みを防ぐ。
  - `message_id TEXT PK`（RFC 5322 Message-ID）/ `source_id UUID FK` / `article_id UUID FK(articles.id) nullable` / `received_at TIMESTAMPTZ` / `status TEXT CHECK (status IN ('imported','skipped','failed'))` / `last_error TEXT nullable`。

記事本体は **既存 `articles` を再利用**（新カラムなし）。`url` はメール固有の安定値（Message-ID ベースの擬似 URL か、メール内の正規リンク）、`title`=件名、`content`=サニタイズ前提の HTML 本文、`published_at`=メール日時。サニタイズはフロントの `lib/sanitize.ts` が既に担う。

## 3. 主要エンドポイント

最小構成。受信経路（方式A）が中心なので公開 API は管理用に絞る。

| メソッド / パス | 用途 |
|---|---|
| `GET /api/newsletters/sources` | 合成フィード（ニュースレター購読）の一覧。`{ id, from_address, title, feed_id, created_at }` の配列。 |
| `POST /api/newsletters/sources` | 送信元アドレスを購読として登録（対応する合成 `feeds` 行も作成）。body `{ from_address, title? }`。 |
| `DELETE /api/newsletters/sources/{id}` | 購読解除（合成フィード・記事の扱いは詳細化で決定。CASCADE か論理削除か）。 |
| `POST /api/newsletters/poll` | 手動トリガで IMAP 取り込みを即時実行（方式A、scheduler 待ちを回避するデバッグ/運用用）。202/サマリを返す。 |
| `POST /api/newsletters/inbound` | （方式B採用時のみ）raw MIME を受けて取り込む webhook。要認証。 |

取り込み結果の表示は **既存の記事一覧・3ペインリーダーをそのまま使う**ため、専用の閲覧 API は作らない。設定 UI は `/settings`（既存）に「ニュースレター受信アドレス」セクションを追記する想定（フロントは詳細化で）。

## 4. 主なリスク / ops 考慮

- **メール受信経路の運用（最重要）**: IMAP/MTA の認証情報・接続先・TLS を `AppConfig`（環境変数）で持つ。資格情報は `.env` 管理でコミット禁止。方式Aは専用メールボックスの用意（Gmail アプリパスワード等 or 自宅 MTA）が前提。**機能未設定時は `AppError::NotEnabled` を返す**（Anthropic/Instapaper と同じ任意有効パターン）。
- **冪等性**: 同じメールを scheduler が複数回引く／再配送される。`newsletter_messages.message_id` PK で de-dup し、`articles.url` の UNIQUE 制約（既存 upsert 経路）と二重で守る。
- **HTML サニタイズ / セキュリティ**: ニュースレター HTML はトラッキングピクセル・リモート画像・スクリプトを含む。保存は素の HTML、表示時に既存 `lib/sanitize.ts` で除去。トラッキング画像のプロキシ/ブロックは将来課題として明記。
- **MIME パースの堅牢性**: multipart、文字コード（quoted-printable / base64 / 非UTF-8）、`text/html` 不在で `text/plain` のみ等。parse 失敗は1通単位で `status='failed'` + `last_error` に記録し、他メールの取り込みを止めない。crate 選定（`mail-parser` 等）は詳細化で確定。
- **送信元なりすまし / 取り込み対象の限定**: 登録済み `newsletter_sources.from_address` に一致しないメールは取り込まない（無差別取り込み防止）。SPF/DKIM 検証は方式・MTA に依存するため ops 注記に留める。
- **方式Aのポーリング負荷 / 競合**: `shared/scheduler.rs` の tick に相乗りすると既存フィード更新と直列化しうる。間隔・タイムアウトを設定可能にし、長時間 IMAP で全体を止めない設計にする（別 interval or タイムボックス）。
- **保管/肥大化**: メール HTML は記事本文より大きいことがある。`articles.content` の肥大化と検索インデックス（pg_trgm）への影響を観察。必要なら本文トリミングを詳細化で検討。

## 5. 依存（先に必要な機能）

- **ハード依存: なし**。`articles`/`feeds` テーブルと既存パイプラインのみで成立する。合成フィードを `feeds` 行として作るため、フィード一覧・要約/翻訳・検索・既読は無改修で乗る。
- **ソフト依存 / 整合（あると体験が良い）**:
  - 機能02（フォルダ分け）— ニュースレター用フォルダにまとめられると整理しやすい。`feeds.folder_id` を使うだけで追加実装不要。
  - 機能03（feed_overview 統計）— 合成フィードも投稿頻度/最終投稿の集計対象に自動で入る（`feed_id` 単位集計のため）。
  - 機能01（フィード管理 `/manage`）— 合成フィードの改名・フォルダ割当 UI をそのまま流用。
  - 機能05/06（Instapaper / 後で読む）— ニュースレター記事もそのまま「後で読む」対象になる。
- **`apalis` ジョブ化（ロードマップ）との関係**: IMAP ポーリングは将来 apalis のジョブとして per-source スケジュール・リトライに載せ替える余地がある。現状は `shared/scheduler.rs` 相乗りで足りる。

## 6. 工数感

**目安: M〜L**（受信方式の選択で振れる）。

- 方式A（IMAP ポーリング）採用時: **M寄り**。新スライス1枚 + マイグレーション1本 + scheduler への取り込みフック + MIME parse + 冪等台帳。設定 UI は `/settings` への小追記。
- 方式B（SMTP/webhook）採用時、または両対応: **L**。受信インフラ（MTA/外部サービス）の運用設計・webhook 認証・公開エンドポイントのセキュリティが上乗せ。
- 不確実性が大きいのは「メール受信経路の ops」と「実メールでの MIME parse 堅牢性」。ここはコードより**運用・検証コスト**が支配的。プロトタイプ（1配信元・固定メールボックス）で parse 精度を測ってから本実装に入ると見積りが安定する。

## 詳細化の注記

**本書はスタブ（骨子）であり、実装着手前に必ず本書を CHEATSHEET / 既存設計書（特に `03-feed-stats.md`・`05-instapaper-integration.md`）の章立てに沿って個別詳細化すること。** 詳細化で確定すべき項目:

1. 受信方式の最終決定（A/B/両対応）と、それに伴う `AppConfig` 環境変数の確定。
2. `0006_newsletters.sql` の確定（着手直前に最新マイグレーション番号を再確認し必要なら繰り下げ）。`CREATE TABLE` 全文と外部キー/CASCADE 方針。
3. `domain.rs` の値オブジェクト（`from_address` 正規化、擬似 `FeedUrl`、`Message-ID`）と純関数の単体テスト名一覧。
4. MIME parse の crate 選定と、件名/本文HTML/送信元/日時の抽出仕様・失敗時の `status` 遷移。
5. API 契約の request/response JSON 例（§3 各エンドポイント）と `AppError` 使い分け（未設定=`NotEnabled`）。
6. フロント `/settings` セクションと `lib/api.ts` メソッド追加、`lib/sanitize.ts` 適用の確認。
7. TDD 計画（純関数 `#[cfg(test)]` + `scripts/test/api-newsletters.sh` による決定論シード値検証。冪等性の二重取り込みテストを含める）。
8. 依存ドキュメント（01/02/03/05/06）との命名整合の確認。
