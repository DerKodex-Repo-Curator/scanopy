-- no-transaction
--
-- DOWNTIME MIGRATION
--
-- Tag groups become their own table. A tag referenced its group by name in `tags.tag_group`, and
-- application tags were flagged by `tags.is_application`. Both columns are replaced by
-- `tags.tag_group_id`, a reference to a `tag_groups` row, and tags in a group gain `position`, their
-- order within it. Where each value goes:
--
--   is_application = true                  -> the organization's Application group
--   a named group 'application', any case  -> renamed to '<name> (custom)', then a Custom group of
--                                             that name. No organization can name a group after a
--                                             system group, and folding it in would put its
--                                             entities into the application view.
--   a named group 'status', any case       -> the organization's Status group. An entity left
--                                             holding two Status tags keeps the one it got last;
--                                             the other assignment is closed, not deleted, so it
--                                             stays in the entity's history.
--   any other tag_group value               -> a Custom group of exactly that name, one per
--                                             organization and name, created from every row
--                                             version that carries the name.
--   tag_group IS NULL, not application      -> tag_group_id IS NULL (no group).
--
-- Every organization gets an Application and a Status group, whether or not it has tags.
--
-- `tags` and `entity_tags` are SCD2. The `tags` updates cover live, closed (`valid_to IS NOT NULL`)
-- and snapshot (`snapshot_id IS NOT NULL`) rows and touch no validity, lineage or snapshot column,
-- so every row version keeps its group. `position` is set on live rows of ordered groups only: it
-- is the order the group's tags are listed in now, and a closed or snapshot row is never listed.
-- Live tags of a seeded starter group keep the seeded order; any other tag follows, by name. The
-- Application group is created unordered (`ordered = false`): applications have no sequence, so
-- its tags list by name and carry no position. Every other group starts ordered. The dedupe closes
-- live assignments only; snapshot rows are a frozen record and stay as they were.
--
-- Nothing is discarded: after the backfill every non-null `tag_group` and every true
-- `is_application` is held by `tag_group_id`, and only then are the two columns dropped.
--
-- WHY THIS NEEDS A DOWNTIME DEPLOY: containers from the previous release read and write
-- `tags.tag_group` and `tags.is_application`. Under a rolling deploy an old container would still
-- be serving traffic while this drops them, and every tag read and write it made would fail. The
-- release that carries this migration MUST be cut with `Deploy-Mode: downtime`. Rollback is
-- roll-forward only. Registered in DOWNTIME_FILES in backend/scripts/lint-migrations.sh.
--
-- Updates to `tags` run in batches of 1000 rows with a COMMIT per batch (hence `-- no-transaction`),
-- keyset-paginated by `id`. Every step before the final drop is idempotent, so a run that fails
-- part-way can be re-run.

SET lock_timeout = '5s';
SET statement_timeout = '0';

CREATE TABLE IF NOT EXISTS tag_groups (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    group_type TEXT NOT NULL,
    entity_types TEXT[],
    ordered BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The table is new and empty, so a plain build holds its lock for no time.
-- squawk-ignore require-concurrent-index-creation
CREATE UNIQUE INDEX IF NOT EXISTS idx_tag_groups_org_name ON tag_groups (organization_id, name);
-- squawk-ignore require-concurrent-index-creation
CREATE UNIQUE INDEX IF NOT EXISTS idx_tag_groups_org_system
    ON tag_groups (organization_id, group_type)
    WHERE group_type <> 'Custom';

ALTER TABLE tags ADD COLUMN IF NOT EXISTS tag_group_id UUID;
-- A group holds tens of tags, never 2^31.
-- squawk-ignore prefer-bigint-over-int
ALTER TABLE tags ADD COLUMN IF NOT EXISTS position INTEGER;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'tags_tag_group_id_fkey') THEN
        ALTER TABLE tags
            ADD CONSTRAINT tags_tag_group_id_fkey
            FOREIGN KEY (tag_group_id) REFERENCES tag_groups(id) ON DELETE SET NULL
            NOT VALID;
    END IF;
END $$;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tags_tag_group_id ON tags (tag_group_id);

-- A named group 'application' can't keep a system group's name.
UPDATE tags
   SET tag_group = tag_group || ' (custom)'
 WHERE NOT is_application
   AND lower(tag_group) = 'application';

-- Application tags were flagged, not named.
DO $$
DECLARE
    last_id UUID := '00000000-0000-0000-0000-000000000000';
    batch UUID[];
BEGIN
    LOOP
        SELECT array_agg(id ORDER BY id)
          INTO batch
          FROM (SELECT id FROM tags
                 WHERE id > last_id AND is_application
                 ORDER BY id LIMIT 1000) t;

        EXIT WHEN batch IS NULL;

        UPDATE tags SET tag_group = 'Application' WHERE id = ANY(batch);

        last_id := batch[array_length(batch, 1)];
        COMMIT;
    END LOOP;
END $$;

-- Fold every spelling of 'status' into the system group.
UPDATE tags
   SET tag_group = 'Status'
 WHERE lower(tag_group) = 'status'
   AND tag_group <> 'Status';

