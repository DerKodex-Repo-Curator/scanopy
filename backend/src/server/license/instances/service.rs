use std::net::IpAddr;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use semver::Version;
use uuid::Uuid;

use super::{LicenseInstance, LicenseInstanceBase, ServerInstance, active_count, holds_slot};
use crate::server::license::online::InstanceUsage;
use crate::server::shared::storage::filter::StorableFilter;
use crate::server::shared::storage::generic::GenericPostgresStorage;
use crate::server::shared::storage::lock::{DEFAULT_LOCK_TIMEOUT, LockKey};
use crate::server::shared::storage::traits::{Storable, Storage};

/// One entitlement check-in, as the cloud saw it.
#[derive(Debug, Clone)]
pub struct CheckIn {
    pub organization_id: Uuid,
    pub instance_id: Option<Uuid>,
    pub server_version: Option<Version>,
    pub ip: Option<IpAddr>,
    pub at: DateTime<Utc>,
}

/// What a recorded check-in found: the organization's instance count, and
/// whether the instance that checked in holds a slot under the limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedCheckIn {
    pub usage: InstanceUsage,
    pub holds_slot: bool,
}

pub struct LicenseInstanceService {
    instances: Arc<GenericPostgresStorage<LicenseInstance>>,
    server_instance: Arc<GenericPostgresStorage<ServerInstance>>,
}

impl LicenseInstanceService {
    pub fn new(
        instances: Arc<GenericPostgresStorage<LicenseInstance>>,
        server_instance: Arc<GenericPostgresStorage<ServerInstance>>,
    ) -> Self {
        Self {
            instances,
            server_instance,
        }
    }

    /// This database's instance id, created by the first caller. Serialized
    /// by an advisory lock, so replicas starting together all read one id.
    pub async fn server_instance_id(&self) -> Result<Uuid> {
        let guard = self
            .server_instance
            .session_lock(LockKey::ServerInstance, DEFAULT_LOCK_TIMEOUT)
            .await?;
        let result = self.server_instance_id_locked().await;
        guard.release().await?;
        result
    }

    async fn server_instance_id_locked(&self) -> Result<Uuid> {
        let existing = self
            .server_instance
            .get_all(StorableFilter::new_for_server_wide_job())
            .await?;
        if let Some(instance) = existing.into_iter().next() {
            return Ok(instance.id);
        }
        let created = self
            .server_instance
            .create(&ServerInstance::new(()))
            .await?;
        tracing::info!(instance_id = %created.id, "Created this server's instance id");
        Ok(created.id)
    }

    /// Record a check-in against the instance it came from, creating the row
    /// on its first check-in, and return the organization's instance count
    /// against `limit` and whether this instance holds a slot. Check-ins with
    /// no instance id share one row, and the first check-in that sends an id
    /// takes that row over, so upgrading an old server keeps its slot.
    pub async fn record_check_in(
        &self,
        check_in: CheckIn,
        limit: Option<u32>,
    ) -> Result<RecordedCheckIn> {
        let guard = self
            .instances
            .session_lock(
                LockKey::LicenseInstances {
                    organization_id: check_in.organization_id,
                },
                DEFAULT_LOCK_TIMEOUT,
            )
            .await?;
        let result = self.record_check_in_locked(check_in, limit).await;
        guard.release().await?;
        result
    }

    async fn record_check_in_locked(
        &self,
        check_in: CheckIn,
        limit: Option<u32>,
    ) -> Result<RecordedCheckIn> {
        let mut instances = self.list(check_in.organization_id).await?;
        let server_version = check_in.server_version.map(|v| v.to_string());

        let matching = instances
            .iter()
            .position(|i| i.base.instance_id == check_in.instance_id)
            .or_else(|| {
                check_in
                    .instance_id
                    .and_then(|_| instances.iter().position(|i| i.base.instance_id.is_none()))
            });
        let id = match matching.map(|index| &mut instances[index]) {
            Some(instance) => {
                instance.base.instance_id = check_in.instance_id;
                instance.base.last_seen_at = check_in.at;
                instance.base.last_seen_ip = check_in.ip.or(instance.base.last_seen_ip);
                instance.base.server_version =
                    server_version.or(instance.base.server_version.take());
                instance.updated_at = check_in.at;
                self.instances.update(instance).await?;
                instance.id
            }
            None => {
                let created = self
                    .instances
                    .create(&LicenseInstance::new(LicenseInstanceBase {
                        organization_id: check_in.organization_id,
                        instance_id: check_in.instance_id,
                        server_version,
                        last_seen_ip: check_in.ip,
                        first_seen_at: check_in.at,
                        last_seen_at: check_in.at,
                    }))
                    .await?;
                let id = created.id;
                instances.push(created);
                id
            }
        };

        Ok(RecordedCheckIn {
            usage: InstanceUsage {
                active: active_count(&instances, check_in.at),
                limit,
            },
            holds_slot: holds_slot(&instances, id, limit, check_in.at),
        })
    }

