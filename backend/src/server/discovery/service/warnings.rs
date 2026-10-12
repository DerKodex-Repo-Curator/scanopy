//! A historical run's warnings: written as `discovery_warnings` rows when the run is recorded, and
//! read back onto `run_type.results.warnings` wherever the run is read through this service.
use super::*;

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
