-- Provenance for the discovered rows that had none: ports, IP addresses, interfaces, each
-- interface's learned MACs and VLAN membership, and LLDP/CDP neighbor candidates.
--
-- Each column records which method or integration found the row or value, as an `AttributeSource`
-- in the same externally tagged JSON as `mac_address_source`. The security page's list of what
-- Scanopy collects, and the daemon's check that each producer writes only what it declares, read
-- these.
--
-- Nothing is backfilled and nothing is dropped. No existing row can be attributed reliably: none
-- of them recorded how it was found, and several producers write the same row. `Unspecified`
-- states exactly that. A row heals the next time a current daemon reports it, because ingest
-- replaces an `Unspecified` source with an attributed one.
--
-- Additive and prod-safe: `ADD COLUMN` with a constant default is a catalog-only change on PG11+,
-- with no table rewrite. Snapshot and closed SCD2 rows take the default like live rows. New
-- snapshot copies carry the columns through each entity's `to_params`.

SET lock_timeout = '5s';
SET statement_timeout = '30s';

ALTER TABLE ports
    ADD COLUMN IF NOT EXISTS port_source JSONB NOT NULL DEFAULT '"Unspecified"'::jsonb;

ALTER TABLE ip_addresses
    ADD COLUMN IF NOT EXISTS ip_address_source JSONB NOT NULL DEFAULT '"Unspecified"'::jsonb;

ALTER TABLE interfaces
    ADD COLUMN IF NOT EXISTS interface_source JSONB NOT NULL DEFAULT '"Unspecified"'::jsonb,
    ADD COLUMN IF NOT EXISTS fdb_macs_source JSONB NOT NULL DEFAULT '"Unspecified"'::jsonb,
    ADD COLUMN IF NOT EXISTS vlan_membership_source JSONB NOT NULL DEFAULT '"Unspecified"'::jsonb;

ALTER TABLE interface_neighbor_candidates
    ADD COLUMN IF NOT EXISTS evidence_source JSONB NOT NULL DEFAULT '"Unspecified"'::jsonb;
