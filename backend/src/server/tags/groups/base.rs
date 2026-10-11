use std::fmt::Display;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{Display as StrumDisplay, EnumIter, EnumString};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::server::shared::{
    entities::{ChangeTriggersTopologyStaleness, EntityDiscriminants},
    types::api::deserialize_empty_string_as_none,
};

/// What kind of tag group this is.
///
/// `Application`, `Status` and `Visibility` are system groups: every organization has exactly one
/// of each, and they can't be renamed, deleted or recreated. Application tags drive the
/// application view. Status tags track where an asset is on its way from planned to
/// decommissioned, on entities that have no status of their own. A host holding a Visibility tag
/// is left out of topology. `Custom` groups are named by the organization.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    Hash,
    ToSchema,
    EnumIter,
    EnumString,
    StrumDisplay,
)]
pub enum TagGroupType {
    Application,
    Status,
    Visibility,
    #[default]
    Custom,
}

impl TagGroupType {
    /// Every system group type, in the order the enum declares them.
    pub fn system() -> impl Iterator<Item = TagGroupType> {
        Self::iter().filter(|t| t.is_system())
    }

    pub fn is_system(&self) -> bool {
        match self {
            TagGroupType::Application | TagGroupType::Status | TagGroupType::Visibility => true,
            TagGroupType::Custom => false,
        }
    }

    /// Whether a new group of this type orders its tags. Applications and visibility have no
    /// sequence.
    pub fn default_ordered(&self) -> bool {
        match self {
            TagGroupType::Application | TagGroupType::Visibility => false,
            TagGroupType::Status | TagGroupType::Custom => true,
        }
    }

    /// Whether this type's scope is computed from entity metadata rather than set by the
    /// organization. A computed scope is never stored: it is read from [`Self::admits`] each time,
    /// so it follows entity types as they gain or lose a status or a place in topology.
    pub fn has_computed_scope(&self) -> bool {
        match self {
            TagGroupType::Status | TagGroupType::Visibility => true,
            TagGroupType::Application | TagGroupType::Custom => false,
        }
    }

    /// The entity types a group of this type applies to, when its scope is computed.
    pub fn computed_entity_types(&self) -> Option<Vec<EntityDiscriminants>> {
        self.has_computed_scope().then(|| {
            EntityDiscriminants::iter()
                .filter(|t| t.is_taggable() && self.admits(*t))
                .collect()
        })
    }

    /// Whether a group of this type can ever apply to `entity_type`, whatever its scope says. A
    /// status tag would compete with the status an entity already carries, and Visibility only
    /// means something on an entity a topology draws.
    pub fn admits(&self, entity_type: EntityDiscriminants) -> bool {
        match self {
            TagGroupType::Status => !entity_type.has_status(),
            TagGroupType::Visibility => entity_type.drawn_in_topology(),
            TagGroupType::Application | TagGroupType::Custom => true,
        }
    }

    /// The description a new group of this type starts with. The migration seeds the same text.
    pub fn default_description(&self) -> Option<&'static str> {
        match self {
            TagGroupType::Application => Some(
                "What each entity is part of. A service inherits its host's application tag, and \
                 the Application view groups by it.",
            ),
            TagGroupType::Status => Some("Where an asset is in its lifecycle."),
            TagGroupType::Visibility => {
                Some("Entities holding a tag of this group are left out of every topology.")
            }
            TagGroupType::Custom => None,
        }
    }

    /// The system group a name refers to, ignoring case.
    pub fn system_named(name: &str) -> Option<TagGroupType> {
        Self::system().find(|t| t.to_string().eq_ignore_ascii_case(name.trim()))
    }
}

/// A limited group names at least one entity type, and only taggable ones. (`None`, every type, is
/// never passed here.)
fn default_ordered() -> bool {
    true
}

fn validate_entity_types(
    entity_types: &Vec<EntityDiscriminants>,
) -> Result<(), validator::ValidationError> {
    if entity_types.is_empty() {
        return Err(validator::ValidationError::new("tag_group_entity_types")
            .with_message("Choose at least one entity type, or leave the group unlimited".into()));
    }
    if let Some(entity_type) = entity_types.iter().find(|t| !t.is_taggable()) {
        return Err(validator::ValidationError::new("tag_group_entity_types")
            .with_message(format!("{entity_type} can't be tagged").into()));
    }
    Ok(())
}

