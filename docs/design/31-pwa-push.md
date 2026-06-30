# 31 PWA プッシュ通知（フィード優先度）

> 設計スタブ（約1ページ）。本書は方向性の合意用であり、**詳細は実装前にこの 31-pwa-push.md を個別詳細化してから着手すること**（データモデル確定・Web Push 鍵管理・SW 設計の章を本書に追記する）。粒度の基準は `03-feed-stats.md` / `05-instapaper-integration.md` を参照。

## 1. 概要 / 価値

高優先フィードに新着が入ったとき、ブラウザ／ホーム画面アプリ（既存 PWA）に **Web Push 通知** を出す。全フィードを無差別に通知すると鬱陶しいので、フィード単位の **優先度（`feed.priority`）** を導入し、高優先のものだけを通知対象にする。

価値: 重要な発信元（公式アナウンス、推し著者など）の新着を、アプリを開かずに受け取れる。家庭内・単一ユーザ前提だが、複数デバイス（iPhone/iPad/PC）に購読を張れるようにする。通知配送はスケジューラのフィード取得（`shared/scheduler.rs::refresh_all_feeds`）の延長で発火させる。

任意連携として、Web Push の代わり／併用で **ntfy / Gotify への webhook** をサポートする（セルフホスト勢が既存の通知基盤に流せるように）。Web Push が主、webhook は opt-in のサブ経路。

## 2. 想定スライス & テーブル概略

**新スライス `backend/src/features/notifications/`**（`domain/repository/service/handler/mod`）。`features/mod.rs` に `.merge(notifications::routes())` 1行追加。Web Push の鍵生成・配送は trait 化しない（差し替え予定なし。`shared/llm` 以外の抽象境界は作らない方針に従う）。

スケジューラ連携は `shared/scheduler.rs` の取得ループ末尾で `notifications::service::notify_new_articles(&state, &newly_inserted)` を呼ぶ薄い1行に留める（越境共通レイヤーは作らない）。新着判定は既存 upsert で「実際に挿入された記事」を使う。

**マイグレーション（次の空き番号 = `0006`）**。`backend/migrations/` の最新は `0005_search.sql`。**着手前に最新番号を再確認**（並行開発で先に消費されうる）。

- `0006_push_notifications.sql`:
  - `ALTER TABLE feeds ADD COLUMN priority SMALLINT NOT NULL DEFAULT 0;`
    （0=通常 / 1=高。当面は2値。将来の段階拡張余地で SMALLINT）
  - `CREATE TABLE push_subscriptions ( id UUID PK, endpoint TEXT NOT NULL UNIQUE, p256dh TEXT NOT NULL, auth TEXT NOT NULL, user_agent TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT now() );`
    （Web Push Subscription の標準フィールド。endpoint で dedupe）
  - VAPID 鍵はテーブルに置かず **環境変数**（`config.rs` に `vapid_public_key` / `vapid_private_key` を `Option<String>` で追加）。未設定時は機能無効 = `AppError::NotEnabled`（Instapaper / Anthropic と同じ既存パターン）。
  - webhook 先（ntfy/Gotify URL・トークン）も環境変数で扱い、DB テーブルは作らない（singleton 設定。専用テーブルは過剰）。

ドメイン: `FeedPriority`（newtype/enum、不正値を構築時に弾く）、`PushSubscription`（endpoint を値オブジェクト化）。通知ペイロード組み立ては純関数にして単体テスト対象にする。

## 3. 主要エンドポイント

| メソッド | パス | 用途 |
|---|---|---|
| `GET`  | `/api/push/public-key` | VAPID 公開鍵を返す（SW 購読時に必要）。未設定なら 503 `NotEnabled` |
| `POST` | `/api/push/subscribe` | `PushSubscription` を登録（endpoint で upsert） |
| `POST` | `/api/push/unsubscribe` | endpoint 指定で購読解除 |
| `PATCH`| `/api/feeds/{id}/priority` | フィード優先度を更新（`{ "priority": 0\|1 }`） |
| `POST` | `/api/push/test` | 任意: 登録済み購読へテスト通知を送る（疎通確認用） |

