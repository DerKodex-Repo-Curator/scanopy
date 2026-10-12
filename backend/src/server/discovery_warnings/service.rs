use std::collections::HashMap;

use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    daemon::discovery::types::warnings::DiscoveryWarning,
    server::{
        discovery_warnings::r#impl::base::{DiscoveryWarningRecord, RangeEvidence},
        shared::storage::{child::GenericChildStorage, filter::StorableFilter, traits::Storage},
    },
};

pub struct DiscoveryWarningService {
    storage: GenericChildStorage<DiscoveryWarningRecord>,
}

impl DiscoveryWarningService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            storage: GenericChildStorage::new(pool),
        }
    }

    /// Store a finished run's warnings, one row each.
    pub async fn record_for_run(
        &self,
        discovery_id: Uuid,
        site_id: Uuid,
        warnings: &[DiscoveryWarning],
    ) -> Result<()> {
        if warnings.is_empty() {
            return Ok(());
        }
        let records: Vec<DiscoveryWarningRecord> = warnings
            .iter()
            .map(|w| DiscoveryWarningRecord::from_warning(discovery_id, site_id, w))
            .collect();
        self.storage.inner().create_many(&records).await?;
        Ok(())
    }

    /// The warnings of each of these runs, each with the site its row is on, read only from the
    /// given sites. Ordered by code, then id, which keeps a run's warnings in the same order from
    /// one read to the next.
    pub async fn for_runs(
        &self,
        discovery_ids: &[Uuid],
        site_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<(Uuid, DiscoveryWarning)>>> {
        if discovery_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let filter = StorableFilter::<DiscoveryWarningRecord>::new_from_site_ids(site_ids)
            .uuids_column("discovery_id", discovery_ids);
        let records = self
            .storage
            .inner()
            .get_all_ordered(filter, "code ASC, id ASC")
            .await?;

        let mut by_run: HashMap<Uuid, Vec<(Uuid, DiscoveryWarning)>> = HashMap::new();
        for record in records {
            let (discovery_id, site_id) = (record.base.discovery_id, record.base.site_id);
            by_run
                .entry(discovery_id)
                .or_default()
                .push((site_id, record.into_warning()));
        }
        Ok(by_run)
    }

    /// For each of these assumed ranges, given with its site, the evidence it was drawn from. A row
    /// on another site than its range is never read.
    ///
    /// Every scan re-reports a standing range with no evidence, and only the scan that created it
    /// says which devices saw it, so this is the newest report that names any. A range with no
    /// such report (one inferred on the ingest path, which no scan created) is absent.
    pub async fn range_evidence(
        &self,
        subnets: &[(Uuid, Uuid)],
    ) -> Result<HashMap<Uuid, RangeEvidence>> {
        if subnets.is_empty() {
            return Ok(HashMap::new());
        }
        let site_of: HashMap<Uuid, Uuid> = subnets.iter().copied().collect();
        let subnet_ids: Vec<Uuid> = site_of.keys().copied().collect();
        let site_ids: Vec<Uuid> = site_of.values().copied().collect();
        // `ProvisionalSubnetInferred` is the one warning that names a subnet.
        let filter = StorableFilter::<DiscoveryWarningRecord>::new_from_site_ids(&site_ids)
            .uuids_column("subnet_id", &subnet_ids);
        let records = self
            .storage
            .inner()
            .get_all_ordered(filter, "created_at DESC, id ASC")
            .await?;

        let mut evidence = HashMap::new();
        for record in records {
            let (Some(subnet_id), discovery_id) = (record.base.subnet_id, record.base.discovery_id)
            else {
                continue;
            };
            if evidence.contains_key(&subnet_id)
                || site_of.get(&subnet_id) != Some(&record.base.site_id)
            {
                continue;
            }
            if let DiscoveryWarning::ProvisionalSubnetInferred(p) = record.into_warning()
                && !p.seen_by_host_ids.is_empty()
            {
                evidence.insert(
                    subnet_id,
                    RangeEvidence {
                        discovery_id,
                        seen_by_host_ids: p.seen_by_host_ids,
                        addresses: p.addresses,
                        sys_names: p.sys_names,
                        widened_by_vlan: p.widened_by_vlan,
                    },
                );
            }
        }
        Ok(evidence)
    }
}