    /// Every instance that has checked in with the organization's key, most
    /// recent first.
    pub async fn list(&self, organization_id: Uuid) -> Result<Vec<LicenseInstance>> {
        let mut instances = self
            .instances
            .get_all(StorableFilter::new_from_org_id(&organization_id))
            .await?;
        instances.sort_by_key(|i| std::cmp::Reverse(i.base.last_seen_at));
        Ok(instances)
    }

    /// Delete one of the organization's instances, freeing its slot. Returns
    /// `false` when the organization has no instance with that id. A server
    /// that is still running is recorded again at its next check-in.
    pub async fn delete(&self, organization_id: Uuid, id: Uuid) -> Result<bool> {
        let deleted = self
            .instances
            .delete_by_filter(StorableFilter::new_from_org_id(&organization_id).entity_id(&id))
            .await?;
        Ok(deleted > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::shared::storage::factory::StorageFactory;
    use crate::tests::{organization, test_storage};

    fn service(storage: &StorageFactory) -> LicenseInstanceService {
        LicenseInstanceService::new(
            storage.license_instances.clone(),
            storage.server_instance.clone(),
        )
    }

    async fn org_id(storage: &StorageFactory) -> Uuid {
        storage
            .organizations
            .create(&organization())
            .await
            .unwrap()
            .id
    }

    fn check_in(organization_id: Uuid, instance_id: Option<Uuid>, at: DateTime<Utc>) -> CheckIn {
        CheckIn {
            organization_id,
            instance_id,
            server_version: Some(Version::new(0, 17, 30)),
            ip: Some("203.0.113.7".parse().unwrap()),
            at,
        }
    }

    /// Containers starting together on one database, and each restart after,
    /// all get the same instance id.
    #[tokio::test]
    async fn every_container_on_a_database_shares_one_instance_id() {
        let (storage, _container) = test_storage().await;
        let services: Vec<_> = (0..8).map(|_| service(&storage)).collect();
        let ids = futures::future::join_all(services.iter().map(|s| s.server_instance_id())).await;
        let ids: Vec<Uuid> = ids.into_iter().map(Result::unwrap).collect();
        assert!(ids.iter().all(|id| *id == ids[0]));

        let restarted = service(&storage).server_instance_id().await.unwrap();
        assert_eq!(restarted, ids[0]);
    }

    #[tokio::test]
    async fn one_instance_is_within_a_limit_of_one() {
        let (storage, _container) = test_storage().await;
        let org = org_id(&storage).await;
        let recorded = service(&storage)
            .record_check_in(check_in(org, Some(Uuid::new_v4()), Utc::now()), Some(1))
            .await
            .unwrap();
        assert_eq!(
            recorded.usage,
            InstanceUsage {
                active: 1,
                limit: Some(1)
            }
        );
        assert!(recorded.holds_slot);
    }

    /// The server that arrives past the limit is refused, however often the
    /// two check in after that; the first keeps its slot.
    #[tokio::test]
    async fn a_second_instance_is_refused_and_the_first_keeps_its_slot() {
        let (storage, _container) = test_storage().await;
        let org = org_id(&storage).await;
        let service = service(&storage);
        let first = Some(Uuid::new_v4());
        let second = Some(Uuid::new_v4());
        let start = Utc::now();
        service
            .record_check_in(check_in(org, first, start), Some(1))
            .await
            .unwrap();

        for minutes in 1..3 {
            let at = start + chrono::Duration::minutes(minutes);
            let refused = service
                .record_check_in(check_in(org, second, at), Some(1))
                .await
                .unwrap();
            assert!(refused.usage.is_over());
            assert!(!refused.holds_slot);

            let kept = service
                .record_check_in(check_in(org, first, at), Some(1))
                .await
                .unwrap();
            assert!(kept.holds_slot);
        }
    }

    #[tokio::test]
    async fn no_cap_gives_every_instance_a_slot() {
        let (storage, _container) = test_storage().await;
        let org = org_id(&storage).await;
        let service = service(&storage);
        for _ in 0..3 {
            let recorded = service
                .record_check_in(check_in(org, Some(Uuid::new_v4()), Utc::now()), None)
                .await
                .unwrap();
            assert!(recorded.holds_slot);
            assert!(!recorded.usage.is_over());
        }
    }

    /// The one old server on a key upgrades and keeps its slot. A second old
    /// server still running beside it is then the one past the limit.
    #[tokio::test]
    async fn an_upgraded_server_takes_over_the_old_servers_slot() {
        let (storage, _container) = test_storage().await;
        let org = org_id(&storage).await;
        let service = service(&storage);
        let start = Utc::now();
        service
            .record_check_in(check_in(org, None, start), Some(1))
            .await
            .unwrap();

        let upgraded = Some(Uuid::new_v4());
        let recorded = service
            .record_check_in(
                check_in(org, upgraded, start + chrono::Duration::minutes(1)),
                Some(1),
            )
            .await
            .unwrap();
        assert!(recorded.holds_slot);
        let instances = service.list(org).await.unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].base.instance_id, upgraded);

        let still_old = service
            .record_check_in(
                check_in(org, None, start + chrono::Duration::minutes(2)),
                Some(1),
            )
            .await
            .unwrap();
        assert!(!still_old.holds_slot);
    }

