-- Read-on-Save settings. Single-user app => singleton row pinned to id = 1.
-- When mark_read_on_save = true, a successful POST /api/read-later also marks
-- the article as read (articles.is_read = true) to keep unread counts honest.
-- Default is false to preserve existing behavior until the user opts in.
CREATE TABLE IF NOT EXISTS read_later_settings (
    id                INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    mark_read_on_save BOOLEAN NOT NULL DEFAULT false,
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO read_later_settings (id, mark_read_on_save)
VALUES (1, false)
ON CONFLICT (id) DO NOTHING;
