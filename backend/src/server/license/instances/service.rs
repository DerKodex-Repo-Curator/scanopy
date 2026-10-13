use std::net::IpAddr;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use semver::Version;
use uuid::Uuid;

use super::{LicenseInstance, LicenseInstanceBase, ServerInstance, active_count};
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
    /// against `limit`. Check-ins with no instance id share one row.
    pub async fn record_check_in(&self, check_in: CheckIn, limit: u32) -> Result<InstanceUsage> {
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

    async fn record_check_in_locked(&self, check_in: CheckIn, limit: u32) -> Result<InstanceUsage> {
        let mut instances = self.list(check_in.organization_id).await?;
        let server_version = check_in.server_version.map(|v| v.to_string());

        match instances
            .iter_mut()
            .find(|i| i.base.instance_id == check_in.instance_id)
        {
            Some(instance) => {
                instance.base.last_seen_at = check_in.at;
                instance.base.last_seen_ip = check_in.ip.or(instance.base.last_seen_ip);
                instance.base.server_version =
                    server_version.or(instance.base.server_version.take());
                instance.updated_at = check_in.at;
                self.instances.update(instance).await?;
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
                instances.push(created);
            }
        }

        Ok(InstanceUsage {
            active: active_count(&instances, check_in.at),
            limit,
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
        let usage = service(&storage)
            .record_check_in(check_in(org, Some(Uuid::new_v4()), Utc::now()), 1)
            .await
            .unwrap();
        assert_eq!(
            usage,
            InstanceUsage {
                active: 1,
                limit: 1
            }
        );
        assert!(!usage.is_over());
    }

    #[tokio::test]
    async fn a_second_instance_goes_over_a_limit_of_one() {
        let (storage, _container) = test_storage().await;
        let org = org_id(&storage).await;
        let service = service(&storage);
        service
            .record_check_in(check_in(org, Some(Uuid::new_v4()), Utc::now()), 1)
            .await
            .unwrap();
        let usage = service
            .record_check_in(check_in(org, Some(Uuid::new_v4()), Utc::now()), 1)
            .await
            .unwrap();
        assert!(usage.is_over());
        assert_eq!(usage.active, 2);
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
            service.record_check_in(c, 1)
        });
        for usage in futures::future::join_all(concurrent).await {
            assert!(!usage.unwrap().is_over());
        }

        let usage = service
            .record_check_in(check_in(org, instance_id, Utc::now()), 1)
            .await
            .unwrap();
        assert_eq!(usage.active, 1);
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
            service.record_check_in(old, 1).await.unwrap();
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
            .record_check_in(check_in(org, retired, Utc::now()), 1)
            .await
            .unwrap();
        service
            .record_check_in(check_in(org, current, Utc::now()), 1)
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

        let usage = service
            .record_check_in(check_in(org, current, Utc::now()), 1)
            .await
            .unwrap();
        assert!(!usage.is_over());
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
                1,
            )
            .await
            .unwrap();
        let usage = service
            .record_check_in(check_in(org, Some(Uuid::new_v4()), now), 1)
            .await
            .unwrap();
        assert_eq!(usage.active, 1);
    }

    #[tokio::test]
    async fn an_organization_cannot_delete_another_organizations_instance() {
        let (storage, _container) = test_storage().await;
        let owner = org_id(&storage).await;
        let other = org_id(&storage).await;
        let service = service(&storage);
        service
            .record_check_in(check_in(owner, Some(Uuid::new_v4()), Utc::now()), 1)
            .await
            .unwrap();
        let row = service.list(owner).await.unwrap().remove(0);

        assert!(!service.delete(other, row.id).await.unwrap());
        assert_eq!(service.list(owner).await.unwrap().len(), 1);
    }
}
