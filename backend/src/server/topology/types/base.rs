use crate::server::organizations::r#impl::base::UseCase;
use crate::server::services::r#impl::categories::ServiceCategory;
use crate::server::shared::entities::{ChangeTriggersTopologyStaleness, EntityDiscriminants};
use crate::server::topology::types::edges::{EdgeHandle, EdgeTypeDiscriminants};
use crate::server::topology::types::grouping::{
    ContainerRule, ElementRule, ElementSort, GraphRule, IdentifiedRule,
};
use crate::server::topology::types::layout::{Ixy, Uxy};
use crate::server::topology::types::views::{
    MetadataFilterType, TopologyView, TopologyViewSupport,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
};
use strum::IntoEnumIterator;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, ToSchema, Validate)]
pub struct Topology {
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
    pub base: TopologyBase,
}

impl Topology {
    /// Resolve the available views for a share, filtering by data availability.
    /// If `configured` is None or empty, all data-supported views are returned.
    /// If `configured` is Some(non-empty list), returns the intersection preserving list order.
    ///
    /// `support` must be computed from raw entity data (see
    /// `TopologyService::get_view_support`) — NOT from this topology's
    /// persisted graph, because the graph is rebuilt per-view and its
    /// edges/entity_tags reflect only the most recently rendered view.
    pub fn resolve_available_views(
        &self,
        configured: &Option<Vec<TopologyView>>,
        support: &TopologyViewSupport,
    ) -> Vec<TopologyView> {
        let data_supported: Vec<TopologyView> = TopologyView::iter()
            .filter(|v| v.is_supported(support))
            .collect();

        match configured {
            None => data_supported,
            Some(list) if list.is_empty() => data_supported,
            Some(list) => list
                .iter()
                .filter(|v| data_supported.contains(v))
                .cloned()
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Validate, Serialize, Deserialize, Eq, PartialEq, Default, ToSchema)]
pub struct TopologyBase {
    /// The site this entity belongs to.
    pub site_id: Uuid,
    /// Saved layout and view settings for this topology.
    pub options: TopologyOptions,
    // The per-view node/edge graph is no longer persisted — it's a pure
    // function of entities + `options` and is built on request by the read
    // path (`build_all_view_graphs`). The row holds only the user's grouping
    // `options`; snapshots build their graph on request from closed copies, so
    // there are no snapshot-pinned topology rows (and no `snapshot_id`).
}

impl TopologyBase {
    pub fn new(site_id: Uuid) -> Self {
        Self {
            site_id,
            options: TopologyOptions::default(),
        }
    }
}

impl ChangeTriggersTopologyStaleness<Topology> for Topology {
    fn triggers_staleness(&self, other: Option<Topology>) -> bool {
        if let Some(other_topology) = other {
            // Switching the active view is a client-side slice selection (all
            // views are pre-built on the row), not an options change — request
            // options no longer carry a view scalar. Grouping/hide-rule edits
            // still trip this.
            self.base.options.request != other_topology.base.options.request
        } else {
            false
        }
    }
}

impl Display for Topology {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Topology {{ id: {} }}", self.id)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, ToSchema)]
pub struct TopologyOptions {
    // `#[serde(default)]` keeps deserialization lenient (missing -> default), but
    // these are always present in responses, so mark them required in the schema.
    /// Settings applied in the viewer, which do not change what the server returns.
    #[serde(default)]
    #[schema(required)]
    pub local: TopologyLocalOptions,
    /// Settings that change how the server builds the graph.
    #[serde(default)]
    #[schema(required)]
    pub request: TopologyRequestOptions,
}

/// Filter settings for hiding entities by tag in topology visualization.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Default, ToSchema)]
#[serde(from = "TagFilterWire")]
pub struct TopologyTagFilter {
    /// Tag IDs to hide, per entity type. An entity of that type carrying one of them is hidden.
    #[serde(default)]
    pub hidden_tag_ids: BTreeMap<EntityDiscriminants, Vec<Uuid>>,
}

/// What a stored tag filter may look like: the per-entity map, or the three per-entity lists
/// written before it. Topology options are JSONB, so rows saved earlier still hold the lists.
#[derive(Deserialize)]
struct TagFilterWire {
    #[serde(default)]
    hidden_tag_ids: BTreeMap<EntityDiscriminants, Vec<Uuid>>,
    #[serde(default)]
    hidden_host_tag_ids: Vec<Uuid>,
    #[serde(default)]
    hidden_service_tag_ids: Vec<Uuid>,
    #[serde(default)]
    hidden_subnet_tag_ids: Vec<Uuid>,
}

impl From<TagFilterWire> for TopologyTagFilter {
    fn from(wire: TagFilterWire) -> Self {
        let mut hidden_tag_ids = wire.hidden_tag_ids;
        for (entity, ids) in [
            (EntityDiscriminants::Host, wire.hidden_host_tag_ids),
            (EntityDiscriminants::Service, wire.hidden_service_tag_ids),
            (EntityDiscriminants::Subnet, wire.hidden_subnet_tag_ids),
        ] {
            if ids.is_empty() {
                continue;
            }
            let entry = hidden_tag_ids.entry(entity).or_default();
            for id in ids {
                if !entry.contains(&id) {
                    entry.push(id);
                }
            }
        }
        Self { hidden_tag_ids }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, ToSchema)]
#[serde(default)]
pub struct TopologyLocalOptions {
    /// Keep unrelated edges at full opacity when something is selected.
    pub no_fade_edges: bool,
    /// Edge types to leave out of the drawing. Unknown values are dropped on read rather than
    /// failing the topology — see [`deserialize_known_edge_types`](crate::server::topology::types::edges::deserialize_known_edge_types).
    #[serde(
        deserialize_with = "crate::server::topology::types::edges::deserialize_known_edge_types"
    )]
    pub hide_edge_types: Vec<EdgeTypeDiscriminants>,
    /// Restrict the view to entities carrying these tags.
    #[serde(default)]
    pub tag_filter: TopologyTagFilter,
    /// Show the minimap.
    #[serde(default = "default_true")]
    pub show_minimap: bool,
    /// Collapse parallel edges between the same pair of nodes into one.
    #[serde(default = "default_true")]
    pub bundle_edges: bool,
}

