//! Generic ordering support for entity queries.
//!
//! Provides the `OrderField` trait and `apply_ordering` function to eliminate
//! duplicated ordering logic across entity handlers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::shared::{
    entities::EntityDiscriminants,
    storage::{filter::StorableFilter, traits::Storable},
};

use super::query::OrderDirection;

// ============================================================================
// OrderField Trait
// ============================================================================

/// Trait for order field enums that generate SQL ORDER BY expressions.
///
/// Implement this for entity-specific OrderField enums to enable generic
/// ordering via `apply_ordering()`.
pub trait OrderField: Clone + Copy + Default + Send + Sync + 'static {
    /// Returns the SQL ORDER BY expression for this field.
    ///
    /// The expression should be fully qualified with table name or alias.
    /// Examples: `"hosts.created_at"`, `"COALESCE(virt_service.name, '')"`
    fn to_sql(&self) -> &'static str;

    /// Returns the JOIN clause if this field requires one, None otherwise.
    ///
    /// Example: `"LEFT JOIN services AS virt_service ON ..."`
    fn join_sql(&self) -> Option<&'static str> {
        None
    }

    /// Whether rows with no value for this field sort last in both directions.
    ///
    /// Postgres puts NULLs last for ASC but first for DESC. A field whose NULL means "nothing to
    /// sort by" (e.g. a host with no MAC) returns true so those rows never lead a DESC listing.
    fn nulls_last(&self) -> bool {
        false
    }

    /// The expression rows are grouped on when the list is grouped by this field, and the key the
    /// response's `group_counts` carry. Defaults to [`Self::to_sql`].
    ///
    /// Separate because a field can group on something other than what it sorts and filters on:
    /// "Virtualized By" sorts, and lists filter options, by the virtualizing service's name, but
    /// groups each host under the top of its virtualization chain.
    fn group_sql(&self) -> &'static str {
        self.to_sql()
    }

    /// The JOIN [`Self::group_sql`] reads from. Defaults to [`Self::join_sql`].
    fn group_join_sql(&self) -> Option<&'static str> {
        self.join_sql()
    }

    /// The order inside each group when the list is grouped by this field, if the field
    /// prescribes one. It takes precedence over the user's sort, which still breaks its ties.
    ///
    /// For a group that is a tree: the rows have to arrive parent first for the indentation to
    /// mean anything, and a paginated list can't reorder rows it hasn't received.
    fn group_order_sql(&self) -> Option<&'static str> {
        None
    }

    /// The field's ORDER BY term in the given direction.
    fn order_term(&self, dir: &str) -> String {
        if self.nulls_last() {
            format!("{} {} NULLS LAST", self.to_sql(), dir)
        } else {
            format!("{} {}", self.to_sql(), dir)
        }
    }

    /// The ORDER BY term that keeps this field's groups together.
    fn group_term(&self) -> String {
        if self.nulls_last() {
            format!("{} ASC NULLS LAST", self.group_sql())
        } else {
            format!("{} ASC", self.group_sql())
        }
    }
}

/// The order field of an entity with no server-side ordering.
///
/// Its one variant is the order the generic `get_all_handler` already lists by, unqualified
/// because that handler's query carries no JOIN.
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, Default, utoipa::ToSchema, strum::EnumIter,
)]
#[serde(rename_all = "snake_case")]
pub enum NoOrderField {
    #[default]
    CreatedAt,
}

impl OrderField for NoOrderField {
    fn to_sql(&self) -> &'static str {
        match self {
            Self::CreatedAt => "created_at",
        }
    }
}

// ============================================================================
// Generic apply_ordering Function
// ============================================================================

/// Apply ordering to a filter based on group_by, order_by, and direction.
///
/// This function handles:
/// - Adding JOINs required by order fields
/// - Avoiding duplicate JOINs when group_by and order_by use the same JOIN
/// - Building the ORDER BY clause with group_by first (always ASC) then order_by
/// - Ending a field ordering with the row id, so rows that tie on the field (every NULL, say)
///   keep one order across pages instead of repeating on one page and vanishing from the next
///
/// Returns: (modified_filter, order_by_sql)
pub fn apply_ordering<T, O>(
    group_by: Option<O>,
    order_by: Option<O>,
    direction: Option<OrderDirection>,
    mut filter: StorableFilter<T>,
    default_order: &str,
) -> (StorableFilter<T>, String)
where
    T: Storable,
    O: OrderField,
{
    let mut order_parts = Vec::new();

    // Primary: group_by field (always ASC to keep groups together)
    if let Some(group_field) = group_by {
        if let Some(join) = group_field.group_join_sql() {
            filter = filter.join(join);
        }
        order_parts.push(group_field.group_term());
        if let Some(within_group) = group_field.group_order_sql() {
            order_parts.push(format!("{within_group} ASC"));
        }
    }

    // Secondary: order_by field with specified direction
    if let Some(order_field) = order_by {
        // Only add JOIN if not already added by group_by
        let group_join = group_by.and_then(|g| g.group_join_sql());
        let order_join = order_field.join_sql();
        if let Some(join) = order_join
            && group_join != order_join
        {
            filter = filter.join(join);
        }
        let dir = direction.unwrap_or_default().to_sql();
        order_parts.push(order_field.order_term(dir));
    }

    let order_by_sql = if order_parts.is_empty() {
        default_order.to_string()
    } else {
        order_parts.push(format!("{}.id ASC", T::table_name()));
        order_parts.join(", ")
    };

    (filter, order_by_sql)
}

// ============================================================================
// Ordering by a tag group
// ============================================================================

