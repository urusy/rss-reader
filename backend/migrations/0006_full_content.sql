-- Full article body extracted on demand from the article's source URL
-- (DOM heuristic + sanitize). NULL until extraction succeeds; on failure we
-- leave it NULL and keep falling back to articles.content.
-- AI features (summarize/translate/future Ask) read full_content when present.
ALTER TABLE articles
    ADD COLUMN IF NOT EXISTS full_content TEXT,
    ADD COLUMN IF NOT EXISTS extracted_at TIMESTAMPTZ;
