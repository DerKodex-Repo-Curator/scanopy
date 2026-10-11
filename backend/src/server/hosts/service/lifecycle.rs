//! Constructor, query helpers, response builders, and child loading.
use super::*;

impl HostService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        storage: Arc<GenericPostgresStorage<Host>>,
        ip_address_service: Arc<IPAddressService>,
        port_service: Arc<PortService>,
        service_service: Arc<ServiceService>,
        interface_service: Arc<InterfaceService>,
        interface_neighbor_service: Arc<InterfaceNeighborService>,
        daemon_service: Arc<DaemonService>,
        credential_service: Arc<CredentialService>,
        subnet_service: Arc<SubnetService>,
        vlan_service: Arc<VlanService>,
        site_service: Arc<SiteService>,
        organization_service: Arc<OrganizationService>,
        event_bus: Arc<EventBus>,
        entity_tag_service: Arc<EntityTagService>,
    ) -> Self {
        Self {
            storage,
            ip_address_service,
            port_service,
            service_service,
            interface_service,
            interface_neighbor_service,
            daemon_service,
            credential_service,
            subnet_service,
            vlan_service,
            site_service,
            organization_service,
            event_bus,
            entity_tag_service,
        }
    }

    /// Get ports for a specific host
    pub async fn get_ports_for_host(&self, host_id: &Uuid) -> Result<Vec<Port>> {
        self.port_service.get_for_host(host_id).await
    }

    /// Get ip_addresses for a specific host
    pub async fn get_ip_addresses_for_host(&self, host_id: &Uuid) -> Result<Vec<IPAddress>> {
        self.ip_address_service.get_for_host(host_id).await
    }

    // =========================================================================
    // HostResponse builders (load children for API responses)
    // =========================================================================

    /// Get a single host with all children hydrated for API response
    pub async fn get_host_response(&self, id: &Uuid) -> Result<Option<HostResponse>> {
        let mut host = match self.get_by_id(id).await? {
            Some(h) => h,
            None => return Ok(None),
        };

        // Hydrate tags from junction table
        let tags = self
            .entity_tag_service
            .get_tags(id, &EntityDiscriminants::Host)
            .await?;
        host.base.tags = tags;

        let (ip_addresses, ports, services, interfaces) =
            self.load_children_for_host(&host.id).await?;
        let mut response = [HostResponse::from_host_with_children(
            host,
            ip_addresses,
            ports,
            services,
            interfaces,
        )];
        self.place_in_virtualization_trees(&mut response).await?;
        let [response] = response;
        Ok(Some(response))
    }

    /// Get all hosts with all children hydrated for API response
    pub async fn get_all_host_responses(
        &self,
        filter: StorableFilter<Host>,
    ) -> Result<Vec<HostResponse>> {
        let hosts = self.get_all(filter).await?;
        if hosts.is_empty() {
            return Ok(vec![]);
        }

        let host_ids: Vec<Uuid> = hosts.iter().map(|h| h.id).collect();
        let (ip_addresses_map, ports_map, services_map, interfaces_map) =
            self.load_children_for_hosts(&host_ids, None).await?;

        // Hydrate tags from junction table
        let tags_map = self
            .entity_tag_service
            .get_tags_map(&host_ids, EntityDiscriminants::Host, None)
            .await?;

        let responses = hosts
            .into_iter()
            .map(|mut host| {
                // Apply hydrated tags
                if let Some(tags) = tags_map.get(&host.id) {
                    host.base.tags = tags.clone();
                }
                let ip_addresses = ip_addresses_map.get(&host.id).cloned().unwrap_or_default();
                let ports = ports_map.get(&host.id).cloned().unwrap_or_default();
                let services = services_map.get(&host.id).cloned().unwrap_or_default();
                let interfaces = interfaces_map.get(&host.id).cloned().unwrap_or_default();
                HostResponse::from_host_with_children(
                    host,
                    ip_addresses,
                    ports,
                    services,
                    interfaces,
                )
            })
            .collect::<Vec<_>>();
        let mut responses = responses;
        self.place_in_virtualization_trees(&mut responses).await?;

        Ok(responses)
    }

    /// Get paginated hosts for API response, with children hydrated unless
    /// `include_children` is false.
    ///
    /// The child collections dominate the payload — a host list carrying them is
    /// an order of magnitude larger than one without — so callers that only need
    /// host identity (name pickers, id→name lookups, counts) pass `false` and
    /// skip the ports, services and interfaces. Tags are hydrated either
    /// way: they live on the host row's own junction table, callers filter and
    /// label by them, and they cost one query for the whole page.
    ///
    /// IP addresses are loaded either way too. The last rung of the
    /// [`Host::display_name`] ladder is the host's address, so without them every
    /// host titled by its IP came back with no `display_name` and rendered as
    /// "Unnamed host" in every picker fed by this path. They stay on the response
    /// because pickers show and search them.
    ///
    /// [`Host::display_name`]: crate::server::hosts::r#impl::base::Host::display_name
    ///
    /// Supports custom ordering via the `order_by` parameter.
    pub async fn get_all_host_responses_paginated(
        &self,
        filter: StorableFilter<Host>,
        order_by: &str,
        at: Option<DateTime<Utc>>,
        include_children: bool,
    ) -> Result<PaginatedResult<HostResponse>> {
        let result = self.storage().get_paginated(filter, order_by).await?;
        Ok(PaginatedResult {
            items: self
                .host_responses(result.items, at, include_children)
                .await?,
            total_count: result.total_count,
        })
    }

    /// [`Self::get_all_host_responses_paginated`] for a list grouped by virtualization tree and
    /// sorted by another field: each tree parent leads its subtree, and every set of siblings
    /// follows `order_by`. SQL can't order siblings by a field of their own and their
    /// ancestors', so the order is built here from every matching row's key, and the page is
    /// sliced from it.
    ///
    /// `filter` carries [`VIRTUALIZATION_TREE_JOIN`] and whatever `order_by` joins, and no
    /// pagination: `limit` and `offset` are applied to the ordered ids.
    ///
    /// [`VIRTUALIZATION_TREE_JOIN`]: crate::server::hosts::r#impl::virtualization_tree::VIRTUALIZATION_TREE_JOIN
    pub async fn get_all_host_responses_tree_ordered(
        &self,
        filter: StorableFilter<Host>,
        order_by: &str,
        limit: Option<u32>,
        offset: u32,
        at: Option<DateTime<Utc>>,
        include_children: bool,
    ) -> Result<PaginatedResult<HostResponse>> {
        use crate::server::hosts::r#impl::virtualization_tree::{
            VIRTUALIZATION_TREE_GROUP_SQL, VIRTUALIZATION_TREE_PARENT_SQL, tree_order,
        };

        let keys = self
            .storage()
            .get_ordered_values(
                filter,
                order_by,
                &[
                    VIRTUALIZATION_TREE_GROUP_SQL,
                    VIRTUALIZATION_TREE_PARENT_SQL,
                ],
            )
            .await?;
        let rows: Vec<(Uuid, String, Option<Uuid>)> = keys
            .into_iter()
            .map(|(id, values)| {
                let group = values.first().cloned().flatten().unwrap_or_default();
                let parent = values
                    .get(1)
                    .cloned()
                    .flatten()
                    .and_then(|v| v.parse().ok());
                (id, group, parent)
            })
            .collect();
        let total_count = rows.len() as u64;

        let page: Vec<Uuid> = tree_order(&rows)
            .into_iter()
            .skip(offset as usize)
            .take(limit.map(|l| l as usize).unwrap_or(usize::MAX))
            .collect();
        let mut hosts = self
            .storage()
            .get_all(StorableFilter::<Host>::new_from_entity_ids(&page))
            .await?;
        let rank: HashMap<Uuid, usize> = page.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        hosts.sort_by_key(|h| rank.get(&h.id).copied().unwrap_or(usize::MAX));

        Ok(PaginatedResult {
            items: self.host_responses(hosts, at, include_children).await?,
            total_count,
        })
    }

    /// Responses for `hosts`, in their order: children, tags and tree placement hydrated.
    async fn host_responses(
        &self,
        hosts: Vec<Host>,
        at: Option<DateTime<Utc>>,
        include_children: bool,
    ) -> Result<Vec<HostResponse>> {
        if hosts.is_empty() {
            return Ok(vec![]);
        }

        let host_ids: Vec<Uuid> = hosts.iter().map(|h| h.id).collect();
        // Hydrate children as-of the same instant as the host rows so a snapshot
        // view shows a coherent point-in-time host + children bundle.
        let (ip_addresses_map, ports_map, services_map, interfaces_map) = if include_children {
            self.load_children_for_hosts(&host_ids, at).await?
        } else {
            (
                self.ip_address_service.get_for_hosts(&host_ids, at).await?,
                Default::default(),
                Default::default(),
                Default::default(),
            )
        };

        // Hydrate tags from junction table
        let tags_map = self
            .entity_tag_service
            .get_tags_map(&host_ids, EntityDiscriminants::Host, None)
            .await?;

        let responses = hosts
            .into_iter()
            .map(|mut host| {
                // Apply hydrated tags
                if let Some(tags) = tags_map.get(&host.id) {
                    host.base.tags = tags.clone();
                }
                let ip_addresses = ip_addresses_map.get(&host.id).cloned().unwrap_or_default();
                let ports = ports_map.get(&host.id).cloned().unwrap_or_default();
                let services = services_map.get(&host.id).cloned().unwrap_or_default();
                let interfaces = interfaces_map.get(&host.id).cloned().unwrap_or_default();
                HostResponse::from_host_with_children(
                    host,
                    ip_addresses,
                    ports,
                    services,
                    interfaces,
                )
            })
            .collect::<Vec<_>>();
        let mut responses = responses;
        self.place_in_virtualization_trees(&mut responses).await?;
        Ok(responses)
    }

    /// Load all children for a single host.
    pub(crate) async fn load_children_for_host(
        &self,
        host_id: &Uuid,
    ) -> Result<(Vec<IPAddress>, Vec<Port>, Vec<Service>, Vec<Interface>)> {
        let ip_addresses = self.ip_address_service.get_for_host(host_id).await?;
        let ports = self.port_service.get_for_host(host_id).await?;
        // SCD2: live services only.
        let services = self
            .service_service
            .get_all_ordered(
                StorableFilter::<Service>::new_from_host_ids(&[*host_id]).live(),
                "position ASC",
            )
            .await?;
        let interfaces = self.interface_service.get_for_host(host_id).await?;

        Ok((ip_addresses, ports, services, interfaces))
    }

    /// Batch load all children for multiple hosts.
    /// `at = None` loads live children; `Some(t)` loads SCD2 state as of `t`.
    async fn load_children_for_hosts(
        &self,
        host_ids: &[Uuid],
        at: Option<DateTime<Utc>>,
    ) -> Result<(
        HashMap<Uuid, Vec<IPAddress>>,
        HashMap<Uuid, Vec<Port>>,
        HashMap<Uuid, Vec<Service>>,
        HashMap<Uuid, Vec<Interface>>,
    )> {
        let ip_addresses_map = self.ip_address_service.get_for_hosts(host_ids, at).await?;
        let ports_map = self.port_service.get_for_hosts(host_ids, at).await?;

        // Load services ordered by position and group by host_id.
        let services = self
            .service_service
            .get_all_ordered(
                StorableFilter::<Service>::new_from_host_ids(host_ids).live_or_as_of(at),
                "position ASC",
            )
            .await?;

        let mut services_map: HashMap<Uuid, Vec<Service>> = HashMap::new();
        for service in services {
            services_map
                .entry(service.base.host_id)
                .or_default()
                .push(service);
        }

        // Load interfaces and group by host_id
        let mut interfaces_map = self.interface_service.get_for_hosts(host_ids, at).await?;
        // Sort each host's entries by if_index
        for entries in interfaces_map.values_mut() {
            entries.sort_by_key(|e| e.base.if_index);
        }

        Ok((ip_addresses_map, ports_map, services_map, interfaces_map))
    }
}
