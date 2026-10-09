-- no-transaction
--
-- Status becomes a system tag group next to Application, and both are stored by name in
-- `tags.tag_group`. Application used to live in `tags.is_application`, which
-- 20261009120101_drop_tags_is_application.sql drops once this has copied it. Where each value
-- goes:
--
--   is_application = true                  -> tag_group = 'Application'
--   a named group 'application', any case  -> renamed to '<name> (custom)'. No organization
--                                             can name a group after a system group any more,
--                                             and folding it in would put its entities into the
--                                             application view.
--   a named group 'status', other casing   -> tag_group = 'Status', the system group. An entity
--                                             left holding two Status tags keeps the one it got
--                                             last; the other assignment is closed, not deleted,
--                                             so it stays in the entity's history.
--   tag_group = 'Status'                   -> unchanged; it now reads as the system group.
--
-- `tags` and `entity_tags` are SCD2: the updates to `tags` cover live, closed and snapshot rows
-- and touch no validity or lineage column. The dedupe closes live assignments only; snapshot
-- rows are a frozen record and stay as they were.
--
-- The application backfill is batched at 1000 rows with a COMMIT per batch (hence
-- `-- no-transaction`), keyset-paginated by `id`. Every step is idempotent, so a run that fails
-- part-way can be re-run.

SET lock_timeout = '5s';
SET statement_timeout = '0';

UPDATE tags
   SET tag_group = tag_group || ' (custom)'
 WHERE NOT is_application
   AND lower(tag_group) = 'application';

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

UPDATE tags
   SET tag_group = 'Status'
 WHERE lower(tag_group) = 'status'
   AND tag_group <> 'Status';

WITH held AS (
    SELECT et.id,
           row_number() OVER (
               PARTITION BY et.entity_id, et.entity_type
               ORDER BY et.valid_from DESC, et.id
           ) AS position
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
   AND held.position > 1;
