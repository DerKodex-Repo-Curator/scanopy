-- The FirstSnmpCredentialCreated milestone became FirstInterfaceCredentialCreated and now counts
-- a credential for any integration that reports interfaces: SNMP, gNMI, UniFi, Instant On and
-- SSH. Rows that hold the old name keep it; the server reads it through a serde alias.
--
-- Backfill the milestone for orgs that created an interface credential of one of the newly
-- counted types before the rename, so their interface prompts clear without a second credential.
-- Nothing is dropped.
SET lock_timeout = '5s';

UPDATE organizations o
SET onboarding = o.onboarding || '["FirstInterfaceCredentialCreated"]'::JSONB
WHERE NOT (o.onboarding @> '["FirstInterfaceCredentialCreated"]'::JSONB)
  AND NOT (o.onboarding @> '["FirstSnmpCredentialCreated"]'::JSONB)
  AND EXISTS (
    SELECT 1
    FROM credentials c
    WHERE c.organization_id = o.id
      AND c.credential_type->>'type' IN (
        'SnmpV1', 'SnmpV2c', 'SnmpV3',
        'Gnmi',
        'UnifiApiKey', 'UnifiLocalAdmin',
        'InstantOnAccount',
        'SshPassword', 'SshKey'
      )
  );
