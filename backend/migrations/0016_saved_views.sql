-- 0016_saved_views.sql
-- Smart views: a saved set of article filter criteria, rendered as a virtual feed
-- in the sidebar. Single-user app, so views are global. Criteria live in `query`
-- (JSONB) as a flat AND of optional fields (see QuerySpec). Resolution is
-- read-only (evaluated against articles at request time; no materialized set).

CREATE TABLE IF NOT EXISTS saved_views (
    id         UUID PRIMARY KEY,
    name       TEXT NOT NULL,
    query      JSONB NOT NULL,
    position   INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_saved_views_name_lower ON saved_views (lower(name));
CREATE INDEX IF NOT EXISTS idx_saved_views_position ON saved_views (position, created_at);
