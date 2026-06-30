-- Full-text search over articles via pg_trgm (trigram matching).
--
-- Why pg_trgm and not tsvector: PostgreSQL's built-in FTS configurations
-- ('simple'/'english') tokenize on whitespace/punctuation only, so they do not
-- split Japanese (which has no word boundaries) — searching "機械学習" would
-- miss "機械学習の研究". Trigram matching is language-agnostic: it indexes
-- 3-char windows, so substring search works for both Japanese and English
-- without an external tokenizer. pg_trgm is a "trusted" contrib extension
-- bundled with the official postgres image, so no custom image is needed.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- GIN trigram indexes accelerate `title/content ILIKE '%query%'` (any query of
-- 3+ chars). Shorter queries fall back to a sequential scan, which is fine for
-- a single-user, home-network dataset.
CREATE INDEX IF NOT EXISTS idx_articles_title_trgm
    ON articles USING gin (title gin_trgm_ops);

CREATE INDEX IF NOT EXISTS idx_articles_content_trgm
    ON articles USING gin (content gin_trgm_ops);