    /// A restart, and a rolling deploy with old and new containers checking in
    /// side by side on different versions, are one instance.
    #[tokio::test]
    async fn check_ins_from_one_database_count_once_across_versions() {
        let (storage, _container) = test_storage().await;
        let org = org_id(&storage).await;
        let service = service(&storage);
        let instance_id = Some(Uuid::new_v4());

        let concurrent = (0..4).map(|minor| {
            let mut c = check_in(org, instance_id, Utc::now());
            c.server_version = Some(Version::new(0, 17, 30 + minor));
            service.record_check_in(c, Some(1))
        });
        for recorded in futures::future::join_all(concurrent).await {
            assert!(!recorded.unwrap().usage.is_over());
        }

        let recorded = service
            .record_check_in(check_in(org, instance_id, Utc::now()), Some(1))
            .await
            .unwrap();
        assert_eq!(recorded.usage.active, 1);
        assert_eq!(service.list(org).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn servers_without_an_instance_id_share_one_slot() {
        let (storage, _container) = test_storage().await;
        let org = org_id(&storage).await;
        let service = service(&storage);
        for _ in 0..3 {
            let mut old = check_in(org, None, Utc::now());
            old.server_version = None;
            service.record_check_in(old, Some(1)).await.unwrap();
        }
        let instances = service.list(org).await.unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].base.instance_id, None);
    }

    #[tokio::test]
    async fn deleting_an_instance_frees_its_slot() {
        let (storage, _container) = test_storage().await;
        let org = org_id(&storage).await;
        let service = service(&storage);
        let retired = Some(Uuid::new_v4());
        let current = Some(Uuid::new_v4());
        service
            .record_check_in(check_in(org, retired, Utc::now()), Some(1))
            .await
            .unwrap();
        service
            .record_check_in(check_in(org, current, Utc::now()), Some(1))
            .await
            .unwrap();

        let row = service
            .list(org)
            .await
            .unwrap()
            .into_iter()
            .find(|i| i.base.instance_id == retired)
            .unwrap();
        assert!(service.delete(org, row.id).await.unwrap());

        let recorded = service
            .record_check_in(check_in(org, current, Utc::now()), Some(1))
            .await
            .unwrap();
        assert!(recorded.holds_slot);
    }

    #[tokio::test]
    async fn an_instance_silent_past_the_threshold_frees_its_slot() {
        let (storage, _container) = test_storage().await;
        let org = org_id(&storage).await;
        let service = service(&storage);
        let now = Utc::now();
        service
            .record_check_in(
                check_in(org, Some(Uuid::new_v4()), now - chrono::Duration::days(8)),
                Some(1),
            )
            .await
            .unwrap();
        let recorded = service
            .record_check_in(check_in(org, Some(Uuid::new_v4()), now), Some(1))
            .await
            .unwrap();
        assert_eq!(recorded.usage.active, 1);
        assert!(recorded.holds_slot);
    }

    #[tokio::test]
    async fn an_organization_cannot_delete_another_organizations_instance() {
        let (storage, _container) = test_storage().await;
        let owner = org_id(&storage).await;
        let other = org_id(&storage).await;
        let service = service(&storage);
        service
            .record_check_in(check_in(owner, Some(Uuid::new_v4()), Utc::now()), Some(1))
            .await
            .unwrap();
        let row = service.list(owner).await.unwrap().remove(0);

        assert!(!service.delete(other, row.id).await.unwrap());
        assert_eq!(service.list(owner).await.unwrap().len(), 1);
    }
}
