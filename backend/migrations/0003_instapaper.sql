-- Instapaper credentials. Single-user app => singleton row pinned to id = 1.
-- Stored reversibly (plaintext for MVP) because the Simple Developer API needs
-- the cleartext password to perform HTTP Basic auth on every request.
-- GET /status never returns the password; encryption-at-rest is a future step.
CREATE TABLE IF NOT EXISTS instapaper_credentials (
    id         INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    username   TEXT NOT NULL,
    password   TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