fn default_true() -> bool {
    true
}

impl Default for TopologyLocalOptions {
    fn default() -> Self {
        Self {
            no_fade_edges: false,
            hide_edge_types: vec![EdgeTypeDiscriminants::Hypervisor],
            tag_filter: TopologyTagFilter::default(),
            show_minimap: true,
            bundle_edges: true,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, ToSchema)]
#[serde(default)]
pub struct TopologyRequestOptions {
    /// Entity types hidden per view. Keyed by TopologyView, values are entity
    /// types (matching those declared as container/element/inline in the
    /// view's element_config). Hides every manifestation of the entity in
    /// that view — element nodes, container nodes, and inline rows on
    /// element cards. Supersedes the old `hide_ports` (L3-only, inline-only).
    #[serde(default)]
    pub hide_entities: HashMap<TopologyView, Vec<EntityDiscriminants>>,
    /// Generic per-(view, entity, filter) hide-set for metadata filters
    /// (Category, Virtualization, etc). Supersedes the old
    /// `hide_service_categories`; nested so JSON keys are strings all the
    /// way down.
    #[serde(default = "default_hide_metadata_values")]
    pub hide_metadata_values: HashMap<
        TopologyView,
        HashMap<EntityDiscriminants, HashMap<MetadataFilterType, Vec<String>>>,
    >,
    /// Rules deciding how nodes are grouped into containers.
    #[serde(default = "default_container_rules")]
    pub container_rules: HashMap<TopologyView, Vec<IdentifiedRule<ContainerRule>>>,
    /// Rules deciding how entities are placed and inlined within containers. A missing field
    /// takes the rules for an organization with no stated use case.
    pub element_rules: Vec<IdentifiedRule<ElementRule>>,
    /// How nodes are ordered inside containers, per view. A view with no entry uses the
    /// layout's own order.
    #[serde(default)]
    pub element_sort: HashMap<TopologyView, ElementSort>,
}

/// The hide-set a topology starts with when the user has expressed no preference.
///
/// Sourced from each view's own `default_hidden_values` so there is exactly one definition of
/// what counts as a product default — the frontend reads the same list out of the generated view
/// fixture.
fn default_hide_metadata_values()
-> HashMap<TopologyView, HashMap<EntityDiscriminants, HashMap<MetadataFilterType, Vec<String>>>> {
    TopologyView::iter()
        .map(|view| (view, view.element_config().default_hidden_values))
        .collect()
}

impl TopologyRequestOptions {
    /// Adopt any default hide-set entry the stored options have no opinion about.
    ///
    /// `#[serde(default = ...)]` is a whole-field fallback: options that already carry a
    /// `hide_metadata_values` map take it verbatim, so a newly added default reaches new installs
    /// only. Every existing user would keep a map written before the filter existed and never be
    /// offered it — which for the L2 link-state filter means keeping a view that renders every
    /// ifTable row, the condition this default exists to fix.
    ///
    /// Merging is per `(view, entity, filter)` key and only where the key is **absent**. A user
    /// who has toggled a filter has an opinion recorded under that key and it is left exactly as
    /// it stands, including an explicit empty list — that is "show everything", not "unset". Only
    /// a filter they have never been offered is filled in.
    pub fn merge_missing_hide_defaults(&mut self) {
        for (view, by_entity) in default_hide_metadata_values() {
            let stored_view = self.hide_metadata_values.entry(view).or_default();
            for (entity, by_filter) in by_entity {
                let stored_entity = stored_view.entry(entity).or_default();
                for (filter_type, values) in by_filter {
                    stored_entity.entry(filter_type).or_insert(values);
                }
            }
        }
    }
}

fn default_container_rules() -> HashMap<TopologyView, Vec<IdentifiedRule<ContainerRule>>> {
    use ContainerRule::*;

    // Build from applicable_views: for each rule type, add it to every view it applies to
    let all_rules: Vec<IdentifiedRule<ContainerRule>> = vec![
        IdentifiedRule::new(BySubnet),
        IdentifiedRule::new(MergeContainerBridges),
        IdentifiedRule::new(ByApplication { tag_ids: vec![] }),
        IdentifiedRule::new(ByHost),
    ];

    let mut map: HashMap<TopologyView, Vec<IdentifiedRule<ContainerRule>>> =
        TopologyView::iter().map(|p| (p, vec![])).collect();

    for gr in all_rules {
        for &view in gr.rule.applicable_views() {
            map.entry(view).or_default().push(gr.clone());
        }
    }

    map
}

/// The element rules a new topology starts with for an organization's use case.
///
/// The infrastructure rule collects every service category that is not application-relevant for
/// `use_case`, so a network appliance is infrastructure unless the organization is an MSP, and an
/// IoT device is infrastructure unless it is a homelab.
pub(crate) fn default_element_rules(use_case: UseCase) -> Vec<IdentifiedRule<ElementRule>> {
    vec![
        IdentifiedRule::new(ElementRule::ByTrunkPort),
        IdentifiedRule::new(ElementRule::ByVLAN),
        IdentifiedRule::new(ElementRule::ByPortOpStatus),
        IdentifiedRule::new(ElementRule::ByServiceCategory {
            categories: infrastructure_categories(use_case),
            title: Some("Infrastructure".into()),
            is_infra_rule: true,
        }),
        IdentifiedRule::new(ElementRule::ByTag {
            tag_ids: vec![],
            title: None,
        }),
        IdentifiedRule::new(ElementRule::ByHypervisor),
        IdentifiedRule::new(ElementRule::ByContainerRuntime),
        IdentifiedRule::new(ElementRule::ByStack),
    ]
}

impl Default for TopologyRequestOptions {
    fn default() -> Self {
        Self {
            hide_entities: HashMap::new(),
            hide_metadata_values: default_hide_metadata_values(),
            container_rules: default_container_rules(),
            element_rules: default_element_rules(UseCase::default()),
            element_sort: HashMap::new(),
        }
    }
}

/// Service categories that are not application-relevant for `use_case`.
fn infrastructure_categories(use_case: UseCase) -> Vec<ServiceCategory> {
    ServiceCategory::iter()
        .filter(|c| !c.application_relevant_use_cases().contains(&use_case))
        .collect()
}

impl TopologyRequestOptions {
    /// The options a new topology starts with for an organization with this use case.
    pub fn for_use_case(use_case: UseCase) -> Self {
        Self {
            element_rules: default_element_rules(use_case),
            ..Self::default()
        }
    }

