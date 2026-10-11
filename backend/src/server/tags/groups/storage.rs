use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::postgres::PgRow;
use strum::IntoEnumIterator;
use uuid::Uuid;

use crate::server::{
    shared::{
        entities::EntityDiscriminants,
        entity_metadata::EntityCategory,
        storage::traits::{Entity, SqlValue, Storable},
    },
    tags::groups::base::{TagGroup, TagGroupBase},
};

impl Storable for TagGroup {
    const HAS_SCD2: bool = false;
    type BaseData = TagGroupBase;

    fn table_name() -> &'static str {
        "tag_groups"
    }

    fn new(base: Self::BaseData) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            base,
        }
    }

    fn get_base(&self) -> Self::BaseData {
        self.base.clone()
    }

    fn to_params(&self) -> Result<(Vec<&'static str>, Vec<SqlValue>), anyhow::Error> {
        let Self {
            id,
            created_at,
            updated_at,
            base:
                Self::BaseData {
                    name,
                    organization_id,
                    group_type,
                    entity_types,
                    ordered,
                },
        } = self.clone();

        Ok((
            vec![
                "id",
                "organization_id",
                "name",
                "group_type",
                "entity_types",
                "ordered",
                "created_at",
                "updated_at",
            ],
            vec![
                SqlValue::Uuid(id),
                SqlValue::Uuid(organization_id),
                SqlValue::String(name),
                SqlValue::String(group_type.to_string()),
                SqlValue::OptionalStringArray(
                    entity_types.map(|types| types.iter().map(|t| t.to_string()).collect()),
                ),
                SqlValue::Bool(ordered),
                SqlValue::Timestamp(created_at),
                SqlValue::Timestamp(updated_at),
            ],
        ))
    }

    fn from_row(row: &PgRow) -> Result<Self, anyhow::Error> {
        let group_type: String = row.get("group_type");
        Ok(TagGroup {
            id: row.get("id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            base: TagGroupBase {
                name: row.get("name"),
                organization_id: row.get("organization_id"),
                group_type: group_type
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Unknown tag group type {group_type}"))?,
                // A type name this build no longer knows is dropped rather than failing the list.
                ordered: row.get("ordered"),
                entity_types: row
                    .get::<Option<Vec<String>>, _>("entity_types")
                    .map(|names| {
                        names
                            .iter()
                            .filter_map(|name| {
                                EntityDiscriminants::iter().find(|t| t.to_string() == *name)
                            })
                            .collect()
                    }),
            },
        })
    }
}

impl Entity for TagGroup {
    fn id(&self) -> Uuid {
        self.id
    }

    fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    fn set_id(&mut self, id: Uuid) {
        self.id = id;
    }

    fn set_created_at(&mut self, time: DateTime<Utc>) {
        self.created_at = time;
    }

    fn entity_type() -> EntityDiscriminants {
        EntityDiscriminants::TagGroup
    }

    const ENTITY_NAME_SINGULAR: &'static str = "Tag Group";
    const ENTITY_NAME_PLURAL: &'static str = "Tag Groups";
    const ENTITY_DESCRIPTION: &'static str = "Sets of tags an entity holds one of, in a set order, optionally limited to some entity types.";

    fn entity_category() -> EntityCategory {
        EntityCategory::Platform
    }

    fn site_id(&self) -> Option<Uuid> {
        None
    }

    fn organization_id(&self) -> Option<Uuid> {
        Some(self.base.organization_id)
    }

    fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    fn set_updated_at(&mut self, time: DateTime<Utc>) {
        self.updated_at = time;
    }
}
