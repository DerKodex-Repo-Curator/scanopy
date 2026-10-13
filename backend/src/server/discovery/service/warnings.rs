//! A historical run's warnings: written as `discovery_warnings` rows when the run is recorded, and
//! read back onto `run_type.results.warnings` wherever the run is read through this service.
use super::*;
use crate::daemon::discovery::types::warnings::DiscoveryWarning;
use crate::server::credentials::service::FoundIntegration;

impl DiscoveryService {
    /// Store the warnings a historical run carries in memory. Called once, right after the run's
    /// row is created, since the row's storage never holds them.
    pub async fn record_warnings(&self, discovery: &Discovery) -> Result<()> {
        let RunType::Historical { results } = &discovery.base.run_type else {
            return Ok(());
        };
        self.warning_service
            .record_for_run(discovery.id, discovery.base.site_id, &results.warnings)
            .await
    }

    /// What each site's latest completed full scan found on hosts with no credential, as
    /// recorded then. A host deleted since is left out. The caller re-checks coverage, since a
    /// credential saved after the scan covers the host now.
    ///
    /// A rescan is skipped: it touches one host, so it would hide every other host the last full
    /// scan reported.
    pub async fn latest_integrations_without_credentials(
        &self,
        site_ids: &[Uuid],
    ) -> Result<Vec<FoundIntegration>> {
        let mut found = Vec::new();
        for site_id in site_ids {
            let filter = StorableFilter::<Discovery>::new_from_site_ids(&[*site_id])
                .historical_discovery()
                .exclude_rescans()
                .completed_runs()
                .limit(1);
            let mut latest = self
                .discovery_storage
                .get_all_ordered(filter, "updated_at DESC")
                .await?;
            self.attach_warnings(&mut latest).await?;
            for discovery in &latest {
                let RunType::Historical { results } = &discovery.base.run_type else {
                    continue;
                };
                found.extend(results.warnings.iter().filter_map(|w| match w {
                    DiscoveryWarning::IntegrationWithoutCredential {
                        host_id: Some(host_id),
                        integration,
                    } => Some(FoundIntegration {
                        host_id: *host_id,
                        site_id: discovery.base.site_id,
                        integration: *integration,
                    }),
                    _ => None,
                }));
            }
        }
        Ok(found)
    }

    /// Fill each historical run's warnings from the table.
    ///
    /// Rows are read only from the runs' own sites, so a warning is reachable only through a run
    /// the caller was already allowed to read, the same way its tags are.
    pub(crate) async fn attach_warnings(&self, discoveries: &mut [Discovery]) -> Result<()> {
        let historical: Vec<&Discovery> = discoveries
            .iter()
            .filter(|d| matches!(d.base.run_type, RunType::Historical { .. }))
            .collect();
        if historical.is_empty() {
            return Ok(());
        }
        let ids: Vec<Uuid> = historical.iter().map(|d| d.id).collect();
        let site_ids: Vec<Uuid> = historical
            .iter()
            .map(|d| d.base.site_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let mut by_run = self.warning_service.for_runs(&ids, &site_ids).await?;
        for discovery in discoveries {
            if let RunType::Historical { results } = &mut discovery.base.run_type {
                // A row on another site than its run is never attached, even when it shares the
                // run's id.
                results.warnings = by_run
                    .remove(&discovery.id)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|(site_id, _)| *site_id == discovery.base.site_id)
                    .map(|(_, warning)| warning)
                    .collect();
            }
        }
        Ok(())
    }
}
