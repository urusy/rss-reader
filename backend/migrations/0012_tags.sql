-- 0012_tags.sql
-- Tags: a flat, user-owned vocabulary for classifying articles. Single-user app,
-- so tags are global. Two attach sources: 'user' and 'ai'. Foundation for
-- digests / smart views / rules (future).

CREATE TABLE IF NOT EXISTS tags (
    id         UUID PRIMARY KEY,
    name       TEXT NOT NULL,
    color      TEXT,
    source     TEXT NOT NULL DEFAULT 'user'
                 CHECK (source IN ('user', 'ai')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Case-insensitive uniqueness: "Rust" and "rust" are the same tag.
CREATE UNIQUE INDEX IF NOT EXISTS idx_tags_name_lower ON tags (lower(name));

CREATE TABLE IF NOT EXISTS article_tags (
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    tag_id     UUID NOT NULL REFERENCES tags(id)     ON DELETE CASCADE,
    source     TEXT NOT NULL DEFAULT 'user'
                 CHECK (source IN ('user', 'ai')),
    confidence REAL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (article_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_article_tags_tag_id ON article_tags(tag_id);

-- AI suggestion cache (presence of a row = cache hit; re-suggest on ?refresh).
CREATE TABLE IF NOT EXISTS article_tag_suggestions (
    article_id   UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    suggestions  JSONB NOT NULL,
    model        TEXT NOT NULL,
    suggested_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
