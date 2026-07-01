-- #31 PWA Web Push 通知。フィード優先度・購読・通知ウォーターマークを追加する。
-- 既存ファイルは編集せず追記のみ（マイグレーション規約）。

-- フィード優先度: 0=通常 / 1=高。高優先のフィードの新着だけを通知対象にする。
-- 当面は2値だが、将来の段階拡張余地で SMALLINT。既存行は 0（通常）に落ちる。
ALTER TABLE feeds ADD COLUMN IF NOT EXISTS priority SMALLINT NOT NULL DEFAULT 0;

-- Web Push Subscription（ブラウザの PushSubscription 標準フィールド）。
-- endpoint は購読の一意キー。同一ブラウザの再購読は endpoint で dedupe/更新する。
CREATE TABLE IF NOT EXISTS push_subscriptions (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    endpoint   TEXT NOT NULL UNIQUE,
    p256dh     TEXT NOT NULL,
    auth       TEXT NOT NULL,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 通知の高ウォーターマーク（シングルトン）。「ここまでの created_at は通知済み」の1行。
-- now() で seed するので、初回サイクル（＝再起動直後）に既存記事が一斉通知されない。
CREATE TABLE IF NOT EXISTS push_notify_state (
    id               BOOLEAN PRIMARY KEY DEFAULT true,
    last_notified_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT push_notify_state_singleton CHECK (id)
);
INSERT INTO push_notify_state (id) VALUES (true) ON CONFLICT (id) DO NOTHING;
