use crate::server::auth::middleware::auth::AuthenticatedEntity;
use crate::server::billing::types::base::BillingPlan;
use crate::server::credentials::service::CredentialService;
use crate::server::license::mint::MintError;
use crate::server::license::types::LicenseKeyType;
use crate::server::organizations::demo_status::DemoPopulateStatus;
use crate::server::organizations::r#impl::base::LICENSE_SILENCE_THRESHOLD_DAYS;
use crate::server::shared::events::bus::EventBus;
use crate::server::shared::events::traits::{Event, OrgScope};
use crate::server::shared::events::types::{BillingOperation, OnboardingOperationDiscriminants};
use crate::server::shared::services::traits::EventBusService;
use crate::server::shared::storage::filter::StorableFilter;
use crate::server::shared::storage::lock::{
    DEFAULT_LOCK_TIMEOUT, LockError, LockKey, SessionLockGuard,
};
use crate::server::shared::storage::traits::Storage;
use crate::server::shared::types::metadata::HasId;
use crate::server::tags::entity_tags::EntityTagService;
use crate::server::{
    organizations::r#impl::base::Organization,
    shared::{services::traits::CrudService, storage::generic::GenericPostgresStorage},
};
use anyhow::Error;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use semver::Version;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct OrganizationService {
    storage: Arc<GenericPostgresStorage<Organization>>,
    event_bus: Arc<EventBus>,
    /// In-memory status of each org's background demo-populate task, polled by
    /// the frontend after the `202`. See [`super::demo_status`].
    demo_status: RwLock<HashMap<Uuid, DemoPopulateStatus>>,
    /// Seeds each new organization's default credentials. Set after construction, since the
    /// credential service depends on this one.
    credential_service: OnceLock<Arc<CredentialService>>,
}

impl EventBusService<Organization> for OrganizationService {
    fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    fn get_site_id(&self, _entity: &Organization) -> Option<Uuid> {
        None
    }
    fn get_organization_id(&self, entity: &Organization) -> Option<Uuid> {
        Some(entity.id)
    }
}

#[async_trait]
impl CrudService<Organization> for OrganizationService {
    fn storage(&self) -> &Arc<GenericPostgresStorage<Organization>> {
        &self.storage
    }

    fn entity_tag_service(&self) -> Option<&Arc<EntityTagService>> {
        None
    }

    /// A new organization starts with its default credentials, before anything creates its
    /// first site, so that site can be assigned the SNMP default.
    async fn create(
        &self,
        entity: Organization,
        authentication: AuthenticatedEntity,
    ) -> Result<Organization, Error> {
        let created = self.create_base(entity, authentication).await?;
        match self.credential_service.get() {
            Some(credentials) => {
                credentials.seed_defaults(created.id).await?;
            }
            None => tracing::warn!(
                organization_id = %created.id,
                "Credential service not set; the organization starts without default credentials"
            ),
        }
        Ok(created)
    }
}

/// Why a license key type switch was refused.
#[derive(Debug, thiserror::Error)]
pub enum SwitchKeyTypeError {
    /// Leaving an air-gapped key before the licence period is up. The key
    /// already installed on the customer's server validates offline and runs
    /// to its embedded expiry whatever the cloud does, so switching would
    /// leave two live keys with no way to retire the one that matters.
    #[error(
        "an air-gapped key stays valid until {paid_through}; you can switch back to an online key from that date"
    )]
    AirGappedStillCurrent { paid_through: DateTime<Utc> },
    /// The organization cannot be issued the key type it asked for, so the
    /// switch was abandoned instead of persisted. Keeping the old type is what
    /// stops a refused mint stranding the org: the switch retires the previous
    /// key, and the read path mints whatever the row says, so a stored type
    /// that cannot be signed would fail every later read.
    #[error(transparent)]
    Mint(#[from] MintError),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error(transparent)]
    Service(#[from] Error),
}

/// Drop sub-second precision, so a value survives a database round trip
/// unchanged.
fn whole_second(at: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::from_timestamp(at.timestamp(), 0).unwrap_or(at)
}

/// An organization row read under its advisory lock. Change `org`, awaiting other work in between
/// if the change depends on it, then [`save`](Self::save) it. Dropping it unsaved writes nothing
/// and releases the lock.
pub struct LockedOrganization<'a> {
    pub org: Organization,
    lock: SessionLockGuard,
    service: &'a OrganizationService,
}

