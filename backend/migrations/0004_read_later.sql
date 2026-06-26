-- Read-later: per-article state of "save to Instapaper".
-- One row per article (PK = article_id) makes duplicate saves idempotent.
-- status lifecycle: 'pending' (about to send) -> 'added' (Instapaper accepted)
--                                            \-> 'failed' (Instapaper rejected; see last_error)
CREATE TABLE IF NOT EXISTS read_later_items (
    article_id          UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    status              TEXT NOT NULL DEFAULT 'pending'
                          CHECK (status IN ('pending', 'added', 'failed')),
    instapaper_added_at TIMESTAMPTZ,         -- set when status becomes 'added'
    last_error          TEXT,                -- set when status becomes 'failed'
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- For listing failed/pending items (small table; index is optional but cheap).
CREATE INDEX IF NOT EXISTS idx_read_later_status ON read_later_items(status);
