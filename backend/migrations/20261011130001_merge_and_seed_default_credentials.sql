-- no-transaction
--
-- Seeded default credentials. Every organization holds exactly one of each:
--
--   Docker socket   {"type": "DockerSocket"}                                          (no path)
--   Podman socket   {"type": "PodmanSocket"}                                          (no path)
--   SNMP (public)   {"type": "SnmpV2c", "community": {"mode": "Inline", "value": "public"}}
--
-- A credential is a default when its settings are identical to one of these
-- (`CredentialType::is_default`): a socket with no path or a blank one, or SNMPv2c with the inline
-- community `public`. The names and descriptions match `CredentialTypeDiscriminants::seeded_default`.
--
-- 1. Merge duplicates. Daemon setup used to create one socket credential per daemon, so an
--    organization can hold several identical defaults of a type. The oldest (by created_at, then
--    id) survives, and every reference to the others moves to it before they are deleted:
--
--      site_credentials (site_id, credential_id)      re-pointed; a site already assigned the
--                                                     survivor keeps its one row
--      host_credentials (host_id, credential_id,      re-pointed; where the host already has the
--        ip_address_ids)                              survivor, ip_address_ids becomes the union
--                                                     (NULL, the whole host, wins)
--      entity_tags (entity_type '"Credential"')       live tags the survivor lacks move to it,
--                                                     closed and snapshot rows move as they are;
--                                                     a live tag it already has is a duplicate
--      discovery.integration_targets                  credential ids rewritten; a target that
--                                                     becomes identical to another in the same
--                                                     list is dropped as a duplicate
--      discovery.discovery_type                       credential ids rewritten (legacy SNMP
--                                                     mapping: default_credential_id,
--                                                     ip_overrides[].credential_id)
--      discovery.run_type                             credential ids rewritten (Historical
--                                                     results: credential_results[], warnings[],
--                                                     and the discovery_type copy)
--      credentials.description                        the survivor gets the seeded description
--                                                     when it has none
--
--    No other column holds a credential id: site_credentials and host_credentials are the only
--    foreign keys to credentials, and these three discovery columns are the only JSON that names
--    one. Nothing is lost. The duplicates are identical to the survivor by definition, and every
--    assignment, tag and reference is carried over. The survivor keeps its own name.
--
-- 2. Seed each default an organization lacks.
--
-- 3. Assign the SNMP default to every existing site, so sites keep probing `public` the way the
--    daemon used to do implicitly. New sites get it when they are created.
--
-- Batched at 1000 rows with a COMMIT per batch (hence `-- no-transaction`). It all runs in one DO
-- block, so the scratch tables live on one connection. Re-running from any partial state finishes
-- the job: the mapping is recomputed from live rows, every insert skips what exists, and the
-- duplicates are deleted last.

SET lock_timeout = '5s';
SET statement_timeout = '0';

DO $$
DECLARE
    last_id UUID;
    batch UUID[];
    org_row RECORD;
    pair RECORD;
