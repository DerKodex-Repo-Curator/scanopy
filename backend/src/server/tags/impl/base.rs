use std::fmt::Display;

use crate::server::shared::{
    entities::ChangeTriggersTopologyStaleness,
    types::{Color, Icon, api::deserialize_empty_string_as_none},
};
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use strum::IntoEnumIterator;
use strum_macros::{EnumDiscriminants, EnumIter};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

/// A group of tags of which an entity may hold at most one.
///
/// Assigning a tag from a group replaces whichever tag of the same group the entity already holds.
/// `Application` and `Status` are system groups: every organization has them, and no
/// organization can name a group of its own after them. Application tags drive the application
/// view, which can place an entity in one application only. Status tags track where an asset is
/// on its way from planned to decommissioned. `Named` groups are named by the organization (an
/// environment, a tier) and exist for as long as a tag carries the name.
#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash, ToSchema, EnumDiscriminants,
)]
#[strum_discriminants(derive(EnumIter))]
#[serde(tag = "type", from = "TagGroupInput")]
pub enum TagGroup {
    /// The system group of application tags.
    #[schema(title = "Application")]
    Application,
    /// The system group of an asset's lifecycle status.
    #[schema(title = "Status")]
    Status,
    /// A group the organization named.
    #[schema(title = "Named")]
    Named {
        /// The group's name, shared by every tag in it.
        name: String,
    },
}

impl TagGroup {
    /// Every system group, in the order the enum declares them.
    pub fn system_groups() -> impl Iterator<Item = TagGroup> {
        TagGroupDiscriminants::iter().filter_map(|group| match group {
            TagGroupDiscriminants::Application => Some(TagGroup::Application),
            TagGroupDiscriminants::Status => Some(TagGroup::Status),
            TagGroupDiscriminants::Named => None,
        })
    }

    /// The system group a name refers to, ignoring case.
    pub fn system_group_named(name: &str) -> Option<TagGroup> {
        Self::system_groups().find(|group| group.to_string().eq_ignore_ascii_case(name))
    }

    /// The group stored as `name` in `tags.tag_group`. A system group is stored as its own name,
    /// which no named group may take.
    pub fn from_stored_name(name: String) -> TagGroup {
        Self::system_groups()
            .find(|group| group.to_string() == name)
            .unwrap_or(TagGroup::Named { name })
    }
}

impl Display for TagGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Application => write!(f, "Application"),
            Self::Status => write!(f, "Status"),
            Self::Named { name } => write!(f, "{name}"),
        }
    }
}

/// The wire shape of [`TagGroup`]. A named group given a system group's name, in any case, reads
/// as that system group, so clients written before the group became a system one keep working.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum TagGroupInput {
    Application,
    Status,
    Named { name: String },
}

impl From<TagGroupInput> for TagGroup {
    fn from(input: TagGroupInput) -> Self {
        match input {
            TagGroupInput::Application => TagGroup::Application,
            TagGroupInput::Status => TagGroup::Status,
            TagGroupInput::Named { name } => {
                let name = name.trim().to_string();
                TagGroup::system_group_named(&name).unwrap_or(TagGroup::Named { name })
            }
        }
    }
}

fn validate_tag_group(group: &TagGroup) -> Result<(), validator::ValidationError> {
    match group {
        TagGroup::Named { name } if name.is_empty() || name.chars().count() > 100 => {
            Err(validator::ValidationError::new("tag_group_name")
                .with_message("Tag group name must be between 1 and 100 characters".into()))
        }
        TagGroup::Named { name } if TagGroup::system_group_named(name).is_some() => {
            Err(validator::ValidationError::new("tag_group_reserved")
                .with_message(format!("\"{name}\" is a system tag group name").into()))
        }
        _ => Ok(()),
    }
}

