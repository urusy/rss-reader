-- 19 ミュート（NGワード）フィルタ。
-- mute_rules: field（一致対象カラム）× pattern（部分一致語）× action（hide/mark_read）。
--   match_type は前方互換のため列だけ用意し、v1 は 'contains' のみ。
-- articles.muted_at: hide ルール合致のスタンプ。NULL=表示。apply で再計算する。
--   mark_read は既存 articles.is_read を使うので新カラム不要。

CREATE TABLE IF NOT EXISTS mute_rules (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    field       TEXT        NOT NULL CHECK (field IN ('title', 'content', 'url')),
    pattern     TEXT        NOT NULL CHECK (length(btrim(pattern)) > 0),
    match_type  TEXT        NOT NULL DEFAULT 'contains' CHECK (match_type IN ('contains')),
    action      TEXT        NOT NULL DEFAULT 'hide'     CHECK (action IN ('hide', 'mark_read')),
    enabled     BOOLEAN     NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_mute_rules_enabled
    ON mute_rules (enabled) WHERE enabled = true;

ALTER TABLE articles
    ADD COLUMN IF NOT EXISTS muted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_articles_muted_at
    ON articles (muted_at) WHERE muted_at IS NULL;
