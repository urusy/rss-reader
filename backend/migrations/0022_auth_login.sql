-- ログイン機能: パスワード + サーバー側セッション(HttpOnly Cookie)。
-- 既存ファイルは編集せず追記のみ(マイグレーション規約)。

-- 単一ユーザーの資格情報(シングルトン行。push_notify_state と同型)。
-- 行が無い = 初回セットアップ未完了。パスワードは Argon2id の PHC 文字列のみ保存。
CREATE TABLE IF NOT EXISTS auth_credential (
    id            BOOLEAN PRIMARY KEY DEFAULT true
                  CONSTRAINT auth_credential_singleton CHECK (id),
    password_hash TEXT NOT NULL,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- サーバー側セッション。トークンは SHA-256(base64url) のみ保存し、平文は
-- Set-Cookie で一度クライアントへ渡すだけ(DB ダンプ流出でもセッション奪取不可)。
CREATE TABLE IF NOT EXISTS auth_sessions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash   TEXT NOT NULL UNIQUE,
    label        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires_at ON auth_sessions (expires_at);