BEGIN
    PERFORM set_config('lock_timeout', '5s', false);
    PERFORM set_config('statement_timeout', '0', false);

    DROP TABLE IF EXISTS pg_temp.seeded_default_types;
    CREATE TEMP TABLE seeded_default_types (
        type TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT NOT NULL,
        credential_type JSONB NOT NULL
    );
    INSERT INTO seeded_default_types VALUES
        ('DockerSocket', 'Docker socket',
         'Added to every new daemon. Reads the local Docker socket at its default path.',
         '{"type": "DockerSocket"}'),
        ('PodmanSocket', 'Podman socket',
         'Added to every new daemon. Reads the local Podman socket at its default path.',
         '{"type": "PodmanSocket"}'),
        ('SnmpV2c', 'SNMP (public)',
         'Assigned to every new site. Tries the SNMPv2c community "public" on every host.',
         '{"type": "SnmpV2c", "community": {"mode": "Inline", "value": "public"}}');

    -- Every credential identical to a seeded default.
    DROP TABLE IF EXISTS pg_temp.default_credentials;
    CREATE TEMP TABLE default_credentials AS
    SELECT c.id, c.organization_id, t.type, c.created_at
      FROM credentials c
      JOIN seeded_default_types t ON t.type = c.credential_type->>'type'
     WHERE CASE t.type
               WHEN 'SnmpV2c' THEN c.credential_type->'community'->>'mode' = 'Inline'
                               AND c.credential_type->'community'->>'value' = 'public'
               ELSE coalesce(c.credential_type->>'socket_path', '') ~ '^\s*$'
           END;

    -- Each duplicate and the oldest default of its organization and type.
    DROP TABLE IF EXISTS pg_temp.default_credential_merge;
    CREATE TEMP TABLE default_credential_merge AS
    SELECT id AS duplicate_id, survivor_id, organization_id
      FROM (SELECT id, organization_id,
                   first_value(id) OVER (PARTITION BY organization_id, type
                                         ORDER BY created_at, id) AS survivor_id
              FROM default_credentials) ranked
     WHERE id <> survivor_id;
    CREATE INDEX ON default_credential_merge (duplicate_id);

    -- 1a. Site assignments.
    INSERT INTO site_credentials (site_id, credential_id)
    SELECT sc.site_id, m.survivor_id
      FROM site_credentials sc
      JOIN default_credential_merge m ON m.duplicate_id = sc.credential_id
    ON CONFLICT DO NOTHING;
    COMMIT;

    -- 1b. Host assignments. Several duplicates can sit on one host, so they are folded together
    --     first, then folded into any row the survivor already has.
    INSERT INTO host_credentials (host_id, credential_id, ip_address_ids)
    SELECT host_id, survivor_id,
           CASE WHEN bool_or(whole_host) THEN NULL
                ELSE coalesce(array_agg(DISTINCT ip) FILTER (WHERE ip IS NOT NULL), '{}')
           END
      FROM (SELECT hc.host_id, m.survivor_id, hc.ip_address_ids IS NULL AS whole_host, ip
              FROM host_credentials hc
              JOIN default_credential_merge m ON m.duplicate_id = hc.credential_id
              LEFT JOIN LATERAL unnest(hc.ip_address_ids) AS ip ON true) moved
     GROUP BY host_id, survivor_id
    ON CONFLICT (host_id, credential_id) DO UPDATE
       SET ip_address_ids = CASE
               WHEN host_credentials.ip_address_ids IS NULL OR EXCLUDED.ip_address_ids IS NULL
                   THEN NULL
               ELSE ARRAY(SELECT DISTINCT unnest(host_credentials.ip_address_ids
                                                 || EXCLUDED.ip_address_ids))
           END;
    COMMIT;

    -- 1c. Tags. One live row per tag moves to the survivor unless it already has that tag; closed
    --     and snapshot rows move as they are. What is left on a duplicate is a second copy of a
    --     live tag the survivor holds, and goes with the duplicate.
    UPDATE entity_tags et
       SET entity_id = pick.survivor_id
      FROM (SELECT DISTINCT ON (m.survivor_id, t.tag_id) t.id, m.survivor_id
              FROM entity_tags t
              JOIN default_credential_merge m ON m.duplicate_id = t.entity_id
             WHERE t.entity_type = '"Credential"' AND t.valid_to IS NULL
             ORDER BY m.survivor_id, t.tag_id, t.created_at, t.id) pick
     WHERE et.id = pick.id
       AND NOT EXISTS (SELECT 1 FROM entity_tags s
                        WHERE s.entity_id = pick.survivor_id
                          AND s.entity_type = '"Credential"'
                          AND s.tag_id = et.tag_id
                          AND s.valid_to IS NULL);
    UPDATE entity_tags et
       SET entity_id = m.survivor_id
      FROM default_credential_merge m
     WHERE et.entity_id = m.duplicate_id
       AND et.entity_type = '"Credential"'
       AND et.valid_to IS NOT NULL;
    DELETE FROM entity_tags et
     USING default_credential_merge m
     WHERE et.entity_id = m.duplicate_id AND et.entity_type = '"Credential"';
    COMMIT;

    -- 1d. Credential ids inside the discovery JSON, per organization, 1000 discoveries at a time.
    --     A UUID's text appears nowhere but as that id, so a text replacement reaches every place
    --     one is stored, however deeply nested.
    FOR org_row IN SELECT DISTINCT organization_id FROM default_credential_merge LOOP
        last_id := '00000000-0000-0000-0000-000000000000';
        LOOP
            SELECT array_agg(id ORDER BY id)
              INTO batch
              FROM (SELECT d.id FROM discovery d
                      JOIN sites s ON s.id = d.site_id
                     WHERE s.organization_id = org_row.organization_id AND d.id > last_id
                     ORDER BY d.id LIMIT 1000) t;
            EXIT WHEN batch IS NULL;

            FOR pair IN SELECT duplicate_id, survivor_id FROM default_credential_merge
                      WHERE organization_id = org_row.organization_id LOOP
                UPDATE discovery d
                   SET integration_targets = replace(d.integration_targets::text,
                                                     pair.duplicate_id::text,
                                                     pair.survivor_id::text)::jsonb,
                       discovery_type = replace(d.discovery_type::text,
                                                pair.duplicate_id::text,
                                                pair.survivor_id::text)::jsonb,
                       run_type = replace(d.run_type::text,
                                          pair.duplicate_id::text,
                                          pair.survivor_id::text)::jsonb
                 WHERE d.id = ANY(batch)
                   AND (d.integration_targets::text LIKE '%' || pair.duplicate_id::text || '%'
                        OR d.discovery_type::text LIKE '%' || pair.duplicate_id::text || '%'
                        OR d.run_type::text LIKE '%' || pair.duplicate_id::text || '%');
            END LOOP;

            -- Two targets that now name the same credential the same way are one target.
            UPDATE discovery d
               SET integration_targets = deduped.targets
              FROM (SELECT d2.id,
                           coalesce((SELECT jsonb_agg(e ORDER BY ord)
                                       FROM (SELECT DISTINCT ON (e) e, ord
                                               FROM jsonb_array_elements(d2.integration_targets)
                                                    WITH ORDINALITY AS a(e, ord)
                                              ORDER BY e, ord) firsts),
                                    '[]'::jsonb) AS targets
                      FROM discovery d2
                     WHERE d2.id = ANY(batch)) deduped
             WHERE d.id = deduped.id
               AND d.integration_targets IS DISTINCT FROM deduped.targets;

            last_id := batch[array_length(batch, 1)];
            COMMIT;
        END LOOP;
    END LOOP;

    -- Each organization's default gets the seeded description when it has none: the survivor of
    -- a merge, or the only copy when there was nothing to merge.
    UPDATE credentials c
       SET description = t.description
      FROM default_credentials d
      JOIN seeded_default_types t ON t.type = d.type
     WHERE c.id = d.id
       AND d.id NOT IN (SELECT duplicate_id FROM default_credential_merge)
       AND coalesce(btrim(c.description), '') = '';
    COMMIT;

    -- 1e. The duplicates, now unreferenced. Their junction rows go with them (ON DELETE CASCADE),
    --     and each was copied to the survivor above.
    LOOP
        SELECT array_agg(duplicate_id) INTO batch
          FROM (SELECT m.duplicate_id FROM default_credential_merge m
                  JOIN credentials c ON c.id = m.duplicate_id
                 LIMIT 1000) t;
        EXIT WHEN batch IS NULL;
        DELETE FROM credentials WHERE id = ANY(batch);
        COMMIT;
    END LOOP;

    -- 2. Seed each default an organization lacks, 1000 organizations at a time.
    last_id := '00000000-0000-0000-0000-000000000000';
    LOOP
        SELECT array_agg(id ORDER BY id)
          INTO batch
          FROM (SELECT id FROM organizations WHERE id > last_id ORDER BY id LIMIT 1000) t;
        EXIT WHEN batch IS NULL;

        INSERT INTO credentials (id, organization_id, name, description, credential_type,
                                 daemon_os, created_at, updated_at)
        SELECT gen_random_uuid(), o.id, t.name, t.description, t.credential_type, NULL,
               now(), now()
          FROM organizations o
          CROSS JOIN seeded_default_types t
         WHERE o.id = ANY(batch)
           AND NOT EXISTS (
               SELECT 1 FROM credentials c
                WHERE c.organization_id = o.id
                  AND c.credential_type->>'type' = t.type
                  AND CASE t.type
                          WHEN 'SnmpV2c' THEN c.credential_type->'community'->>'mode' = 'Inline'
                                          AND c.credential_type->'community'->>'value' = 'public'
                          ELSE coalesce(c.credential_type->>'socket_path', '') ~ '^\s*$'
                      END);

        last_id := batch[array_length(batch, 1)];
        COMMIT;
    END LOOP;

    -- 3. Every site is assigned its organization's SNMP default, 1000 sites at a time. Each
    --    organization now has exactly one.
    last_id := '00000000-0000-0000-0000-000000000000';
    LOOP
        SELECT array_agg(id ORDER BY id)
          INTO batch
          FROM (SELECT id FROM sites WHERE id > last_id ORDER BY id LIMIT 1000) t;
        EXIT WHEN batch IS NULL;

        INSERT INTO site_credentials (site_id, credential_id)
        SELECT s.id, c.id
          FROM sites s
          JOIN credentials c ON c.organization_id = s.organization_id
         WHERE s.id = ANY(batch)
           AND c.credential_type->>'type' = 'SnmpV2c'
           AND c.credential_type->'community'->>'mode' = 'Inline'
           AND c.credential_type->'community'->>'value' = 'public'
        ON CONFLICT DO NOTHING;

        last_id := batch[array_length(batch, 1)];
        COMMIT;
    END LOOP;

    DROP TABLE IF EXISTS pg_temp.default_credential_merge;
    DROP TABLE IF EXISTS pg_temp.default_credentials;
    DROP TABLE IF EXISTS pg_temp.seeded_default_types;
END $$;
