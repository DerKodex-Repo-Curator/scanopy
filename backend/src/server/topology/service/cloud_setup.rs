//! Cloud setup: the site, its system subnets, the live topology row and the starter tags
//! a cloud org works from.
//!
//! This is the one place that sequence is created. Registration runs it for
//! the site requested at signup (`AuthService::provision_user`), and billing
//! events that land an org on a cloud plan run it too, so a self-hosted license
//! buyer who moves to cloud ends up with what a cloud signup has. The
//! site-create and org-reset handlers call the same functions. Every step is
//! idempotent: it fills only what's missing, so a bus retry or a repeat event
//! never duplicates anything, and `LockKey::CloudSetup` keeps two runs for one
//! org from both creating a site.

use anyhow::Error;
use async_trait::async_trait;
use uuid::Uuid;

use crate::server::{
    auth::{r#impl::base::PendingSiteSetup, middleware::auth::AuthenticatedEntity},
    billing::types::base::BillingPlan,
    shared::{
        events::{
            registry::SubscriberRegistration,
            traits::{Event, EventFilter, Subscriber},
            types::{BillingOperation, BillingOperationDiscriminants},
        },
        services::traits::CrudService,
        storage::{
            filter::StorableFilter,
            lock::{DEFAULT_LOCK_TIMEOUT, LockKey},
            seed_data::{
                create_remote_subnet, create_starter_tags, create_wan_subnet,
                starter_custom_group_names,
            },
            traits::Storable,
        },
    },
    sites::r#impl::{Site, SiteBase},
    subnets::{r#impl::base::Subnet, r#impl::types::SubnetType},
    tags::{groups::base::TagGroup, r#impl::base::Tag},
    topology::{
        service::main::TopologyService,
        types::base::{Topology, TopologyBase},
    },
};

/// Subnet types every site is seeded with.
const SYSTEM_SUBNET_TYPES: [SubnetType; 2] = [SubnetType::Internet, SubnetType::Remote];

/// What one site lacks compared with a freshly created one.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct SiteSetupGaps {
    pub topology: bool,
    pub subnet_types: Vec<SubnetType>,
}

impl SiteSetupGaps {
    pub(crate) fn find(has_topology: bool, existing_subnet_types: &[SubnetType]) -> Self {
        Self {
            topology: !has_topology,
            subnet_types: SYSTEM_SUBNET_TYPES
                .into_iter()
                .filter(|t| !existing_subnet_types.contains(t))
                .collect(),
        }
    }
}

/// The starter tags an org lacks, by name, in its `groups`. Names compare case-insensitively, so
/// an org that already made its own "active" tag doesn't get a second one.
pub(crate) fn missing_starter_tags(
    organization_id: Uuid,
    groups: &[TagGroup],
    existing_names: &[String],
) -> Vec<Tag> {
    create_starter_tags(organization_id, groups)
        .into_iter()
        .filter(|tag| {
            !existing_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&tag.base.name))
        })
        .collect()
}

/// Whether a billing event puts the org on a cloud plan it may not have been
/// set up for: a first plan pick or resubscribe, or a move off a license plan.
/// A move between cloud plans and renewals are excluded, so a cloud org that
/// removed its sites doesn't get one back on every billing change.
pub(crate) fn lands_on_cloud_plan(operation: &BillingOperation) -> bool {
    let is_cloud = |plan: &BillingPlan| plan.license_plan().is_none();
    match operation {
        BillingOperation::TrialStarted { plan, .. }
        | BillingOperation::CheckoutCompleted { plan, .. } => is_cloud(plan),
        BillingOperation::PlanChanged { from, to, .. } => !is_cloud(from) && is_cloud(to),
        _ => false,
    }
}