impl LockedOrganization<'_> {
    /// Write the row, publish its `Updated` event, and release the lock.
    pub async fn save(
        mut self,
        authentication: AuthenticatedEntity,
    ) -> Result<Organization, Error> {
        let updated = self.service.update(&mut self.org, authentication).await?;
        self.lock.release().await?;
        Ok(updated)
    }

    /// Write the row without an entity event, for frequent bookkeeping no user edits (license
    /// check-ins, notification ratchets).
    pub async fn save_quietly(mut self) -> Result<Organization, Error> {
        let updated = self.service.storage.update(&mut self.org).await?;
        self.lock.release().await?;
        Ok(updated)
    }

    /// Release the lock without writing.
    pub async fn release(self) -> Result<(), Error> {
        Ok(self.lock.release().await?)
    }
}

impl OrganizationService {
    /// Hold the organization row's advisory lock. Every read-modify-write of
    /// an org takes it, through [`lock_for_update`](Self::lock_for_update),
    /// because each writes the whole row back and would otherwise revert a
    /// concurrent writer's change.
    pub(crate) async fn lock_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<SessionLockGuard, Error> {
        Ok(self
            .storage
            .session_lock(LockKey::Organization(organization_id), DEFAULT_LOCK_TIMEOUT)
            .await?)
    }