    /// Give an infrastructure rule stored with no categories the use case's infrastructure set.
    ///
    /// The grouping-rules migration wrote the infrastructure rule with an empty category list and
    /// left the categories to be filled when the topology is read. A rule that has any categories
    /// is the user's and is left alone.
    pub fn populate_empty_infra_rule(&mut self, use_case: UseCase) {
        for identified in &mut self.element_rules {
            if let ElementRule::ByServiceCategory {
                categories,
                is_infra_rule: true,
                ..
            } = &mut identified.rule
                && categories.is_empty()
            {
                *categories = infrastructure_categories(use_case);
            }
        }
    }
}

/// Lightweight request type for updating a single node's position.
///
/// Used for drag operations - instead of sending the entire topology (which can be
/// several megabytes for large networks), only sends the node ID and new position.
/// Fixes HTTP 413 errors on drag operations.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopologyNodePositionUpdate {
    /// Site ID for authorization
    pub site_id: Uuid,
    /// View whose node/edge slice this update targets
    pub view: TopologyView,
    /// ID of the node to update
    pub node_id: Uuid,
    /// New position for the node
    pub position: Ixy,
}

/// Lightweight request type for updating an edge's handles.
///
/// Used for edge reconnect operations - instead of sending the entire topology,
/// only sends the edge ID and new handle positions.
/// Fixes HTTP 413 errors on edge reconnect operations.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopologyEdgeHandleUpdate {
    /// Site ID for authorization
    pub site_id: Uuid,
    /// View whose node/edge slice this update targets
    pub view: TopologyView,
    /// ID of the edge to update
    pub edge_id: Uuid,
    /// New source handle position
    pub source_handle: EdgeHandle,
    /// New target handle position
    pub target_handle: EdgeHandle,
}

