-- Ask Claude: optional persisted Q&A log per article.
-- Only written when a request sets save=true; the chat itself is stateless
-- (the client resends the full messages[] each turn). This is a read-back log,
-- not a session store.
CREATE TABLE IF NOT EXISTS article_notes (
    id          UUID PRIMARY KEY,
    article_id  UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    role        TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content     TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_article_notes_article_id
    ON article_notes (article_id, created_at);
