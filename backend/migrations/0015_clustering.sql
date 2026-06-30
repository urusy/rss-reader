-- Semantic clustering & deduplication. A background job groups recent articles
-- covering the same story (pg_trgm title similarity + union-find) so the UI can
-- show one card per topic. An optional Claude cross-outlet summary is cached per
-- cluster. The tables are fully rebuilt on each recluster (delete+insert).
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE IF NOT EXISTS article_clusters (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title          TEXT NOT NULL,
    size           INTEGER NOT NULL,
    signature      TEXT NOT NULL,
    summary        TEXT,
    summary_lang   TEXT,
    summary_model  TEXT,
    summarized_at  TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_article_clusters_size ON article_clusters (size DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_article_clusters_signature
    ON article_clusters (signature);

CREATE TABLE IF NOT EXISTS cluster_members (
    cluster_id        UUID NOT NULL REFERENCES article_clusters(id) ON DELETE CASCADE,
    article_id        UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    is_representative  BOOLEAN NOT NULL DEFAULT false,
    is_duplicate       BOOLEAN NOT NULL DEFAULT false,
    similarity         REAL NOT NULL DEFAULT 0,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_cluster_members_cluster_id
    ON cluster_members (cluster_id);