/// A lucide icon a person chose for a tag.
///
/// A newtype because `lucide_icons::Icon` derives neither `Eq` nor `Hash`, which every entity
/// base needs. The icon is a fieldless enum, so its discriminant is a complete identity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String)]
pub struct TagIcon(pub Icon);

impl TagIcon {
    /// Every icon in the linked lucide build, ordered by name.
    ///
    /// The crate lists its icons only as font glyphs, which all sit in the Private Use Area, so
    /// walking that block finds each one exactly once.
    pub fn all() -> impl Iterator<Item = TagIcon> {
        let mut icons: Vec<TagIcon> = ('\u{e000}'..='\u{f8ff}')
            .filter_map(|glyph| Icon::try_from(glyph).ok().map(TagIcon))
            .collect();
        icons.sort_by_cached_key(|icon| icon.to_string());
        icons.into_iter()
    }
}

impl PartialEq for TagIcon {
    fn eq(&self, other: &Self) -> bool {
        self.0 as u32 == other.0 as u32
    }
}

impl Eq for TagIcon {}

impl std::hash::Hash for TagIcon {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.0 as u32).hash(state);
    }
}

impl Display for TagIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for TagIcon {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Icon::try_from(s)
            .map(TagIcon)
            .map_err(|_| anyhow::anyhow!("Unknown icon {s}"))
    }
}

fn validate_tag_base(base: &TagBase) -> Result<(), validator::ValidationError> {
    if base.is_application() && base.icon.is_some() {
        return Err(
            validator::ValidationError::new("application_tag_icon").with_message(
                "Application tags use the application icon and cannot set their own".into(),
            ),
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Validate, Serialize, Deserialize, Eq, PartialEq, Hash, ToSchema)]
#[validate(schema(function = "validate_tag_base"))]
pub struct TagBase {
    /// Human-facing name for this tag.
    #[validate(length(
        min = 1,
        max = 100,
        message = "Tag name must be between 1 and 100 characters"
    ))]
    pub name: String,
    /// Free-text notes about the tag.
    #[serde(deserialize_with = "deserialize_empty_string_as_none")]
    pub description: Option<String>,
    /// Colour the tag is drawn in.
    pub color: Color,
    /// The organization that owns this record.
    pub organization_id: Uuid,
    /// The group this tag belongs to, if any. An entity holds at most one tag of a group.
    #[serde(default)]
    #[validate(custom(function = "validate_tag_group"))]
    pub tag_group: Option<TagGroup>,
    /// Icon drawn on the tag. Application tags always use the application icon.
    #[serde(default)]
    pub icon: Option<TagIcon>,
}

impl TagBase {
    /// Whether this tag groups an application, so it drives the application view.
    pub fn is_application(&self) -> bool {
        matches!(self.tag_group, Some(TagGroup::Application))
    }
}

impl Default for TagBase {
    fn default() -> Self {
        Self {
            name: "New Tag".to_string(),
            description: None,
            color: Color::Yellow,
            organization_id: Uuid::nil(),
            tag_group: None,
            icon: None,
        }
    }
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash, Default, ToSchema, Validate,
)]
#[schema(example = crate::server::shared::types::examples::tag)]
pub struct Tag {
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
    /// Start of the interval this revision was current for (SCD2 history).
    #[serde(default)]
    #[schema(read_only)]
    pub valid_from: DateTime<Utc>,
    /// End of the interval this revision was current for. `null` while it is the live revision.
    #[serde(default)]
    #[schema(read_only)]
    pub valid_to: Option<DateTime<Utc>>,
    /// Stable identifier shared by every revision of the same entity across its history.
    #[serde(default)]
    #[schema(read_only)]
    pub lineage_id: Option<Uuid>,
    #[serde(flatten)]
    #[validate(nested)]
    pub base: TagBase,
}

impl Tag {
    /// Whether this tag groups an application, so it drives the application view.
    pub fn is_application(&self) -> bool {
        self.base.is_application()
    }
}

