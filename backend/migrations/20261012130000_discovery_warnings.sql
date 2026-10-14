-- no-transaction
--
-- Discovery warnings become rows of their own, in `discovery_warnings`, one per warning.
--
-- A historical run kept its warnings in `discovery.run_type` JSONB (`results.warnings[]`), where
-- the entity ids they name were unchecked text. Each id now has a foreign-key column:
--
--   credential_id     credentials, ON DELETE SET NULL   (13 credential warning codes)
--   host_id           hosts,       ON DELETE SET NULL   (LldpNeighbour*, LldpPort*)
--   remote_host_id    hosts,       ON DELETE SET NULL   (LldpPort*)
--   subnet_id         subnets,     ON DELETE SET NULL   (ProvisionalSubnetInferred)
--
-- A warning outlives what it names: the scan report renders a deleted host as gone, so deleting
-- one sets the id to NULL rather than the warning. Deleting the run deletes its warnings, and
-- `site_id` (the run's site) scopes every read.
--
-- Where each stored value goes:
--
--   warnings[i]                        one row, discovery_id = the run, site_id = its site
--   warnings[i].code                   code; a code this release doesn't know is 'Unknown'
--   warnings[i].<the four ids>         their columns, each only when the row it names still
--                                      exists, otherwise NULL, so no foreign key dangles
--   the rest of warnings[i]            details, minus the four id keys. `seen_by_host_ids`
--                                      stays here: a list of ids cannot be a foreign key.
--   a bare string warning              details = {"code": "Unknown", "detail": <the string>},
--                                      the same value the server reads it as
--
-- Nothing is dropped. `results.warnings` stays in the stored JSON, unread, until the contract
-- migration in v0.18.2 strips it. That migration first re-runs this backfill, so runs a v0.18.0
-- container wrote during the rolling deploy reach the table too (docs/db-expand-contract-ledger.md).
--
-- Batched at about 1000 warnings with a COMMIT per batch (hence `-- no-transaction`). A run's
-- warnings go in together, and a run that already has rows is skipped, so re-running from any
-- partial state finishes the job.

SET lock_timeout = '5s';
SET statement_timeout = '0';

CREATE TABLE IF NOT EXISTS discovery_warnings (
    id UUID PRIMARY KEY,
    discovery_id UUID NOT NULL REFERENCES discovery(id) ON DELETE CASCADE,
    site_id UUID NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    credential_id UUID REFERENCES credentials(id) ON DELETE SET NULL,
    host_id UUID REFERENCES hosts(id) ON DELETE SET NULL,
    remote_host_id UUID REFERENCES hosts(id) ON DELETE SET NULL,
    subnet_id UUID REFERENCES subnets(id) ON DELETE SET NULL,
    details JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The table is new and empty, so these build instantly and block nothing.
-- squawk-ignore require-concurrent-index-creation
CREATE INDEX IF NOT EXISTS idx_discovery_warnings_discovery ON discovery_warnings (discovery_id);
-- squawk-ignore require-concurrent-index-creation
CREATE INDEX IF NOT EXISTS idx_discovery_warnings_site ON discovery_warnings (site_id);
-- squawk-ignore require-concurrent-index-creation
CREATE INDEX IF NOT EXISTS idx_discovery_warnings_credential ON discovery_warnings (credential_id)
    WHERE credential_id IS NOT NULL;
-- squawk-ignore require-concurrent-index-creation
CREATE INDEX IF NOT EXISTS idx_discovery_warnings_host ON discovery_warnings (host_id)
    WHERE host_id IS NOT NULL;
-- squawk-ignore require-concurrent-index-creation
CREATE INDEX IF NOT EXISTS idx_discovery_warnings_remote_host ON discovery_warnings (remote_host_id)
    WHERE remote_host_id IS NOT NULL;
-- squawk-ignore require-concurrent-index-creation
CREATE INDEX IF NOT EXISTS idx_discovery_warnings_subnet ON discovery_warnings (subnet_id)
    WHERE subnet_id IS NOT NULL;

DO $$
DECLARE
    -- The codes `DiscoveryWarningCode` has at this release. Anything else is stored as 'Unknown',
    -- which is what the server reads it as.
    known_codes TEXT[] := ARRAY[
        'InterfaceSetCutShort', 'InterfaceDetailsCutShort', 'SnmpWalkEntryCap',
        'SnmpWalkUnsupported', 'SnmpWalkDesynchronised', 'SnmpWalkPartialDiscarded',
        'SnmpWalkPartialRecorded', 'SnmpWalkBridgeMibAbsent', 'SnmpWalkNoAnswer',
        'ClaimedCountReadCutShort', 'ClaimedCountUnderRead', 'ClaimedCapabilityReadCutShort',
        'ClaimedCapabilityEmpty', 'LldpLocalPortDropped', 'LldpLocalPortDroppedReadCutShort',
        'LldpLocalPortMisplaced', 'MalformedNeighboursWalkCutShort', 'MalformedNeighboursGhostRows',
        'MalformedNeighboursIncompleteRecords', 'MalformedNeighboursUnexpectedType',
        'MalformedNeighboursUnreadableIndex', 'SnmpCollectedNothing', 'VlanRecordingFailed',
        'EqualReachIntegrationsMerged', 'CredentialTargetNotScanned',
        'CredentialTargetNotResponding', 'CredentialDaemonOsMismatch', 'CredentialDaemonTooOld',
        'CredentialGateClosed', 'CredentialRejected', 'CredentialMalformed', 'CredentialTlsFailed',
        'CredentialNotThisService', 'CredentialCollectionFailed', 'CredentialCollectionTimedOut',
        'CredentialUnreachable', 'CredentialTimedOut', 'ConnectionsWithoutProtocolResponse',
        'ScanTimeLimitWithEstimate', 'ScanTimeLimit', 'DcpSweepTimedOut', 'IcmpSweepTimedOut',
        'ReverseDnsTimedOut', 'LldpNeighbourNotFound', 'LldpNeighbourAmbiguous',
        'LldpPortNoStrategy', 'LldpPortNotFound', 'LldpPortAmbiguous', 'ProvisionalSubnetInferred',
        'NeighbourResolutionIncomplete', 'FdbResolutionIncomplete', 'OutdatedDaemonFormat',
        'WarningsTruncated', 'Unknown'
    ];
    uuid_pattern TEXT := '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$';
    last_id UUID := '00000000-0000-0000-0000-000000000000';
    batch UUID[];
BEGIN
    PERFORM set_config('lock_timeout', '5s', false);
    PERFORM set_config('statement_timeout', '0', false);

    LOOP
        -- The next runs, in id order, that hold warnings and have no rows yet: as many as start
        -- before 1000 warnings have accumulated, so a batch holds about 1000 and a run larger
        -- than that goes alone.
        SELECT array_agg(id ORDER BY id)
        INTO batch
        FROM (
            SELECT id, SUM(n) OVER (ORDER BY id) - n AS before_this
            FROM (
                SELECT d.id, jsonb_array_length(d.run_type->'results'->'warnings') AS n
                FROM discovery d
                WHERE d.id > last_id
                  AND d.run_type->>'type' = 'Historical'
                  AND jsonb_typeof(d.run_type->'results'->'warnings') = 'array'
                  AND jsonb_array_length(d.run_type->'results'->'warnings') > 0
                  AND NOT EXISTS (
                      SELECT 1 FROM discovery_warnings x WHERE x.discovery_id = d.id
                  )
                ORDER BY d.id
                LIMIT 1000
            ) candidates
        ) running
        WHERE before_this < 1000;

        EXIT WHEN batch IS NULL;

        INSERT INTO discovery_warnings (
            id, discovery_id, site_id, code,
            credential_id, host_id, remote_host_id, subnet_id,
            details, created_at
        )
        SELECT
            gen_random_uuid(),
            d.id,
            d.site_id,
            CASE WHEN w.elem->>'code' = ANY(known_codes) THEN w.elem->>'code' ELSE 'Unknown' END,
            CASE WHEN w.elem->>'credential_id' ~ uuid_pattern THEN
                (SELECT c.id FROM credentials c WHERE c.id = (w.elem->>'credential_id')::uuid)
            END,
            CASE WHEN w.elem->>'host_id' ~ uuid_pattern THEN
                (SELECT h.id FROM hosts h WHERE h.id = (w.elem->>'host_id')::uuid)
            END,
            CASE WHEN w.elem->>'remote_host_id' ~ uuid_pattern THEN
                (SELECT h.id FROM hosts h WHERE h.id = (w.elem->>'remote_host_id')::uuid)
            END,
            CASE WHEN w.elem->>'subnet_id' ~ uuid_pattern THEN
                (SELECT s.id FROM subnets s WHERE s.id = (w.elem->>'subnet_id')::uuid)
            END,
            w.elem - 'credential_id' - 'host_id' - 'remote_host_id' - 'subnet_id',
            d.created_at
        FROM discovery d
        CROSS JOIN LATERAL (
            SELECT CASE
                WHEN jsonb_typeof(e) = 'object' THEN e
                ELSE jsonb_build_object('code', 'Unknown', 'detail', e #>> '{}')
            END AS elem
            FROM jsonb_array_elements(d.run_type->'results'->'warnings') e
        ) w
        WHERE d.id = ANY(batch);

        last_id := batch[array_length(batch, 1)];
        COMMIT;
    END LOOP;
END $$;
