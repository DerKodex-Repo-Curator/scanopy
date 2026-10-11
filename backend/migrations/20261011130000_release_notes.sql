-- Release notes: which server versions have been announced, and which release each user has seen.
--
-- `release_announcements` holds one row per server version this database has run. The first
-- container to start on a version inserts its row, under an advisory lock, and sends the release
-- email if that release asks for one. `email_sent_at` records that send. It stays NULL for a release
-- shown in the app only, and for the version a fresh install starts on.
--
-- `users.release_notes_seen_version` is the newest release whose notes the user has dismissed.
-- New users start at the server's version. NULL marks a user who predates this column.
--
-- Expand only: a new table and a nullable column, nothing dropped or rewritten.

SET lock_timeout = '5s';
SET statement_timeout = '5s';

CREATE TABLE IF NOT EXISTS release_announcements (
    id UUID PRIMARY KEY,
    version TEXT NOT NULL,
    email_sent_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The table is new and empty, so a plain build holds its lock for no time.
-- squawk-ignore require-concurrent-index-creation
CREATE UNIQUE INDEX IF NOT EXISTS idx_release_announcements_version
    ON release_announcements (version);

ALTER TABLE users ADD COLUMN IF NOT EXISTS release_notes_seen_version TEXT;