impl ChangeTriggersTopologyStaleness<Tag> for Tag {
    /// Always returns true — the actual determination of whether a Tag change
    /// should mark a topology stale lives downstream in the topology subscriber,
    /// which has the cross-service access needed to check if the tag is an
    /// application tag or is referenced by a ByTag element rule.
    /// See `TopologyService::tag_affects_any_topology`.
    fn triggers_staleness(&self, _other: Option<Tag>) -> bool {
        true
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tag {}: {}", self.base.name, self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(tag_group: Option<TagGroup>, icon: Option<Icon>) -> TagBase {
        TagBase {
            tag_group,
            icon: icon.map(TagIcon),
            ..Default::default()
        }
    }

    #[test]
    fn an_application_tag_cannot_choose_its_own_icon() {
        assert!(
            base(Some(TagGroup::Application), Some(Icon::Rocket))
                .validate()
                .is_err()
        );
        assert!(base(Some(TagGroup::Application), None).validate().is_ok());
        assert!(
            base(Some(TagGroup::Status), Some(Icon::Rocket))
                .validate()
                .is_ok()
        );
    }

    fn group_from_json(group: serde_json::Value) -> Option<TagGroup> {
        serde_json::from_value::<TagBase>(serde_json::json!({
            "name": "Old printer",
            "description": null,
            "color": "Gray",
            "organization_id": Uuid::nil(),
            "tag_group": group,
        }))
        .unwrap()
        .tag_group
    }

    #[test]
    fn a_group_name_is_trimmed_and_a_blank_one_refused() {
        assert_eq!(
            group_from_json(serde_json::json!({ "type": "Named", "name": "  Lifecycle " })),
            Some(TagGroup::Named {
                name: "Lifecycle".to_string()
            })
        );

        let blank = base(
            Some(TagGroup::Named {
                name: String::new(),
            }),
            None,
        );
        assert!(blank.validate().is_err());
    }

    /// Clients that predate the system Status group send it as a named group.
    #[test]
    fn a_named_group_with_a_system_name_reads_as_the_system_group() {
        assert_eq!(
            group_from_json(serde_json::json!({ "type": "Named", "name": " status" })),
            Some(TagGroup::Status)
        );
        assert_eq!(
            group_from_json(serde_json::json!({ "type": "Named", "name": "APPLICATION" })),
            Some(TagGroup::Application)
        );
        assert_eq!(
            group_from_json(serde_json::json!({ "type": "Status" })),
            Some(TagGroup::Status)
        );
    }

    #[test]
    fn a_named_group_cannot_take_a_system_name() {
        for name in ["Status", "application"] {
            let group = Some(TagGroup::Named {
                name: name.to_string(),
            });
            assert!(base(group, None).validate().is_err());
        }
    }

    /// `tags.tag_group` stores a system group as its name, so each must read back as itself.
    #[test]
    fn every_system_group_reads_back_from_its_stored_name() {
        for group in TagGroup::system_groups() {
            assert_eq!(TagGroup::from_stored_name(group.to_string()), group);
        }
    }

    #[test]
    fn an_unknown_icon_name_is_refused() {
        assert!(serde_json::from_value::<TagIcon>(serde_json::json!("not-an-icon")).is_err());
        assert!(serde_json::from_value::<TagIcon>(serde_json::json!("archive-x")).is_ok());
    }

    /// The picker offers exactly what `all` lists, so every entry must round-trip through the
    /// API's name for it, and none may repeat.
    #[test]
    fn every_listed_icon_round_trips_through_its_name() {
        let icons: Vec<TagIcon> = TagIcon::all().collect();
        assert!(icons.len() > 1000, "expected the full lucide set");
        let names: std::collections::HashSet<String> =
            icons.iter().map(|icon| icon.to_string()).collect();
        assert_eq!(names.len(), icons.len());
        for icon in icons {
            assert_eq!(icon.to_string().parse::<TagIcon>().unwrap(), icon);
        }
    }
}
