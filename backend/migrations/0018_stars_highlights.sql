-- Feature 32: stars + highlights/annotations (local knowledge base).
-- Additive only. Both tables cascade-delete with their article (no orphans).

-- Star: a 0/1 importance flag, at most one row per article (PK = article_id).
CREATE TABLE IF NOT EXISTS article_stars (
    article_id UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Highlight: a selected text range + optional note. Many rows per article.
-- The quote string is the durable anchor; offsets are a best-effort hint that
-- may drift when content is re-fetched/re-extracted (feature 13).
CREATE TABLE IF NOT EXISTS highlights (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    article_id   UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    quote        TEXT NOT NULL,
    note         TEXT,
    start_offset INTEGER,
    end_offset   INTEGER,
    color        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_highlights_article_id ON highlights(article_id);