impl TopologyService {
    /// Give the org a site if it has none, then fill in each site's system
    /// subnets and live topology. `requested` is the site named at signup; an
    /// org without one gets the default site.
    pub async fn ensure_cloud_setup(
        &self,
        organization_id: Uuid,
        requested: Option<&PendingSiteSetup>,
        authentication: AuthenticatedEntity,
    ) -> Result<(), Error> {
        let lock = self
            .storage()
            .session_lock(
                LockKey::CloudSetup { organization_id },
                DEFAULT_LOCK_TIMEOUT,
            )
            .await?;

        let mut sites = self
            .site_service
            .get_all(StorableFilter::<Site>::new_from_org_id(&organization_id))
            .await?;

        if sites.is_empty() {
            let mut site = Site::new(SiteBase::new(organization_id));
            if let Some(requested) = requested {
                site.id = requested.site_id;
                site.base.name = requested.name.clone();
            }
            sites.push(
                self.site_service
                    .create(site, authentication.clone())
                    .await?,
            );
        }

        for site in &sites {
            self.ensure_site_setup(site.id, authentication.clone())
                .await?;
        }

        self.ensure_starter_tags(organization_id, authentication)
            .await?;

        lock.release().await?;
        Ok(())
    }

    /// Create whichever of the system groups, starter groups and starter tags the org lacks. A
    /// group or tag the org already has by that name is kept as it is, so a person's edits to it
    /// survive a repeat run.
    async fn ensure_starter_tags(
        &self,
        organization_id: Uuid,
        authentication: AuthenticatedEntity,
    ) -> Result<(), Error> {
        let existing: Vec<String> = self
            .tag_service
            .get_all(StorableFilter::<Tag>::new_from_org_id(&organization_id).live())
            .await?
            .into_iter()
            .map(|tag| tag.base.name)
            .collect();

        let groups = self
            .tag_service
            .tag_group_service()
            .ensure_groups(
                organization_id,
                &starter_custom_group_names(),
                authentication.clone(),
            )
            .await?;

        for tag in missing_starter_tags(organization_id, &groups, &existing) {
            self.tag_service.create(tag, authentication.clone()).await?;
        }
        Ok(())
    }

    /// Create whichever of the site's system subnets and live topology row
    /// are missing.
    pub async fn ensure_site_setup(
        &self,
        site_id: Uuid,
        authentication: AuthenticatedEntity,
    ) -> Result<(), Error> {
        let has_topology = !self
            .get_all(StorableFilter::<Topology>::new_from_site_ids(&[site_id]))
            .await?
            .is_empty();
        let subnet_types: Vec<SubnetType> = self
            .subnet_service
            .get_all(StorableFilter::<Subnet>::new_from_site_ids(&[site_id]))
            .await?
            .into_iter()
            .map(|s| s.base.subnet_type)
            .collect();

        let gaps = SiteSetupGaps::find(has_topology, &subnet_types);

        for subnet_type in gaps.subnet_types {
            let subnet = match subnet_type {
                SubnetType::Internet => create_wan_subnet(site_id),
                SubnetType::Remote => create_remote_subnet(site_id),
                _ => continue,
            };
            self.subnet_service
                .create(subnet, authentication.clone())
                .await?;
        }

        if gaps.topology {
            let site = self
                .site_service
                .get_by_id(&site_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Site {site_id} not found"))?;
            let mut base = TopologyBase::new(site_id);
            base.options.request = self
                .default_request_options(site.base.organization_id)
                .await?;
            self.create(Topology::new(base), authentication).await?;
        }

        Ok(())
    }
}

#[async_trait]
impl Subscriber<BillingOperation> for TopologyService {
    fn filter(&self) -> EventFilter<BillingOperation> {
        EventFilter::ops(vec![
            BillingOperationDiscriminants::TrialStarted,
            BillingOperationDiscriminants::CheckoutCompleted,
            BillingOperationDiscriminants::PlanChanged,
        ])
    }

    async fn handle(&self, events: Vec<Event<BillingOperation>>) -> Result<(), Error> {
        for event in events {
            if lands_on_cloud_plan(&event.operation) {
                self.ensure_cloud_setup(
                    event.scope.organization_id,
                    None,
                    event.authentication.clone(),
                )
                .await?;
            }
        }
        Ok(())
    }
}

inventory::submit!(SubscriberRegistration::new::<
    TopologyService,
    BillingOperation,
>());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::billing::types::base::PlanConfig;

    fn plan_changed(from: BillingPlan, to: BillingPlan) -> BillingOperation {
        BillingOperation::PlanChanged {
            from,
            to,
            is_downgrade: false,
            next_renewal_at: None,
            license_key_type: None,
        }
    }