/// Sorting or grouping a server-paged list by the tag each row holds from one tag group.
///
/// A tag group is data, not a column, so it can't be an [`OrderField`] variant: the group's id
/// travels as its own query parameter. The id is a parsed [`Uuid`] and the entity type an enum, so
/// both are written into the JOIN as literals.
#[derive(Debug, Clone, Copy, Default)]
pub struct TagGroupOrdering {
    /// Group rows by the tag they hold from this group, in the group's tag order.
    pub group_by: Option<Uuid>,
    /// Sort rows by the position of the tag they hold from this group.
    pub order_by: Option<Uuid>,
}

impl TagGroupOrdering {
    /// The expression a list grouped by a tag group is grouped on, and the key of each group in
    /// the response's `group_counts`: `<position padded to 10 digits>:<tag id>`. Leading with the
    /// position keeps groups in the tag group's order; rows holding no tag of the group have no
    /// key and come last.
    pub const GROUP_SQL: &'static str = "tag_group_grouped.group_key";

    pub fn is_set(&self) -> bool {
        self.group_by.is_some() || self.order_by.is_some()
    }

    /// The JOIN exposing, as `alias`, the position and group key of the tag each row of `T`
    /// holds from `group_id`.
    fn join<T: Storable>(alias: &str, entity_type: EntityDiscriminants, group_id: Uuid) -> String {
        let table = T::table_name();
        // `entity_tags.entity_type` holds the JSON-encoded discriminant, as `SqlValue` binds it.
        let entity_type = serde_json::to_string(&entity_type).unwrap_or_default();
        format!(
            "LEFT JOIN LATERAL (SELECT t.position, \
             lpad(t.position::text, 10, '0') || ':' || t.id::text AS group_key \
             FROM entity_tags et JOIN tags t ON t.id = et.tag_id \
             WHERE et.entity_id = {table}.id AND et.entity_type = '{entity_type}' \
             AND et.valid_to IS NULL AND et.snapshot_id IS NULL \
             AND t.tag_group_id = '{group_id}' \
             ORDER BY t.position LIMIT 1) AS {alias} ON true"
        )
    }

    /// [`apply_ordering`], with this tag-group grouping or sort in place of `group_by` or
    /// `order_by` where set.
    pub fn apply<T, O>(
        &self,
        entity_type: EntityDiscriminants,
        group_by: Option<O>,
        order_by: Option<O>,
        direction: Option<OrderDirection>,
        filter: StorableFilter<T>,
        default_order: &str,
    ) -> (StorableFilter<T>, String)
    where
        T: Storable,
        O: OrderField,
    {
        if !self.is_set() {
            return apply_ordering(group_by, order_by, direction, filter, default_order);
        }
        // Fields this ordering replaces are dropped before the field ordering is built.
        let group_by = group_by.filter(|_| self.group_by.is_none());
        let order_by = order_by.filter(|_| self.order_by.is_none());
        let (mut filter, field_order) =
            apply_ordering(group_by, order_by, direction, filter, default_order);

        let mut parts = Vec::new();
        if let Some(group_id) = self.group_by {
            filter = filter.join(&Self::join::<T>(
                "tag_group_grouped",
                entity_type,
                group_id,
            ));
            parts.push(format!("{} ASC", Self::GROUP_SQL));
        }
        // A field ordering ends with the row id; a tag-group sort goes in before it. With no
        // field left, `apply_ordering` returned the default order, which this replaces.
        let id_term = format!("{}.id ASC", T::table_name());
        if group_by.is_some() || order_by.is_some() {
            let head = field_order
                .strip_suffix(&format!(", {id_term}"))
                .unwrap_or(&field_order);
            parts.push(head.to_string());
        }
        if let Some(group_id) = self.order_by {
            filter = filter.join(&Self::join::<T>("tag_group_sorted", entity_type, group_id));
            let dir = direction.unwrap_or_default().to_sql();
            parts.push(format!("tag_group_sorted.position {dir} NULLS LAST"));
        }
        parts.push(id_term);
        (filter, parts.join(", "))
    }
}

#[cfg(test)]
mod tag_group_ordering_tests {
    use super::*;
    use crate::server::hosts::{handlers::HostOrderField, r#impl::base::Host};

    fn order(ordering: TagGroupOrdering, group_by: Option<HostOrderField>) -> (String, String) {
        let (filter, order) = ordering.apply(
            EntityDiscriminants::Host,
            group_by,
            None::<HostOrderField>,
            Some(OrderDirection::Desc),
            StorableFilter::<Host>::new_from_site_ids(&[]),
            "hosts.created_at ASC",
        );
        (filter.to_join_clause(), order)
    }

    #[test]
    fn unset_leaves_the_field_ordering_alone() {
        let (joins, order) = order(TagGroupOrdering::default(), None);
        assert!(joins.is_empty());
        assert_eq!(order, "hosts.created_at ASC");
    }

    #[test]
    fn grouping_by_a_tag_group_leads_and_the_row_id_still_breaks_ties() {
        let group = Uuid::new_v4();
        let (joins, order) = order(
            TagGroupOrdering {
                group_by: Some(group),
                order_by: Some(group),
            },
            Some(HostOrderField::default()),
        );
        assert!(joins.contains(&group.to_string()));
        assert!(joins.contains("valid_to IS NULL"));
        let terms: Vec<&str> = order.split(", ").collect();
        assert_eq!(terms.first(), Some(&"tag_group_grouped.group_key ASC"));
        assert_eq!(terms.last(), Some(&"hosts.id ASC"));
        assert!(terms.contains(&"tag_group_sorted.position DESC NULLS LAST"));
        // The host field it replaced is gone.
        assert_eq!(terms.len(), 3);
    }
}
