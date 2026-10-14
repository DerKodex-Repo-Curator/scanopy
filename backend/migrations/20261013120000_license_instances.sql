-- Server instances per online license key.
--
-- `server_instance` lives on self-hosted servers: one row per database, whose id is the random
-- instance id the server sends with each entitlement check-in. Every container and replica on the
-- database reads the same row, so restarts, upgrades, rolling deploys and replicas count as one
-- instance. The first container to start creates it, under an advisory lock.
--
-- `license_instances` lives on Scanopy Cloud: one row per instance id that has checked in with an
-- organization's online key. `instance_id` is NULL on the single row shared by servers too old to
-- send one. `last_seen_ip` is the source address the cloud observed on the latest check-in; the
-- server never sends an address. An owner deleting a row frees its slot.
--
-- Expand only: two new tables, nothing dropped or rewritten.

SET lock_timeout = '5s';
SET statement_timeout = '5s';

CREATE TABLE IF NOT EXISTS server_instance (
    id UUID PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS license_instances (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    instance_id UUID,
    server_version TEXT,
    last_seen_ip INET,
    first_seen_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The table is new and empty, so a plain build holds its lock for no time.
-- squawk-ignore require-concurrent-index-creation
CREATE INDEX IF NOT EXISTS idx_license_instances_organization
    ON license_instances (organization_id);

