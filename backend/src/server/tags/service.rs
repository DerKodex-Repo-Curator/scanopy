use crate::server::{
    auth::middleware::auth::AuthenticatedEntity,
    shared::{
        entities::ChangeTriggersTopologyStaleness,
        events::{
            bus::EventBus,
            traits::{EntityEventFlags, EntityScope, Event},
            types::EntityOperation,
        },
        services::traits::{CrudService, EventBusService, SnapshotMutator},
        storage::{
            filter::StorableFilter,
            generic::GenericPostgresStorage,
            traits::{Entity, Storage},
        },
        types::api::ValidationError,
    },
    tags::{
        entity_tags::{EntityTagStorage, entities_holding_several},
        groups::{
            base::{TagGroup, TagGroupType},
            service::TagGroupService,
        },
        r#impl::base::Tag,
    },
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

pub struct TagService {
    storage: Arc<GenericPostgresStorage<Tag>>,
    /// The tags module's own junction storage, read to check a tag group change against existing
    /// assignments.
    entity_tags: Arc<EntityTagStorage>,
    tag_group_service: Arc<TagGroupService>,
    event_bus: Arc<EventBus>,
}

impl EventBusService<Tag> for TagService {
    fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    fn get_site_id(&self, _entity: &Tag) -> Option<Uuid> {
        None
    }
    fn get_organization_id(&self, entity: &Tag) -> Option<Uuid> {
        Some(entity.base.organization_id)
    }
}

#[async_trait]
impl CrudService<Tag> for TagService {
    fn storage(&self) -> &Arc<GenericPostgresStorage<Tag>> {
        &self.storage
    }

    fn entity_tag_service(
        &self,
    ) -> Option<&Arc<crate::server::tags::entity_tags::EntityTagService>> {
        None
    }

    /// A tag joins the end of its group.
    async fn create(&self, mut entity: Tag, authentication: AuthenticatedEntity) -> Result<Tag> {
        entity.base.position = match self.group_for(&entity).await? {
            Some(group) => Some(self.tag_group_service.next_position(group.id).await?),
            None => None,
        };
        self.create_base(entity, authentication).await
    }

    /// Tag rename uses per-action close-and-clone (`SnapshotMutator`) instead
    /// of in-place UPDATE: the prior field values are preserved on a closed
    /// historical row with `lineage_id` lineage; the live row keeps its id
    /// and gets the new field values plus advanced `valid_from`. Stable ids
    /// mean `entity_tags.tag_id` references survive renames without cascade
    /// UPDATE, and as-of joins through the OR-pattern resolve the right name
    /// at any point in time.
    async fn update(
        &self,
        entity: &mut Tag,
        authentication: AuthenticatedEntity,
    ) -> Result<Tag, anyhow::Error> {
        let current = self
            .get_by_id(&entity.id)
            .await?
            .ok_or_else(|| anyhow!("Could not find Tag {}", entity.id))?;

        let group = self.group_for(entity).await?;
        if entity.base.tag_group_id == current.base.tag_group_id {
            entity.base.position = current.base.position;
        } else {
            entity.base.position = None;
            if let Some(group) = &group {
                self.refuse_group_already_doubled(entity, group).await?;
                self.tag_group_service
                    .refuse_tag_out_of_scope(entity, group)
                    .await?;
                entity.base.position = Some(self.tag_group_service.next_position(group.id).await?);
            }
        }

        let updated = SnapshotMutator::close_and_clone(self, entity.clone()).await?;

        let trigger_stale = updated.triggers_staleness(Some(current));

        if let Some(scope) = EntityScope::from_ids(
            updated.id(),
            updated.clone().into(),
            self.get_site_id(&updated),
            self.get_organization_id(&updated),
        ) {
            self.event_bus()
                .publish(
                    Event::new(scope, EntityOperation::Updated, authentication).with_flags(
                        EntityEventFlags {
                            trigger_stale,
                            ..Default::default()
                        },
                    ),
                )
                .await?;
        }

        Ok(updated)
    }

    /// Tag delete is a soft-close, not a hard delete: setting `valid_to =
    /// NOW()` on the live row preserves FK integrity for any historical
    /// `entity_tags` row that references this tag. Hard-delete would break
    /// snapshot reads of associations that existed before the delete.
    async fn delete(
        &self,
        id: &Uuid,
        authentication: AuthenticatedEntity,
    ) -> Result<(), anyhow::Error> {
        let mut entity = self
            .get_by_id(id)
            .await?
            .ok_or_else(|| anyhow!("Tag {} not found", id))?;

        if entity.valid_to.is_some() {
            // Already soft-closed; nothing to do.
            return Ok(());
        }

        entity.valid_to = Some(chrono::Utc::now());
        let _ = self.storage().update(&mut entity).await?;

        let trigger_stale = entity.triggers_staleness(None);
        let entity_for_event = entity.clone();

        if let Some(scope) = EntityScope::from_ids(
            entity_for_event.id(),
            entity_for_event.into(),
            self.get_site_id(&entity),
            self.get_organization_id(&entity),
        ) {
            self.event_bus()
                .publish(
                    Event::new(scope, EntityOperation::Deleted, authentication).with_flags(
                        EntityEventFlags {
                            trigger_stale,
                            ..Default::default()
                        },
                    ),
                )
                .await?;
        }

        Ok(())
    }
}

impl TagService {
    pub fn new(
        storage: Arc<GenericPostgresStorage<Tag>>,
        entity_tags: Arc<EntityTagStorage>,
        tag_group_service: Arc<TagGroupService>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            storage,
            entity_tags,
            tag_group_service,
            event_bus,
        }
    }

    pub fn tag_group_service(&self) -> &Arc<TagGroupService> {
        &self.tag_group_service
    }

    /// The group `tag` names, checked to belong to the tag's organization. An application tag
    /// draws the application icon, so it can't choose its own.
    async fn group_for(&self, tag: &Tag) -> Result<Option<TagGroup>> {
        let Some(group_id) = tag.base.tag_group_id else {
            return Ok(None);
        };
        let group = self
            .tag_group_service
            .get_by_id(&group_id)
            .await?
            .filter(|g| g.base.organization_id == tag.base.organization_id)
            .ok_or_else(|| ValidationError::new(format!("Tag group {group_id} not found")))?;
        if group.base.group_type == TagGroupType::Application && tag.base.icon.is_some() {
            return Err(ValidationError::new(
                "Application tags use the application icon and cannot set their own",
            )
            .into());
        }
        Ok(Some(group))
    }

    /// The id of the organization's Application group, whose tags drive the application view.
    pub async fn application_group_id(&self, organization_id: &Uuid) -> Result<Option<Uuid>> {
        Ok(self
            .tag_group_service
            .system_group(organization_id, TagGroupType::Application)
            .await?
            .map(|g| g.id))
    }

    /// Refuse to move `tag` into `group` while some entity holds both it and another tag of `group`.
    ///
    /// Moving it would leave that entity breaking the rule the group exists to keep, and which of
    /// its tags should go is a person's call. So the change is refused with a count, and nothing
    /// is removed on the person's behalf.
    async fn refuse_group_already_doubled(&self, tag: &Tag, group: &TagGroup) -> Result<()> {
        let siblings = self
            .get_all(StorableFilter::<Tag>::new_from_org_id(&tag.base.organization_id).live())
            .await?;
        let mut group_tag_ids: Vec<Uuid> = siblings
            .iter()
            .filter(|t| t.id != tag.id && t.in_group(Some(group.id)))
            .map(|t| t.id)
            .collect();
        if group_tag_ids.is_empty() {
            return Ok(());
        }
        group_tag_ids.push(tag.id);

        let rows = self.entity_tags.get_live_for_tags(&group_tag_ids).await?;
        match entities_holding_several(&rows, &group_tag_ids) {
            0 => Ok(()),
            count => Err(ValidationError::new(format!(
                "{count} {} hold more than one tag in the \"{}\" group. Remove the extra tags first.",
                if count == 1 { "entity" } else { "entities" },
                group.base.name
            ))
            .into()),
        }
    }
}
