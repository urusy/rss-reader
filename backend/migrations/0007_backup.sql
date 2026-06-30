-- Audit log for the OPTIONAL scheduled pg_dump task (shared/scheduler.rs と同型の
-- tokio::interval から呼ばれる)。core の export/import はこのテーブルを使わない。
-- BACKUP_DIR + BACKUP_PGDUMP_INTERVAL_SECS が未設定ならスケジューラは起動せず、
-- このテーブルは空のまま残る（実害なし）。
CREATE TABLE IF NOT EXISTS backup_runs (
    id          UUID PRIMARY KEY,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    status      TEXT NOT NULL DEFAULT 'running'
                CHECK (status IN ('running', 'succeeded', 'failed')),
    file_path   TEXT,
    byte_size   BIGINT,
    error       TEXT
);

CREATE INDEX IF NOT EXISTS idx_backup_runs_started_at
    ON backup_runs (started_at DESC);