-- An entity may hold one tag of a group: keep the Status tag it got last.
WITH held AS (
    SELECT et.id,
           row_number() OVER (
               PARTITION BY et.entity_id, et.entity_type
               ORDER BY et.valid_from DESC, et.id
           ) AS rank
      FROM entity_tags et
      JOIN tags t ON t.id = et.tag_id
     WHERE t.tag_group = 'Status'
       AND et.valid_to IS NULL
       AND et.snapshot_id IS NULL
)
UPDATE entity_tags
   SET valid_to = now()
  FROM held
 WHERE entity_tags.id = held.id
   AND held.rank > 1;

-- The system groups, for every organization. Applications have no order; statuses do.
INSERT INTO tag_groups (id, organization_id, name, group_type, ordered)
SELECT gen_random_uuid(), o.id, s.name, s.name, s.name <> 'Application'
  FROM organizations o
 CROSS JOIN (VALUES ('Application'), ('Status')) AS s(name)
 WHERE NOT EXISTS (
     SELECT 1 FROM tag_groups g
      WHERE g.organization_id = o.id AND g.group_type = s.name
 );

-- A Custom group for every other name any row version carries.
INSERT INTO tag_groups (id, organization_id, name, group_type)
SELECT gen_random_uuid(), n.organization_id, n.tag_group, 'Custom'
  FROM (SELECT DISTINCT organization_id, tag_group
          FROM tags
         WHERE tag_group IS NOT NULL
           AND tag_group NOT IN ('Application', 'Status')) n
 WHERE NOT EXISTS (
     SELECT 1 FROM tag_groups g
      WHERE g.organization_id = n.organization_id AND g.name = n.tag_group
 );

-- Point every row version at its group.
DO $$
DECLARE
    last_id UUID := '00000000-0000-0000-0000-000000000000';
    batch UUID[];
BEGIN
    LOOP
        SELECT array_agg(id ORDER BY id)
          INTO batch
          FROM (SELECT id FROM tags
                 WHERE id > last_id AND tag_group IS NOT NULL AND tag_group_id IS NULL
                 ORDER BY id LIMIT 1000) t;

        EXIT WHEN batch IS NULL;

        UPDATE tags
           SET tag_group_id = g.id
          FROM tag_groups g
         WHERE tags.id = ANY(batch)
           AND g.organization_id = tags.organization_id
           AND g.name = tags.tag_group;

        last_id := batch[array_length(batch, 1)];
        COMMIT;
    END LOOP;
END $$;

-- Order each ordered group's live tags: the seeded starter order first, then by name. Batched by group:
-- a group holds tens of tags, so 100 groups stay well under the row limit per batch.
DO $$
DECLARE
    last_id UUID := '00000000-0000-0000-0000-000000000000';
    batch UUID[];
BEGIN
    LOOP
        SELECT array_agg(id ORDER BY id)
          INTO batch
          FROM (SELECT id FROM tag_groups
                 WHERE id > last_id
                 ORDER BY id LIMIT 100) g;

        EXIT WHEN batch IS NULL;

        UPDATE tags
           SET position = ranked.position
          FROM (
              SELECT t.id,
                     row_number() OVER (
                         PARTITION BY t.tag_group_id
                         ORDER BY s.rank NULLS LAST, lower(t.name), t.id
                     ) AS position
                FROM tags t
                JOIN tag_groups g ON g.id = t.tag_group_id
                LEFT JOIN (VALUES
                    ('Status', 'Planned', 1), ('Status', 'Active', 2),
                    ('Status', 'Decommissioning', 3), ('Status', 'Decommissioned', 4),
                    ('Environment', 'Production', 1), ('Environment', 'Staging', 2),
                    ('Environment', 'Development', 3), ('Environment', 'Lab', 4),
                    ('Criticality', 'Critical', 1), ('Criticality', 'High', 2),
                    ('Criticality', 'Medium', 3), ('Criticality', 'Low', 4),
                    ('Management', 'Managed', 1), ('Management', 'Unmanaged', 2),
                    ('Management', 'Third-party', 3)
                ) AS s(group_name, tag_name, rank)
                  ON s.group_name = g.name AND lower(s.tag_name) = lower(t.name)
               WHERE t.tag_group_id = ANY(batch)
                 AND g.ordered
                 AND t.valid_to IS NULL
                 AND t.snapshot_id IS NULL
          ) ranked
         WHERE tags.id = ranked.id
           AND tags.position IS DISTINCT FROM ranked.position;

        last_id := batch[array_length(batch, 1)];
        COMMIT;
    END LOOP;
END $$;

-- Validating an already valid constraint is a no-op, so a re-run is safe.
-- squawk-ignore prefer-robust-stmts
ALTER TABLE tags VALIDATE CONSTRAINT tags_tag_group_id_fkey;

-- Every group name and application flag is now held by tag_group_id. One statement, so a re-run
-- never finds one column gone and the other left.
-- squawk-ignore ban-drop-column
ALTER TABLE tags DROP COLUMN IF EXISTS tag_group, DROP COLUMN IF EXISTS is_application;