レスポンスは既存 `AppError`/`AppResult` 規約に従う。`/api/feeds/{id}/priority` を本スライスに置くか `feeds` スライスへ寄せるかは詳細化で確定（priority は通知関心事だが feeds の同一アグリゲート）。

フロント: 既存 PWA に **service worker の `push` / `notificationclick` ハンドラ**を追加。`lib/api.ts` に `subscribePush()` / `setFeedPriority()` 等を追加。`routes/Settings.tsx` に通知許可ボタン、`FeedManage.tsx` に優先度トグル（`switch.tsx`/`badge.tsx` 再利用）。

## 4. 主なリスク / ops 考慮

- **iOS の Web Push 制約**: iOS Safari は「ホーム画面に追加した PWA」かつ iOS 16.4+ でないと Web Push 不可。ユーザーのデバイス前提（MEMORY: 実機検証は iOS 必須）を満たすか実機確認が要る。非対応環境では UI を機能無効表示にフォールバック。
- **VAPID 鍵管理**: 鍵をローテートすると既存購読が無効化する。鍵は env で固定し、コミットしない（`.env` 非コミット規約）。Rust 側の Web Push 実装は crate（`web-push` 等）採用か、自前 ECDSA/JWT + reqwest（Anthropic 実装と同様の薄い HTTP）かを詳細化で決定。
- **配送失敗 / 失効購読**: push 送信で 404/410 が返ったら該当 `push_subscriptions` 行を削除（GC）。スケジューラ発火なので失敗はログのみで握りつぶし、取得ループを止めない（`scheduler.rs` の既存エラー方針に合わせる）。
- **重複通知**: 同じ記事を二重に新着判定しないこと。`articles.upsert` の「実際に挿入された行」のみ通知対象にする（URL UNIQUE で dedupe 済み）。再起動直後の一斉通知に注意（初回取得は通知抑制を検討）。
- **通知量**: 高優先フィードのバースト投稿で通知が溢れる。1サイクルで「フィードあたり N 件まで」または「まとめ通知（"feed X に新着3件"）」へ集約する案を詳細化で決める。
- **webhook 任意経路**: ntfy/Gotify URL 未設定なら何もしない。設定時のみ並行送信。タイムアウト短め・失敗はログのみ。

## 5. 依存（先に必要な機能）

- **PWA 基盤（実装済み）**: 既に installable PWA は導入済み（MEMORY: responsive-design、現状 SW なし）。本機能で **service worker を初めて常駐実装**することになるため、SW ライフサイクル（registration / 既存 manifest との整合）を詳細化で確認。
- **フィード管理 UI（機能01 / `FeedManage.tsx`）**: 優先度トグルの置き場所として望ましい（ソフト依存）。未着地でも `Settings` 等へ暫定配置で単独着手可能。
- **スケジューラ**: `shared/scheduler.rs` の取得ループに発火フックを1行追加。apalis 移行（ロードマップ）と競合しないよう、フックは「新着リストを渡して通知サービスを呼ぶ」純粋な接点に限定する。
- ハード依存なし（`dependsOn` は空。01 はソフト依存）。

## 6. 工数感

**L（大）**。体感内訳:
- バックエンド（スライス + 0006 マイグレーション + Web Push 送信実装 + scheduler フック）= M〜L。VAPID/ペイロード暗号化が重み（crate 採用なら軽減）。
- フロント（SW の push/click ハンドラ初実装 + 購読フロー + 優先度 UI）= M。SW デバッグ（特に iOS 実機）が読めない。
- webhook 任意経路 = S（後回し可、MVP から外して別チケット化も可）。

MVP を「Web Push + 2値 priority + テスト通知」に絞れば M に収まる。webhook・通知集約・iOS 実機検証を切り出すと段階投入しやすい。

---

> 再掲: 本書はスタブ。**実装前に本 31-pwa-push.md を、データモデル確定・VAPID 実装方式・SW 設計・通知集約ポリシー・TDD テスト計画・順序付き実装手順・リスク表まで含めて個別詳細化すること**（`03` / `05` の章立て・粒度に合わせる）。マイグレーション番号は着手直前に `backend/migrations/` で最新を再確認し、README の Migration Register を更新する。
