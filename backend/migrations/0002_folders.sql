-- 0002_folders.sql
-- Folders: user-defined categories for organizing feeds. Flat (no nesting) for now.
CREATE TABLE IF NOT EXISTS folders (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL,
    position    INTEGER NOT NULL DEFAULT 0,   -- display order; editing UI is future scope
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A feed belongs to at most one folder. NULL = "未分類" (unclassified).
-- The FK is the real integrity guard for folder assignment.
-- ON DELETE SET NULL: deleting a folder moves its feeds back to unclassified
-- (never deletes feeds/articles). This is what makes "未分類" robust.
ALTER TABLE feeds
    ADD COLUMN IF NOT EXISTS folder_id UUID REFERENCES folders(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_feeds_folder_id ON feeds(folder_id);
