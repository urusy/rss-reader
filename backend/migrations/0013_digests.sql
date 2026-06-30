-- AI daily digest. One row per calendar day (UTC). Aggregates the previous 24h
-- of unread articles into topic-grouped Markdown via Claude, cached so same-day
-- re-requests cost no tokens (mirrors summary/translation caching).
CREATE TABLE IF NOT EXISTS digests (
    date          DATE PRIMARY KEY,
    markdown      TEXT NOT NULL,
    model         TEXT NOT NULL,
    article_count INTEGER NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_digests_date_desc ON digests (date DESC);
