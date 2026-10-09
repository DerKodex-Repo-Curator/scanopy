//! What each entity type is described by: the registry CSV export, the list tables, the topology
//! inspector and the diagram exports all read.
//!
//! A field names a value on the entity's serialized JSON, or on entities reached from it through
//! `path`. The registry is declared per type in `shared/entity_fields.rs` and travels to the
//! frontend inside the `entities` and `edge_types` fixtures, so every consumer resolves the same
//! fields the same way instead of each keeping its own list.

use crate::server::shared::entities::EntityDiscriminants;
use serde::Serialize;
use utoipa::ToSchema;

/// One step from an object to related objects.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, ToSchema)]
#[serde(tag = "type")]
pub enum Hop {
    /// The entities named by `key` on the current object: one id, or an array of ids.
    Parent {
        entity: EntityDiscriminants,
        key: &'static str,
    },
    /// Every entity of type `entity` whose `key` holds the current object's id.
    Children {
        entity: EntityDiscriminants,
        key: &'static str,
    },
    /// The array of objects held on the current object under `key`, such as a service's bindings.
    Embedded { key: &'static str },
}

impl Hop {
    pub const fn parent(entity: EntityDiscriminants, key: &'static str) -> Self {
        Hop::Parent { entity, key }
    }

    pub const fn children(entity: EntityDiscriminants, key: &'static str) -> Self {
        Hop::Children { entity, key }
    }

    pub const fn embedded(key: &'static str) -> Self {
        Hop::Embedded { key }
    }
}

/// A metadata fixture whose entries name an enum-valued field, so the UI shows the fixture's
/// name for the raw value. Serializes to the fixture key.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, ToSchema, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum MetadataFixture {
    IfOperStatuses,
    SubnetTypes,
    EntitySources,
    DependencyTypes,
    DiscoveryTypes,
    ServiceDefinitions,
    DiscoveryProtocols,
    Permissions,
    CredentialTypes,
}

/// How a field's value is written. Chosen by the value's shape, never by where it is shown.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, ToSchema)]
#[serde(tag = "type")]
pub enum FieldFormat {
    Text,
    Number,
    Boolean,
    Timestamp,
    /// A value read as a code rather than prose (MAC, CIDR, chassis id): monospace, copyable.
    Identifier,
    /// `speed_bps`, written as 1G / 100M.
    BitsPerSecond,
    /// A `{number, protocol}` port, written as 80/tcp.
    Port,
    /// A host `os` object, written as its name and version.
    OperatingSystem,
    /// An enum value, named through a metadata fixture.
    Metadata {
        fixture: MetadataFixture,
    },
    /// A related entity. The UI links it; CSV writes its id; text-only outputs write its display
    /// field.
    Entity,
}

/// One fact that describes an entity, possibly read from a related entity.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, ToSchema)]
pub struct EntityField {
    /// Stable key: the CSV header, the list-table column id and the diagram Shape Data name.
    pub id: &'static str,
    /// English label; the UI translates it through a generated `meta_*` key.
    pub label: &'static str,
    /// Steps from the entity to the objects the value is read from. Empty for the entity itself.
    pub path: &'static [Hop],
    /// Dotted path to the value on the last object, or `None` for the object itself (an
    /// `Entity` or `Port` field).
    pub field: Option<&'static str>,
    pub format: FieldFormat,
    /// Sibling field holding the value's attribution, for attributed host facts.
    pub source_field: Option<&'static str>,
    /// Shown by default in tables and in the inspector's details. Every field is exported.
    pub summary: bool,
    /// Whether one entity can hold several values: set by any `Children` or `Embedded` hop, and
    /// declared with [`EntityField::many`] for a key holding an array of ids or values.
    pub many: bool,
}

/// Whether a path can reach several objects from one.
const fn path_is_many(path: &[Hop]) -> bool {
    let mut i = 0;
    while i < path.len() {
        if matches!(path[i], Hop::Children { .. } | Hop::Embedded { .. }) {
            return true;
        }
        i += 1;
    }
    false
}

impl EntityField {
    /// A text field read from `id` on the entity itself.
    pub const fn new(id: &'static str, label: &'static str) -> Self {
        EntityField {
            id,
            label,
            path: &[],
            field: Some(id),
            format: FieldFormat::Text,
            source_field: None,
            summary: false,
            many: false,
        }
    }

    /// Read the value from `field` instead of the field named by `id`.
    pub const fn field(self, field: &'static str) -> Self {
        EntityField {
            field: Some(field),
            ..self
        }
    }

    pub const fn format(self, format: FieldFormat) -> Self {
        EntityField { format, ..self }
    }

    /// The related entities themselves, reached through `path`.
    pub const fn entity(self, path: &'static [Hop]) -> Self {
        EntityField {
            path,
            field: None,
            format: FieldFormat::Entity,
            many: path_is_many(path),
            ..self
        }
    }

    /// Read `field` on the objects reached through `path`.
    pub const fn via(self, path: &'static [Hop], field: Option<&'static str>) -> Self {
        EntityField {
            path,
            field,
            many: path_is_many(path),
            ..self
        }
    }

    pub const fn source(self, source_field: &'static str) -> Self {
        EntityField {
            source_field: Some(source_field),
            ..self
        }
    }

    pub const fn summary(self) -> Self {
        EntityField {
            summary: true,
            ..self
        }
    }

    pub const fn metadata(self, fixture: MetadataFixture) -> Self {
        self.format(FieldFormat::Metadata { fixture })
    }

    /// The value is an array: several ids or values held on one key.
    pub const fn many(self) -> Self {
        EntityField { many: true, ..self }
    }
}