fn validate_tag_group_base(base: &TagGroupBase) -> Result<(), validator::ValidationError> {
    if base.group_type == TagGroupType::Custom && TagGroupType::system_named(&base.name).is_some() {
        return Err(validator::ValidationError::new("tag_group_reserved")
            .with_message(format!("\"{}\" is a system tag group name", base.name).into()));
    }
    if let Some(entity_type) = base
        .entity_types
        .iter()
        .flatten()
        .find(|t| !base.group_type.admits(**t))
    {
        return Err(
            validator::ValidationError::new("tag_group_entity_types").with_message(
                format!(
                    "A {} tag group can't apply to {entity_type}",
                    base.group_type
                )
                .into(),
            ),
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Validate, Serialize, Deserialize, Eq, PartialEq, Hash, ToSchema)]
#[validate(schema(function = "validate_tag_group_base"))]
pub struct TagGroupBase {
    /// The group's name. A system group's name is fixed.
    #[validate(length(
        min = 1,
        max = 100,
        message = "Tag group name must be between 1 and 100 characters"
    ))]
    pub name: String,
    /// What the group is for.
    #[serde(default, deserialize_with = "deserialize_empty_string_as_none")]
    #[schema(required)]
    #[validate(length(max = 500, message = "Description must be 500 characters or fewer"))]
    pub description: Option<String>,
    /// The organization that owns this record.
    pub organization_id: Uuid,
    /// Whether this is a system group or one the organization named. Set by the server: groups
    /// created through the API are always `Custom`.
    #[serde(default)]
    #[schema(read_only, required)]
    pub group_type: TagGroupType,
    /// The entity types this group's tags can be applied to. `null` allows every taggable type the
    /// group type admits: Status leaves out types that carry a status of their own. Fixed for
    /// Visibility, which applies to hosts.
    #[serde(default)]
    #[schema(required)]
    #[validate(custom(function = "validate_entity_types"))]
    pub entity_types: Option<Vec<EntityDiscriminants>>,
    /// Whether the group's tags have an order the organization sets. An unordered group lists its
    /// tags by name. Fixed for system groups: statuses are ordered, applications are not.
    #[serde(default = "default_ordered")]
    #[schema(required)]
    pub ordered: bool,
    /// Whether `entity_types` is computed from entity metadata (Status, Visibility) rather than set
    /// by the organization. A computed scope is read-only and never stored.
    #[serde(default)]
    #[schema(read_only, required)]
    pub scope_computed: bool,
}

impl TagGroupBase {
    pub fn new(organization_id: Uuid, name: String, group_type: TagGroupType) -> Self {
        Self {
            name,
            description: group_type.default_description().map(str::to_string),
            organization_id,
            group_type,
            entity_types: group_type.computed_entity_types(),
            ordered: group_type.default_ordered(),
            scope_computed: group_type.has_computed_scope(),
        }
    }

    /// Whether this group's tags may be applied to `entity_type`.
    pub fn allows(&self, entity_type: EntityDiscriminants) -> bool {
        self.group_type.admits(entity_type)
            && self
                .entity_types
                .as_ref()
                .is_none_or(|types| types.contains(&entity_type))
    }

    /// The taggable entity types this group's tags may be applied to.
    pub fn allowed_types(&self) -> Vec<EntityDiscriminants> {
        EntityDiscriminants::iter()
            .filter(|t| t.is_taggable() && self.allows(*t))
            .collect()
    }

    /// The entity types `self` allows and `next` no longer does.
    pub fn types_removed_by(&self, next: &TagGroupBase) -> Vec<EntityDiscriminants> {
        self.allowed_types()
            .into_iter()
            .filter(|t| !next.allows(*t))
            .collect()
    }
}

impl Default for TagGroupBase {
    fn default() -> Self {
        Self::new(Uuid::nil(), "New Group".to_string(), TagGroupType::Custom)
    }
}

