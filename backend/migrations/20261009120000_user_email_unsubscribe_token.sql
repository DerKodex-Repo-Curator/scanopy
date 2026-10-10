-- DOWNTIME MIGRATION
--
-- Brevo retirement: adds the one-click unsubscribe token and drops the Brevo company id.
--
-- `users.email_unsubscribe_token`: per-user token for the one-click unsubscribe link (RFC 8058)
-- carried by pausable emails. The column is nullable with no default, so PostgreSQL records it in
-- the catalog without rewriting `users`. The server mints a token the first time it sends a user a
-- pausable email. No backfill: no earlier column held this value.
--
-- `organizations.brevo_company_id`: dropped. DATA LOST: the Brevo company id stored per
-- organization. It is discarded, not carried forward, because it only pointed at a record in the
-- Brevo CRM, which is being closed. Discarding it was approved by Maya on 2026-10-09. The release
-- notes name the loss.
--
-- WHY THIS NEEDS A DOWNTIME DEPLOY: containers from the previous release read and write
-- `brevo_company_id`, so under a rolling deploy their organization reads and writes would fail
-- once it is gone. It ships in the same downtime release as
-- 20261009120100_tag_groups.sql. Rollback is roll-forward only. Registered in
-- DOWNTIME_FILES in backend/scripts/lint-migrations.sh.
--
-- No view, index, constraint or trigger references the dropped column, so no CASCADE is needed.
-- DROP COLUMN is metadata-only, so the ACCESS EXCLUSIVE lock is brief.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

ALTER TABLE users ADD COLUMN IF NOT EXISTS email_unsubscribe_token UUID;

ALTER TABLE organizations DROP COLUMN brevo_company_id;
