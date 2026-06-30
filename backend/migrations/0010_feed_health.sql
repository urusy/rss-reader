-- 0010_feed_health.sql
-- Feed health / liveness tracking. Records each crawl's outcome on the feed row
-- so the UI can flag dead (repeatedly failing) and stale (no recent posts) feeds.
-- feeds queries use explicit column lists (not SELECT *), so adding columns is
-- non-breaking.

ALTER TABLE feeds ADD COLUMN IF NOT EXISTS last_fetch_status       TEXT;
ALTER TABLE feeds ADD COLUMN IF NOT EXISTS last_error              TEXT;
ALTER TABLE feeds ADD COLUMN IF NOT EXISTS consecutive_failures    INTEGER NOT NULL DEFAULT 0;
ALTER TABLE feeds ADD COLUMN IF NOT EXISTS last_fetch_attempted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_feeds_consecutive_failures
    ON feeds (consecutive_failures) WHERE consecutive_failures > 0;
