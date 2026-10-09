-- DOWNTIME MIGRATION
--
-- Drops `tags.is_application`. 20261009120100_system_tag_groups.sql, which runs just before
-- this, copied every true value into `tags.tag_group = 'Application'` on live, closed and
-- snapshot rows, so nothing is lost.
--
-- WHY THIS NEEDS A DOWNTIME DEPLOY: containers from the previous release read and write
-- `is_application`. Under a rolling deploy an old container would still be serving traffic while
-- this drops the column, and every tag read and write it made would fail. The release that
-- carries this migration MUST be cut with `Deploy-Mode: downtime` in the release body (select
-- "downtime" in the prepare_release workflow dispatch). Without that marker release.yml runs the
-- backward-compat container harness against a schema that assumes otherwise, and the deploy
-- itself may be performed as a rolling one. Rollback is roll-forward only.
--
-- Squawk flags every DROP COLUMN as unsafe, because correctness under a zero-downtime deploy
-- requires expand-and-contract. The downtime window replaces that sequencing here. Registered in
-- DOWNTIME_FILES in backend/scripts/lint-migrations.sh.
--
-- No view, index, constraint or trigger references the column, so no CASCADE is needed.
-- DROP COLUMN is metadata-only, so the ACCESS EXCLUSIVE lock is brief.

SET lock_timeout = '5s';
SET statement_timeout = '30s';

ALTER TABLE tags
    DROP COLUMN is_application;