    /// Lock the organization and read it fresh, for a read-modify-write. `None` (lock released)
    /// when the organization doesn't exist.
    pub async fn lock_for_update(
        &self,
        organization_id: Uuid,
    ) -> Result<Option<LockedOrganization<'_>>, Error> {
        let lock = self.lock_organization(organization_id).await?;
        match self.get_by_id(&organization_id).await? {
            Some(org) => Ok(Some(LockedOrganization {
                org,
                lock,
                service: self,
            })),
            None => {
                lock.release().await?;
                Ok(None)
            }
        }
    }

    /// Apply `change` to the organization under its lock and save it with an `Updated` event.
    /// Returns what `change` returned, or `None` when the organization doesn't exist.
    pub async fn modify<T>(
        &self,
        organization_id: Uuid,
        authentication: AuthenticatedEntity,
        change: impl FnOnce(&mut Organization) -> T,
    ) -> Result<Option<T>, Error> {
        let Some(mut locked) = self.lock_for_update(organization_id).await? else {
            return Ok(None);
        };
        let result = change(&mut locked.org);
        locked.save(authentication).await?;
        Ok(Some(result))
    }

    /// Record an onboarding milestone. `true` only for the call that added it, so the caller
    /// publishes the milestone event once however many requests reach it together.
    pub async fn record_milestone(
        &self,
        organization_id: Uuid,
        step: OnboardingOperationDiscriminants,
    ) -> Result<bool, Error> {
        let Some(mut locked) = self.lock_for_update(organization_id).await? else {
            return Ok(false);
        };
        if !locked.org.not_onboarded(&step) {
            locked.release().await?;
            return Ok(false);
        }
        locked.org.base.onboarding.push(step);
        locked.save(AuthenticatedEntity::System).await?;
        Ok(true)
    }

    /// Record that a self-hosted server fetched an entitlement. Publishes no
    /// entity event: check-ins are frequent and change nothing a user edits.
    /// The billing events a check-in can produce (activation, upgrade, resume)
    /// fire at most once per key, version or silence; see
    /// [`Organization::record_license_check_in`].
    pub async fn record_license_check_in(
        &self,
        organization_id: Uuid,
        at: DateTime<Utc>,
        server_version: Option<Version>,
    ) -> Result<(), Error> {
        let mut events = Vec::new();
        if let Some(mut locked) = self.lock_for_update(organization_id).await? {
            events = locked.org.record_license_check_in(at, server_version);
            locked.save_quietly().await?;
        }

        for operation in events {
            self.publish_license_event(organization_id, operation).await;
        }
        Ok(())
    }

    /// Publish a license telemetry event. Best-effort: the state it describes
    /// is already written, so a failed publish is logged rather than failing
    /// the request that caused it.
    async fn publish_license_event(&self, organization_id: Uuid, operation: BillingOperation) {
        let name = operation.to_string();
        if let Err(e) = self
            .event_bus()
            .publish(Event::new(
                OrgScope { organization_id },
                operation,
                AuthenticatedEntity::System,
            ))
            .await
        {
            tracing::warn!(
                organization_id = %organization_id,
                event = %name,
                error = %e,
                "Failed to publish license event",
            );
        }
    }

    /// Report every online key that has stopped checking in. Run daily; each
    /// silence is reported once, see [`Organization::report_license_silence`].
    /// Returns the number reported.
    pub async fn report_silent_license_check_ins(&self, now: DateTime<Utc>) -> Result<u64, Error> {
        let cutoff = now - chrono::Duration::days(LICENSE_SILENCE_THRESHOLD_DAYS);
        let candidates = self
            .get_all(StorableFilter::<Organization>::new().license_checkin_before(cutoff))
            .await?;

        let mut reported = 0u64;
        for candidate in candidates {
            let mut operation = None;
            if let Some(mut locked) = self.lock_for_update(candidate.id).await? {
                operation = locked.org.report_license_silence(now);
                if operation.is_some() {
                    locked.save_quietly().await?;
                } else {
                    locked.release().await?;
                }
            }

            if let Some(operation) = operation {
                self.publish_license_event(candidate.id, operation).await;
                reported += 1;
            }
        }

        if reported > 0 {
            tracing::info!(
                count = reported,
                "Reported license keys that stopped checking in"
            );
        }
        Ok(reported)
    }

    /// The `iat` this org's online license key is signed with, assigned on
    /// first use. Holding it makes minting deterministic: every copy of the
    /// key returns the same string until the key is rotated.
    pub async fn license_key_issued_at(
        &self,
        organization_id: Uuid,
    ) -> Result<DateTime<Utc>, Error> {
        let mut locked = self
            .lock_for_update(organization_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Organization {organization_id} not found"))?;

        // Rotation and switching set the stamp themselves, so an unset stamp
        // means the org has never been issued a key.
        let first_issue = locked.org.base.license_key_issued_at.is_none();
        let issued_at = match locked.org.base.license_key_issued_at {
            Some(issued_at) => {
                locked.release().await?;
                issued_at
            }
            None => {
                // Whole seconds: the claim carries seconds, and Postgres keeps
                // microseconds, so a truncating round trip would otherwise hand
                // back a different stamp than the one just written.
                let issued_at = whole_second(Utc::now());
                locked.org.base.license_key_issued_at = Some(issued_at);
                let organization = locked.save_quietly().await?;
                tracing::info!(
                    organization_id = %organization_id,
                    key_version = organization.base.license_key_version,
                    "Issued online license key"
                );
                issued_at
            }
        };

        if first_issue {
            self.publish_license_event(organization_id, BillingOperation::LicenseKeyIssued)
                .await;
        }
        Ok(issued_at)
    }

    /// Make `key_type` the org's issued key, retiring the previous one when
    /// the type actually changes. A request for the type already issued is a
    /// re-read and changes nothing, so copying a key twice returns the same
    /// string.
    ///
    /// Retiring works by bumping the key version, which the entitlement
    /// endpoint checks. That kills an online key at once. An air-gapped key
    /// carries no version and never checks in, so a copy already installed on
    /// a server keeps working until its embedded expiry; the UI says so.
    ///
    /// `can_issue` decides whether the org may hold `key_type`, and is given
    /// the state about to be written rather than the current row. A refusal
    /// abandons the switch: the version bump retires the previous key and
    /// cannot be undone, so persisting a type the org cannot mint would leave
    /// it with no usable key and no way back.
    pub async fn switch_license_key_type(
        &self,
        organization_id: Uuid,
        key_type: LicenseKeyType,
        authentication: AuthenticatedEntity,
        can_issue: impl Fn(&Organization) -> Result<(), MintError>,
    ) -> Result<Organization, SwitchKeyTypeError> {
        let mut locked = self
            .lock_for_update(organization_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Organization {organization_id} not found"))?;

        if locked.org.base.license_key_type.unwrap_or_default() == key_type {
            let organization = locked.org.clone();
            locked.release().await?;
            return Ok(organization);
        }

        // Leaving an air-gapped key is only allowed once the licence period is
        // up. Until then the key already installed on their server keeps
        // working whatever we do here, so letting them switch would leave two
        // live keys and no way to retire the one that matters.
        if key_type == LicenseKeyType::Online
            && let Some(paid_through) = locked.org.air_gapped_key_current_until()
        {
            locked.release().await?;
            return Err(SwitchKeyTypeError::AirGappedStillCurrent { paid_through });
        }

        locked.org.base.license_key_type = Some(key_type);
        locked.org.base.license_key_version = locked
            .org
            .base
            .license_key_version
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("License key version overflow"))?;
        locked.org.base.license_key_issued_at = Some(whole_second(Utc::now()));

        // Dry-run the mint against the state about to be written. The loaded
        // copy is mutated but not yet persisted, so a refusal here leaves the
        // row untouched, and the candidate already carries the new version an
        // online key embeds — exact for both key types rather than an
        // approximation from the pre-switch row.
        //
        // This belongs under the lock rather than in the caller: billing
        // webhooks write `plan_status`, `has_payment_method` and
        // `license_paid_through` under the same lock, so a Stripe webhook
        // could land between a caller's own check and this write.
        if let Err(e) = can_issue(&locked.org) {
            locked.release().await?;
            return Err(SwitchKeyTypeError::Mint(e));
        }

        let updated = locked.save(authentication).await?;
        tracing::info!(
            organization_id = %organization_id,
            key_type = %key_type,
            key_version = updated.base.license_key_version,
            "Switched license key type"
        );

        Ok(updated)
    }

    /// Point a stranded organization back at an online key.
    ///
    /// Only for an org stored as air-gapped that can no longer be issued an
    /// air-gapped key. Such a row is trapped in both directions: every read
    /// mints the stored type and fails, and the switch back is refused while
    /// the paid-through date is still ahead. Online is the type it would have
    /// kept had the refused switch never applied.
    ///
    /// The key version stays where it is. The bump that retired the previous
    /// key has already happened, and reviving a retired key is the one thing
    /// this must not do.
    pub async fn revert_license_key_type_to_online(
        &self,
        organization_id: Uuid,
    ) -> Result<Organization, Error> {
        let mut locked = self
            .lock_for_update(organization_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Organization {organization_id} not found"))?;
        locked.org.base.license_key_type = Some(LicenseKeyType::Online);
        let updated = locked.save(AuthenticatedEntity::System).await?;
        tracing::warn!(
            organization_id = %organization_id,
            key_version = updated.base.license_key_version,
            "Reverted a stranded organization to an online license key"
        );
        Ok(updated)
    }

    /// Increment the org's license key version, retiring every key issued so
    /// far. A server still running the old online key gets 403 at its next
    /// check-in and needs the new one.
    pub async fn rotate_license_key(
        &self,
        organization_id: Uuid,
        authentication: AuthenticatedEntity,
    ) -> Result<Organization, Error> {
        let mut locked = self
            .lock_for_update(organization_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Organization {organization_id} not found"))?;
        locked.org.base.license_key_version = locked
            .org
            .base
            .license_key_version
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("License key version overflow"))?;
        // Move the stamp too. The version alone already changes the key; this
        // keeps the stamp honest about when the current key was issued.
        locked.org.base.license_key_issued_at = Some(whole_second(Utc::now()));
        let updated = locked.save(authentication).await?;
        tracing::info!(
            organization_id = %organization_id,
            key_version = updated.base.license_key_version,
            "Rotated license key"
        );
        Ok(updated)
    }
}

impl OrganizationService {
    pub fn new(
        storage: Arc<GenericPostgresStorage<Organization>>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            storage,
            event_bus,
            demo_status: RwLock::new(HashMap::new()),
            credential_service: OnceLock::new(),
        }
    }

    /// Set the credential service dependency after construction (breaks circular dep).
    pub fn set_credential_service(
        &self,
        service: Arc<CredentialService>,
    ) -> Result<(), Arc<CredentialService>> {
        self.credential_service.set(service)
    }

    /// Claim the single-flight demo-populate slot for `org_id`. Returns the
    /// initial `Running` status (to hand straight back in the `202` body) when
    /// the slot was free, or `None` if a populate is already `Running` for this
    /// org — the caller should then respond `409`.
    pub async fn try_begin_demo(&self, org_id: Uuid) -> Option<DemoPopulateStatus> {
        let mut map = self.demo_status.write().await;
        if matches!(map.get(&org_id), Some(DemoPopulateStatus::Running { .. })) {
            return None;
        }
        let status = DemoPopulateStatus::Running {
            started_at: Utc::now(),
        };
        map.insert(org_id, status.clone());
        Some(status)
    }

    /// Record the terminal status of an org's demo-populate task.
    pub async fn set_demo_status(&self, org_id: Uuid, status: DemoPopulateStatus) {
        self.demo_status.write().await.insert(org_id, status);
    }

    /// Current demo-populate status for `org_id`, if a task has ever run.
    pub async fn get_demo_status(&self, org_id: &Uuid) -> Option<DemoPopulateStatus> {
        self.demo_status.read().await.get(org_id).cloned()
    }

    /// Reconcile self-hosted org plans to the `target` plan the license key
    /// entitles (resolved by the caller via `plan_for_license`). Every org whose
    /// current plan differs from `target` is moved onto it. The caller gates this
    /// on a self-hosted deployment (no Stripe secret) with `LicenseStatus::Valid`.
    ///
    /// Grandfathered customers hold claim-absent keys, which resolve to the same
    /// `CommercialSelfHosted` plan their orgs already carry, so this is a no-op
    /// for them. Idempotent — plans equal to `target` are skipped, so re-running
    /// on every boot does nothing once reconciled. The plan is written through the
    /// `LicenseReconciled` billing event's [`BillingOperation::apply`] under the
    /// org lock, then the event is published. Best-effort: a per-org failure is
    /// logged, not fatal. Returns the number of orgs moved.
    pub async fn reconcile_self_hosted_license_plans(
        &self,
        target: BillingPlan,
    ) -> Result<u64, Error> {
        let orgs = self.get_all(StorableFilter::<Organization>::new()).await?;

        let mut upgraded = 0u64;
        for org in orgs {
            let reconciled = async {
                let Some(locked) = self.lock_for_update(org.id).await? else {
                    return Ok(false);
                };
                // `None` resolves to the build default. Plan equality is
                // config-based (see `BillingPlan`'s `PartialEq`), so an org
                // already on `target` is skipped regardless of how its plan
                // was set.
                let current = locked.org.base.plan.unwrap_or_default();
                if current == target {
                    locked.release().await?;
                    return Ok(false);
                }
                let event = Event::new(
                    OrgScope {
                        organization_id: org.id,
                    },
                    BillingOperation::LicenseReconciled {
                        from: current,
                        to: target,
                    },
                    AuthenticatedEntity::System,
                );
                self.commit_billing(locked, None, vec![event]).await?;
                Ok::<_, Error>(true)
            }
            .await;

            match reconciled {
                Ok(true) => upgraded += 1,
                Ok(false) => {}
                Err(e) => tracing::warn!(
                    organization_id = %org.id,
                    error = %e,
                    "Failed to reconcile license plan for org",
                ),
            }
        }

        if upgraded > 0 {
            tracing::info!(
                count = upgraded,
                plan = %target.id(),
                "Reconciled self-hosted org plan(s) to license entitlement",
            );
        }

        Ok(upgraded)
    }

    /// Write the online license key's latest entitlement (`None` after the
    /// cloud rejected the key) and the check-in time to every org. The license
    /// is instance-level, so every row holds the same value. Written through
    /// storage, not `update`: this is server bookkeeping, and an `Updated`
    /// entity event per org on every check-in would carry nothing.
    pub async fn store_license_entitlement(
        &self,
        entitlement: Option<String>,
        checked_at: DateTime<Utc>,
    ) -> Result<(), Error> {
        for org in self.get_all(StorableFilter::<Organization>::new()).await? {
            let Some(mut locked) = self.lock_for_update(org.id).await? else {
                continue;
            };
            locked.org.base.license_entitlement = entitlement.clone();
            locked.org.base.license_entitlement_at = Some(checked_at);
            locked.save_quietly().await?;
        }
        Ok(())
    }

    /// The persisted entitlement with the latest check-in time across orgs,
    /// and that time.
    pub async fn load_license_entitlement(
        &self,
    ) -> Result<Option<(String, Option<DateTime<Utc>>)>, Error> {
        let orgs = self.get_all(StorableFilter::<Organization>::new()).await?;
        Ok(orgs
            .into_iter()
            .filter_map(|org| {
                Some((
                    org.base.license_entitlement?,
                    org.base.license_entitlement_at,
                ))
            })
            .max_by_key(|(_, checked_at)| *checked_at))
    }
}