/// Lightweight request type for updating a node's size and position.
///
/// Used for subnet resize operations - instead of sending the entire topology,
/// only sends the node ID, new size, and new position.
/// Fixes HTTP 413 errors on resize operations.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopologyNodeResizeUpdate {
    /// Site ID for authorization
    pub site_id: Uuid,
    /// View whose node/edge slice this update targets
    pub view: TopologyView,
    /// ID of the node to update
    pub node_id: Uuid,
    /// New size for the node
    pub size: Uxy,
    /// New position for the node
    pub position: Ixy,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn infra_categories(options: &TopologyRequestOptions) -> &[ServiceCategory] {
        options
            .element_rules
            .iter()
            .find_map(|r| match &r.rule {
                ElementRule::ByServiceCategory {
                    categories,
                    is_infra_rule: true,
                    ..
                } => Some(categories.as_slice()),
                _ => None,
            })
            .expect("default rules carry an infrastructure rule")
    }

    /// An MSP manages network appliances as its product, so they stay out of the infrastructure
    /// group; IoT is not application-relevant to an MSP and goes in.
    #[test]
    fn msp_infra_rule_keeps_network_appliances_out() {
        let options = TopologyRequestOptions::for_use_case(UseCase::Msp);
        let infra = infra_categories(&options);
        assert!(!infra.contains(&ServiceCategory::NetworkAppliance));
        assert!(infra.contains(&ServiceCategory::IoT));
    }

    #[test]
    fn homelab_infra_rule_keeps_iot_out() {
        let options = TopologyRequestOptions::for_use_case(UseCase::Homelab);
        let infra = infra_categories(&options);
        assert!(infra.contains(&ServiceCategory::NetworkAppliance));
        assert!(!infra.contains(&ServiceCategory::IoT));
    }

    /// The migration stored the infrastructure rule with no categories; reading it fills them in
    /// for the org's use case, and a rule the user has filled is left as it is.
    #[test]
    fn empty_infra_rule_is_populated_and_user_rule_is_kept() {
        let mut options = TopologyRequestOptions::for_use_case(UseCase::Msp);
        for r in &mut options.element_rules {
            if let ElementRule::ByServiceCategory { categories, .. } = &mut r.rule {
                categories.clear();
            }
        }
        options.populate_empty_infra_rule(UseCase::Msp);
        assert_eq!(
            infra_categories(&options),
            infra_categories(&TopologyRequestOptions::for_use_case(UseCase::Msp))
        );

        for r in &mut options.element_rules {
            if let ElementRule::ByServiceCategory { categories, .. } = &mut r.rule {
                *categories = vec![ServiceCategory::DNS];
            }
        }
        options.populate_empty_infra_rule(UseCase::Msp);
        assert_eq!(infra_categories(&options), &[ServiceCategory::DNS]);
    }

    /// Rows saved before the per-entity map hold three lists; every id in them carries over.
    #[test]
    fn stored_three_list_tag_filter_reads_into_the_map() {
        let (host, service, subnet) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let filter: TopologyTagFilter = serde_json::from_value(serde_json::json!({
            "hidden_host_tag_ids": [host],
            "hidden_service_tag_ids": [service],
            "hidden_subnet_tag_ids": [subnet]
        }))
        .unwrap();
        assert_eq!(
            filter.hidden_tag_ids,
            BTreeMap::from([
                (EntityDiscriminants::Host, vec![host]),
                (EntityDiscriminants::Service, vec![service]),
                (EntityDiscriminants::Subnet, vec![subnet]),
            ])
        );
    }

    #[test]
    fn keyed_tag_filter_round_trips_in_the_new_shape() {
        let filter = TopologyTagFilter {
            hidden_tag_ids: BTreeMap::from([(EntityDiscriminants::Host, vec![Uuid::new_v4()])]),
        };
        let json = serde_json::to_value(&filter).unwrap();
        assert!(json.get("hidden_host_tag_ids").is_none());
        let back: TopologyTagFilter = serde_json::from_value(json).unwrap();
        assert_eq!(back, filter);
    }

    #[test]
    fn empty_or_missing_tag_filter_reads_as_empty() {
        let empty: TopologyTagFilter = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(empty.hidden_tag_ids.is_empty());
        let local: TopologyLocalOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(local.tag_filter.hidden_tag_ids.is_empty());
    }

    /// A saved view that hides an edge type this binary doesn't know still loads, keeping the
    /// edge types it does know.
    #[test]
    fn an_unknown_hidden_edge_type_is_dropped_not_fatal() {
        let options: TopologyLocalOptions = serde_json::from_value(serde_json::json!({
            "hide_edge_types": ["Hypervisor", "SomeFutureEdgeType"]
        }))
        .expect("an unknown edge type must not fail the options");
        assert_eq!(
            options.hide_edge_types,
            vec![EdgeTypeDiscriminants::Hypervisor]
        );
    }

    /// A stored map that predates a filter must adopt its default, or every existing topology
    /// keeps rendering as though the filter were switched off. This is the whole reason the merge
    /// exists: `#[serde(default)]` is a whole-field fallback and cannot fill a missing key.
    #[test]
    fn absent_filter_adopts_its_default() {
        let mut options = TopologyRequestOptions {
            hide_metadata_values: HashMap::from([(
                TopologyView::L2Physical,
                HashMap::from([(
                    EntityDiscriminants::Service,
                    HashMap::from([(MetadataFilterType::Category, vec!["OpenPorts".to_string()])]),
                )]),
            )]),
            ..Default::default()
        };

        options.merge_missing_hide_defaults();

        let interface = &options.hide_metadata_values[&TopologyView::L2Physical]
            [&EntityDiscriminants::Interface][&MetadataFilterType::LinkState];
        assert_eq!(interface, &vec!["Unlinked".to_string()]);
    }

    /// An empty list is an opinion — "show everything" — recorded when the user clears a filter.
    /// Treating it as unset would re-hide what they just chose to see, on the next page load.
    #[test]
    fn cleared_filter_is_not_refilled() {
        let mut options = TopologyRequestOptions {
            hide_metadata_values: HashMap::from([(
                TopologyView::L2Physical,
                HashMap::from([(
                    EntityDiscriminants::Interface,
                    HashMap::from([(MetadataFilterType::LinkState, vec![])]),
                )]),
            )]),
            ..Default::default()
        };

        options.merge_missing_hide_defaults();

        assert!(
            options.hide_metadata_values[&TopologyView::L2Physical]
                [&EntityDiscriminants::Interface][&MetadataFilterType::LinkState]
                .is_empty()
        );
    }

    /// Merging must not disturb a filter the user has actually set.
    #[test]
    fn user_selection_survives_the_merge() {
        let mut options = TopologyRequestOptions {
            hide_metadata_values: HashMap::from([(
                TopologyView::L2Physical,
                HashMap::from([(
                    EntityDiscriminants::Interface,
                    HashMap::from([(MetadataFilterType::LinkState, vec!["Linked".to_string()])]),
                )]),
            )]),
            ..Default::default()
        };

        options.merge_missing_hide_defaults();

        assert_eq!(
            options.hide_metadata_values[&TopologyView::L2Physical]
                [&EntityDiscriminants::Interface][&MetadataFilterType::LinkState],
            vec!["Linked".to_string()]
        );
    }

    /// Every value a view hides by default has to be one the view actually offers, or the chip
    /// that would turn it back on is never rendered and the hide becomes permanent and invisible.
    #[test]
    fn every_default_hidden_value_is_offered_by_its_view() {
        for view in TopologyView::iter() {
            let config = view.element_config();
            for (entity, by_filter) in &config.default_hidden_values {
                for (filter_type, values) in by_filter {
                    let declared = config.filter(*entity, *filter_type);

                    // Service.Category is declared in every view but only *rendered* where
                    // Service has an element or inline role, so a view without that role is
                    // allowed to hide a value it does not offer.
                    let Some(filter) = declared else { continue };

                    for value in values {
                        assert!(
                            filter.values.iter().any(|v| v.id == *value),
                            "{view:?} hides {entity:?}.{filter_type:?}={value} but does not offer it"
                        );
                    }
                }
            }
        }
    }
}
