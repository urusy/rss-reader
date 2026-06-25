-- Feeds: subscribed RSS/Atom sources.
CREATE TABLE IF NOT EXISTS feeds (
    id               UUID PRIMARY KEY,
    url              TEXT NOT NULL UNIQUE,
    title            TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_fetched_at  TIMESTAMPTZ
);

-- Articles: entries pulled from feeds. summary_/translation_ columns are the
-- on-demand Claude cache (null until the user requests processing).
CREATE TABLE IF NOT EXISTS articles (
    id                UUID PRIMARY KEY,
    feed_id           UUID NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    url               TEXT NOT NULL UNIQUE,
    title             TEXT NOT NULL,
    content           TEXT NOT NULL DEFAULT '',
    published_at      TIMESTAMPTZ,
    is_read           BOOLEAN NOT NULL DEFAULT false,
    summary           TEXT,
    summary_lang      TEXT,
    translation       TEXT,
    translation_lang  TEXT,
    processed_at      TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_articles_feed_id      ON articles(feed_id);
CREATE INDEX IF NOT EXISTS idx_articles_published_at ON articles(published_at DESC NULLS LAST);
CREATE INDEX IF NOT EXISTS idx_articles_is_read      ON articles(is_read) WHERE is_read = false;