/// A named set of tags of which an entity holds at most one, in an order the organization sets,
/// optionally limited to some entity types.
#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash, Default, ToSchema, Validate,
)]
#[schema(example = crate::server::shared::types::examples::tag_group)]
pub struct TagGroup {
    /// Server-assigned unique identifier.
    #[serde(default)]
    #[schema(read_only, required)]
    pub id: Uuid,
    /// When this record was first created.
    #[serde(default)]
    #[schema(read_only, required)]
    pub created_at: DateTime<Utc>,
    /// When this record was last modified.
    #[serde(default)]
    #[schema(read_only, required)]
    pub updated_at: DateTime<Utc>,
    #[serde(flatten)]
    #[validate(nested)]
    pub base: TagGroupBase,
}

impl TagGroup {
    pub fn is_system(&self) -> bool {
        self.base.group_type.is_system()
    }
}

impl ChangeTriggersTopologyStaleness<TagGroup> for TagGroup {
    fn triggers_staleness(&self, _other: Option<TagGroup>) -> bool {
        false
    }
}

impl Display for TagGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tag group {}: {}", self.base.name, self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(entity_types: Option<Vec<EntityDiscriminants>>) -> TagGroupBase {
        TagGroupBase {
            entity_types,
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_scope_and_an_untaggable_type_are_refused() {
        assert!(group(Some(vec![])).validate().is_err());
        assert!(
            group(Some(vec![EntityDiscriminants::Interface]))
                .validate()
                .is_err()
        );
        assert!(group(None).validate().is_ok());
        assert!(
            group(Some(vec![EntityDiscriminants::Host]))
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn a_custom_group_cannot_take_a_system_name() {
        for name in ["Status", " application "] {
            let base = TagGroupBase {
                name: name.to_string(),
                ..Default::default()
            };
            assert!(base.validate().is_err());
        }
        let status = TagGroupBase::new(Uuid::nil(), "Status".into(), TagGroupType::Status);
        assert!(status.validate().is_ok());
    }

    #[test]
    fn status_never_applies_to_a_type_with_its_own_status() {
        let status = TagGroupBase::new(Uuid::nil(), "Status".into(), TagGroupType::Status);
        let allowed = status.allowed_types();
        assert!(!allowed.is_empty());
        assert!(allowed.iter().all(|t| !t.has_status()));
        assert!(EntityDiscriminants::iter().any(|t| t.is_taggable() && t.has_status()));

        let with_daemon = TagGroupBase {
            entity_types: Some(vec![EntityDiscriminants::Host, EntityDiscriminants::Daemon]),
            ..status.clone()
        };
        assert!(with_daemon.validate().is_err());
        // A custom group may hold the same types.
        assert!(
            TagGroupBase {
                group_type: TagGroupType::Custom,
                name: "Owner".into(),
                ..with_daemon
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn visibility_covers_exactly_the_taggable_types_a_topology_draws() {
        let visibility =
            TagGroupBase::new(Uuid::nil(), "Visibility".into(), TagGroupType::Visibility);
        let drawn = crate::server::topology::types::views::TopologyView::drawn_entities();
        let expected: Vec<EntityDiscriminants> = EntityDiscriminants::iter()
            .filter(|t| t.is_taggable() && drawn.contains(t))
            .collect();
        assert!(!expected.is_empty());
        assert_eq!(
            visibility.entity_types.as_deref(),
            Some(expected.as_slice())
        );
        assert_eq!(visibility.allowed_types(), expected);
    }

    #[test]
    fn narrowing_names_exactly_the_types_dropped() {
        let unlimited = group(None);
        let hosts_and_services = group(Some(vec![
            EntityDiscriminants::Host,
            EntityDiscriminants::Service,
        ]));
        let hosts = group(Some(vec![EntityDiscriminants::Host]));

        assert_eq!(
            hosts_and_services.types_removed_by(&hosts),
            vec![EntityDiscriminants::Service]
        );
        assert!(hosts.types_removed_by(&hosts_and_services).is_empty());
        assert!(hosts.types_removed_by(&unlimited).is_empty());

        let dropped = unlimited.types_removed_by(&hosts);
        assert!(!dropped.contains(&EntityDiscriminants::Host));
        assert!(dropped.contains(&EntityDiscriminants::Vlan));
        assert!(dropped.iter().all(|t| t.is_taggable()));
    }
}