    #[test]
    fn only_moves_onto_cloud_trigger_setup() {
        let cloud = BillingPlan::Pro(PlanConfig::default());
        let other_cloud = BillingPlan::Starter(PlanConfig::default());
        let licensed = BillingPlan::SelfHostedStandard(PlanConfig::default());
        let trial = |plan: BillingPlan| BillingOperation::TrialStarted {
            plan,
            trial_end: chrono::Utc::now(),
            trial_days: 14,
        };

        assert!(lands_on_cloud_plan(&trial(cloud)));
        assert!(!lands_on_cloud_plan(&trial(licensed)));
        assert!(lands_on_cloud_plan(&plan_changed(licensed, cloud)));
        assert!(!lands_on_cloud_plan(&plan_changed(cloud, licensed)));
        assert!(!lands_on_cloud_plan(&plan_changed(other_cloud, cloud)));
    }

    #[test]
    fn gaps_cover_only_what_is_missing() {
        assert_eq!(
            SiteSetupGaps::find(false, &[]),
            SiteSetupGaps {
                topology: true,
                subnet_types: vec![SubnetType::Internet, SubnetType::Remote],
            }
        );
        assert_eq!(
            SiteSetupGaps::find(true, &[SubnetType::Lan, SubnetType::Remote]),
            SiteSetupGaps {
                topology: false,
                subnet_types: vec![SubnetType::Internet],
            }
        );
        assert_eq!(
            SiteSetupGaps::find(true, &[SubnetType::Remote, SubnetType::Internet]),
            SiteSetupGaps::default()
        );
    }

    /// The groups ensure_groups gives a new org: the system groups and the starter custom groups.
    fn starter_groups(org: Uuid) -> Vec<TagGroup> {
        use crate::server::tags::groups::base::{TagGroupBase, TagGroupType};

        TagGroupType::system()
            .map(|t| TagGroupBase::new(org, t.to_string(), t))
            .chain(
                starter_custom_group_names()
                    .into_iter()
                    .map(|name| TagGroupBase::new(org, name.to_string(), TagGroupType::Custom)),
            )
            .map(TagGroup::new)
            .collect()
    }

    #[test]
    fn a_new_org_gets_every_starter_tag_in_its_group_in_order() {
        let org = Uuid::new_v4();
        let groups = starter_groups(org);
        let tags = missing_starter_tags(org, &groups, &[]);
        assert!(
            tags.iter()
                .all(|t| t.base.organization_id == org && t.base.icon.is_some())
        );
        let group_of = |name: &str| {
            let tag = tags.iter().find(|t| t.base.name == name).unwrap();
            let group = groups
                .iter()
                .find(|g| Some(g.id) == tag.base.tag_group_id)
                .unwrap();
            (group.base.name.clone(), tag.base.position)
        };
        assert_eq!(group_of("Planned"), ("Status".to_string(), Some(1)));
        assert_eq!(group_of("Decommissioned"), ("Status".to_string(), Some(4)));
        assert_eq!(group_of("Production").0, "Environment");
        assert_eq!(group_of("Critical").0, "Criticality");
        assert_eq!(group_of("Third-party").0, "Management");
    }

    #[test]
    fn a_second_run_creates_nothing() {
        let org = Uuid::new_v4();
        let groups = starter_groups(org);
        let existing: Vec<String> = missing_starter_tags(org, &groups, &[])
            .into_iter()
            .map(|t| t.base.name)
            .collect();
        assert!(missing_starter_tags(org, &groups, &existing).is_empty());
    }

    /// A tag the org made itself under one of these names is kept, never duplicated.
    #[test]
    fn an_existing_tag_of_the_same_name_is_kept() {
        let org = Uuid::new_v4();
        let groups = starter_groups(org);
        let all = missing_starter_tags(org, &groups, &[]);
        let taken = all[0].base.name.to_lowercase();
        let missing = missing_starter_tags(org, &groups, &[taken.clone(), "Printers".to_string()]);
        assert_eq!(missing.len(), all.len() - 1);
        assert!(
            missing
                .iter()
                .all(|t| !t.base.name.eq_ignore_ascii_case(&taken))
        );
    }
}
