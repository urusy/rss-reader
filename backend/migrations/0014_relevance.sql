-- 0014_relevance.sql
-- AI relevance scores: per-article cache of how relevant each UNREAD article is
-- to the user's interest profile (frequently-used tags + recent read history).
-- Presence + matching profile_hash = cache hit; re-score on ?refresh=true or
-- when the profile drifts.
CREATE TABLE IF NOT EXISTS article_relevance_scores (
    article_id   UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    score        REAL NOT NULL,
    reasoning    TEXT,
    profile_hash TEXT NOT NULL,
    model        TEXT NOT NULL,
    scored_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_article_relevance_score_desc
    ON article_relevance_scores (score DESC);
