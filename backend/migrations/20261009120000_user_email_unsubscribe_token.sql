-- Per-user token for the one-click unsubscribe link (RFC 8058) carried by pausable emails.
--
-- Additive only. The column is nullable with no default, so PostgreSQL records it in the catalog
-- without rewriting `users`. The server mints a token the first time it sends a user a pausable
-- email. A server still on the previous release omits the column from its writes and leaves it
-- NULL. No backfill: no earlier column held this value.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

ALTER TABLE users ADD COLUMN IF NOT EXISTS email_unsubscribe_token UUID;
