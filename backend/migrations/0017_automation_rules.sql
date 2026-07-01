-- 0017_automation_rules.sql
-- Custom If/Then rules engine. conditions/actions are kept as JSON strings (TEXT)
-- so we don't add the sqlx `json` feature; the app layer (serde_json) converts.
-- article_scores: write target for the `score` action (this slice owns it).
-- articles.author / rules_applied_at: additive columns for matching + dedupe.

CREATE TABLE IF NOT EXISTS automation_rules (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT        NOT NULL CHECK (length(btrim(name)) > 0),
    enabled     BOOLEAN     NOT NULL DEFAULT true,
    position    INTEGER     NOT NULL DEFAULT 0,
    conditions  TEXT        NOT NULL,
    actions     TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_automation_rules_enabled
    ON automation_rules (enabled, position) WHERE enabled = true;

CREATE TABLE IF NOT EXISTS article_scores (
    article_id UUID        PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    score      INTEGER     NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE articles ADD COLUMN IF NOT EXISTS author TEXT;
ALTER TABLE articles ADD COLUMN IF NOT EXISTS rules_applied_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_articles_rules_pending
    ON articles (feed_id) WHERE rules_applied_at IS NULL;
