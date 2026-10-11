<script lang="ts">
	import { tagGroupIdOf, tagSetsParam } from '$lib/features/tags/groupFields';
	import Loading from '$lib/shared/components/feedback/Loading.svelte';
	import EmptyState from '$lib/shared/components/layout/EmptyState.svelte';
	import PreDaemonEmptyState from '$lib/shared/components/layout/PreDaemonEmptyState.svelte';
	import DataControls from '$lib/shared/components/data/DataControls.svelte';
	import type { TableDefaults } from '$lib/shared/components/data/types';
	import {
		entityRef,
		type CardAction,
		type LabelledCardFieldItem
	} from '$lib/shared/components/data/types';
	import { usePortsQuery } from '$lib/features/ports/queries';
	import { useIPAddressesQuery } from '$lib/features/ip-addresses/queries';
	import { useSubnetsQuery, isContainerSubnet } from '$lib/features/subnets/queries';
	import { formatIPAddress } from '$lib/features/hosts/address-labels';
	import { formatPort } from '$lib/shared/utils/formatting';
	import type { IPAddress, Port } from '$lib/features/hosts/types/base';
	import { entities, entitySources, matchConfidences } from '$lib/shared/stores/metadata';
	import { Trash2, Edit } from 'lucide-svelte';
	import type { Service } from '../types/base';
	import ServiceEditModal from './ServiceEditModal.svelte';
	import { useTagsQuery } from '$lib/features/tags/queries';
	import {
		useServicesQuery,
		useUpdateServiceMutation,
		useDeleteServiceMutation,
		useBulkDeleteServicesMutation,
		type ServicesQueryParams,
		useServicesByIds
	} from '../queries';
	import { useDiscoveriesByIds } from '$lib/features/discovery/queries';
	import { discoveryRunIds } from '$lib/features/discovery/columns';
	import { useHostsByIds, useHostSummariesQuery } from '$lib/features/hosts/queries';
	import { hostDisplayName } from '$lib/features/hosts/host-display-name';
	import { useSitesQuery } from '$lib/features/sites/queries';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import type { TabProps } from '$lib/shared/types';
	import type { components } from '$lib/api/schema';
	import { serviceColumns } from './ServiceTab.columns';
	import { collectionLookup } from '$lib/shared/entity-fields';
	import { SvelteMap } from 'svelte/reactivity';
	import { downloadCsv } from '$lib/shared/utils/csvExport';
	import { closeModal, modalState, resolveModalDeepLink } from '$lib/shared/stores/modal-registry';
	import {
		common_confirmBulkDelete,
		common_confirmDeleteName,
		common_delete,
		common_edit,
		common_noEntityYet,
		common_services,
		common_unbound,
		daemons_installPromptServices,
		services_notContainerized,
		services_subtitle
	} from '$lib/paraglide/messages';
	import { serviceDefinitions, ports as ports_metadata } from '$lib/shared/stores/metadata';
	import { hasDaemon } from '$lib/shared/onboarding/checklist';
	import { labelledFieldValueOptions, useFieldValuesQuery } from '$lib/shared/api/field-values';

	type OnboardingOperation = components['schemas']['OnboardingOperationDiscriminants'];
	type ServiceOrderField = components['schemas']['ServiceOrderField'];
	type OrderDirection = components['schemas']['OrderDirection'];
	type EntitySourceType = components['schemas']['EntitySourceDiscriminants'];
	type MatchConfidence = components['schemas']['MatchConfidence'];

	const SERVICE_FIELD_VALUES = '/api/v1/services/field-values/{field}';

	// Well-known port numbers for the filter panel. People look for 443, not
	// "HTTPS", so the options are the numbers themselves. Deduplicated because
	// some ports have both a TCP and a UDP definition (53, 515), and stripped of
	// the Custom entry, whose number is a placeholder 0. Free-form port entry
	// would need a control the value-list filter panel doesn't have.
	let wellKnownPortNumbers = $derived(
		[
			...new Set(
				(ports_metadata.getItems() ?? [])
					.filter((port) => !port.metadata.is_custom)
					.map((port) => port.metadata.number)
			)
		]
			.sort((a, b) => a - b)
			.map(String)
	);

	let { isReadOnly = false }: TabProps = $props();

	// Organization query for onboarding state
	const organizationQuery = useOrganizationQuery();
	let onboarding = $derived((organizationQuery.data?.onboarding ?? []) as OnboardingOperation[]);

	// Pagination state (managed by DataControls, updated via callback)
	let pageSize = $state(20);
	let currentPage = $state(1);

	// Ordering state (for server-side ordering).
	// The grouping and sort the table opens with. Declared ahead of the query state so the
	// first request already carries them; DataControls replaces them with a saved choice.
	const tableDefaults: TableDefaults<ServiceOrderField> = {
		sort: { field: 'name', direction: 'asc' }
	};
	let groupBy = $state<ServiceOrderField | undefined>(tableDefaults.group);
	let orderBy = $state<ServiceOrderField | undefined>(tableDefaults.sort?.field);
	let orderDirection = $state<OrderDirection>(tableDefaults.sort?.direction ?? 'asc');

	// Tag filter state (for server-side filtering)
	let tagIds = $state<string[]>([]);
	// Tag-group columns sort, group and filter on the server, by group id.
	let groupByTagGroup = $state<string | undefined>(undefined);
	let orderByTagGroup = $state<string | undefined>(undefined);
	let tagGroupFilters = $state<Record<string, string[]>>({});

	// Exclude categories state (for server-side filtering)
	let excludeCategories = $state<string[]>(['OpenPorts']);

	// Port filter (server-side: a client-side pass would only narrow the loaded
	// page)
	let ports = $state<number[]>([]);

	// Staleness filter state (server-side: the list is server-paginated)
	let stale = $state<boolean | null>(null);

	// Search state (server-side, for the same reason)
	let search = $state('');

	// The remaining field filters, server-side for the same reason: the client
	// holds one page of services, so filtering here would narrow that page while
	// the total count kept describing every match.
	let filterHostIds = $state<string[]>([]);
	let filterSiteIds = $state<string[]>([]);
	let filterServiceDefinitions = $state<string[]>([]);
	let filterVirtualizationServiceNames = $state<string[]>([]);
	let filterIncludeUncontainerized = $state(false);
	let filterSources = $state<EntitySourceType[]>([]);
	let filterMatchConfidences = $state<MatchConfidence[]>([]);

	// Queries
	const tagsQuery = useTagsQuery();
	// Paginated services with server-side pagination, ordering, and tag filtering
	const servicesQuery = useServicesQuery((): ServicesQueryParams => ({
		limit: pageSize,
		offset: (currentPage - 1) * pageSize,
		group_by: groupBy,
		order_by: orderBy,
		order_direction: orderDirection,
		tag_ids: tagIds.length > 0 ? tagIds : undefined,
		tag_sets: tagSetsParam(tagGroupFilters, tagsQuery.data ?? []),
		group_by_tag_group: groupByTagGroup,
		order_by_tag_group: orderByTagGroup,
		stale: stale ?? undefined,
		search: search || undefined,
		ports: ports.length > 0 ? ports : undefined,
		exclude_categories:
			excludeCategories.length > 0
				? (excludeCategories as components['schemas']['ServiceCategory'][])
				: undefined,
		host_ids: filterHostIds.length > 0 ? filterHostIds : undefined,
		site_ids: filterSiteIds.length > 0 ? filterSiteIds : undefined,
		service_definitions: filterServiceDefinitions.length > 0 ? filterServiceDefinitions : undefined,
		virtualization_service_names:
			filterVirtualizationServiceNames.length > 0 ? filterVirtualizationServiceNames : undefined,
		include_uncontainerized: filterIncludeUncontainerized || undefined,
		sources: filterSources.length > 0 ? filterSources : undefined,
		match_confidences: filterMatchConfidences.length > 0 ? filterMatchConfidences : undefined
	}));
	const sitesQuery = useSitesQuery();
	const portsQuery = usePortsQuery();
	// Option sources for the server-side filters. The lists below are the tab's
	// display data, scoped to the loaded page — filter options have to come from
	// the full sets or the panel only ever offers what is already on screen.
	const allHostsQuery = useHostSummariesQuery({});
	// Filter options: the values the caller's services actually hold, counted by the server.
	// None of these takes the tab's active filters, so the options do not shrink as the user
	// filters.
	const hostValuesQuery = useFieldValuesQuery(SERVICE_FIELD_VALUES, 'host');
	const siteValuesQuery = useFieldValuesQuery(SERVICE_FIELD_VALUES, 'site_id');
	const definitionValuesQuery = useFieldValuesQuery(SERVICE_FIELD_VALUES, 'service_definition');
	const containerizedByValuesQuery = useFieldValuesQuery(SERVICE_FIELD_VALUES, 'containerized_by');
	const matchConfidenceValuesQuery = useFieldValuesQuery(SERVICE_FIELD_VALUES, 'match_confidence');
	const sourceValuesQuery = useFieldValuesQuery(SERVICE_FIELD_VALUES, 'source');
	// The runtimes containerizing the visible services. Rarely on the same page as the services
	// they run, so fetched by id.
	const runtimesQuery = useServicesByIds(() => [
		...new Set(
			(servicesQuery.data?.items ?? [])
				.map((s) => s.virtualization_service_id)
				.filter((id): id is string => id != null)
		)
	]);
	const discoveryRunsQuery = useDiscoveriesByIds(() =>
		discoveryRunIds(servicesQuery.data?.items ?? [])
	);
	const ipAddressesQuery = useIPAddressesQuery();
	const subnetsQuery = useSubnetsQuery();

	// Selective host lookup - only fetches hosts needed for service display
	// Extract host IDs from visible services for host name display
	const hostsQuery = useHostsByIds(() => {
		const ids = (servicesQuery.data?.items ?? [])
			.filter((s) => s.host_id)
			.map((s) => s.host_id)
			.filter((id, idx, arr) => arr.indexOf(id) === idx);
		// Include host for service navigated via EntityTag
		const ms = $modalState;
		if (ms.name === 'service-editor' && ms.entityData?.host_id) {
			const hostId = ms.entityData.host_id as string;
			if (!ids.includes(hostId)) ids.push(hostId);
		}
		return ids;
	});

	// Mutations
	const updateServiceMutation = useUpdateServiceMutation();
	const deleteServiceMutation = useDeleteServiceMutation();
	const bulkDeleteServicesMutation = useBulkDeleteServicesMutation();

	// Derived data
	let tagsData = $derived(tagsQuery.data ?? []);
	let servicesData = $derived(servicesQuery.data?.items ?? []);
	let allHostsData = $derived(allHostsQuery.data?.items ?? []);
	let runtimesData = $derived(runtimesQuery.data ?? []);
	let discoveryRunsData = $derived(discoveryRunsQuery.data ?? []);
	let servicesPagination = $derived(servicesQuery.data?.pagination ?? null);
	let hostsData = $derived(hostsQuery.data ?? []);
	let sitesData = $derived(sitesQuery.data ?? []);
	let portsData = $derived(portsQuery.data ?? []);
	let ipAddressesData = $derived(ipAddressesQuery.data ?? []);
	let subnetsData = $derived(subnetsQuery.data ?? []);
	// Only show full loading on initial load (no data yet)
	let isInitialLoading = $derived(servicesQuery.isPending && !servicesQuery.data);

	function isContainerSubnetFn(subnetId: string): boolean {
		const subnet = subnetsData.find((s) => s.id === subnetId);
		return subnet ? isContainerSubnet(subnet) : false;
	}

	/**
	 * A service's port bindings, grouped by the interface they are bound to.
	 *
	 * Keyed on the binding's `ip_address_id` rather than the resolved interface:
	 * if the lookup fails (an SCD2-closed address the live query no longer
	 * returns) two distinct bindings would otherwise collapse to the same
	 * 'unbound' literal and trip Svelte's each_key_duplicate.
	 */
	function portBindingItems(service: Service): LabelledCardFieldItem[] {
		const grouped = new SvelteMap<string | null, { iface: IPAddress | null; ports: Port[] }>();

		for (const binding of service.bindings.filter((b) => b.type === 'Port')) {
			const port = portsData.find((p) => p.id === binding.port_id);
			if (!port) continue;

			const interfaceId = binding.ip_address_id ?? null;
			if (!grouped.has(interfaceId)) {
				const iface = interfaceId
					? (ipAddressesData.find((i) => i.id === interfaceId) ?? null)
					: null;
				grouped.set(interfaceId, { iface, ports: [] });
			}
			grouped.get(interfaceId)!.ports.push(port);
		}

		return [...grouped.entries()].map(([interfaceId, { iface, ports: bound }]) => {
			const portList = bound.map((p) => formatPort(p)).join(', ');
			return {
				id: interfaceId ?? 'unbound',
				label: iface
					? `${iface.name ? iface.name + ': ' : ''} ${iface.ip_address} (${portList})`
					: `${common_unbound()} (${portList})`,
				color: entities.getColorHelper('Port').color
			};
		});
	}

	/** Interfaces a service is bound to directly, rather than through a port. */
	function ipBindingItems(service: Service): LabelledCardFieldItem[] {
		return service.bindings
			.filter((b) => b.type === 'IPAddress')
			.map((b) => b.ip_address_id)
			.filter((id): id is string => id !== null)
			.map((id) => ipAddressesData.find((i) => i.id === id))
			.filter((iface): iface is IPAddress => iface !== undefined)
			.map((iface) => ({
				id: iface.id,
				label: formatIPAddress(iface, isContainerSubnetFn),
				color: entities.getColorHelper('IPAddress').color,
				entityRef: entityRef('IPAddress', iface.id, iface, { subnets: subnetsData })
			}));
	}

	// Page change handler for server-side pagination
	function handlePageChange(page: number, newPageSize: number) {
		currentPage = page;
		pageSize = newPageSize;
	}

	// Order change handler for server-side ordering
	// Values are now directly ServiceOrderField values from the orderField property
	function handleOrderChange(
		groupField: string | null,
		orderField: string | null,
		direction: 'asc' | 'desc'
	) {
		groupByTagGroup = tagGroupIdOf(groupField) ?? undefined;
		orderByTagGroup = tagGroupIdOf(orderField) ?? undefined;
		groupBy = groupByTagGroup ? undefined : ((groupField as ServiceOrderField) ?? undefined);
		orderBy = orderByTagGroup ? undefined : ((orderField as ServiceOrderField) ?? undefined);
		orderDirection = direction;
	}

	// Tag filter change handler for server-side filtering
	function handleTagFilterChange(selectedTagIds: string[]) {
		tagIds = selectedTagIds;
		// Reset to page 1 is handled by DataControls
	}

	/**
	 * Server-side field filter handler.
	 *
	 * The panel offers what the user reads — a host's title, a site's name —
	 * while the API filters on ids, so each case resolves the labels back
	 * through the same data the options were built from. Every key here must
	 * match a field marked `serverFiltered`; an unhandled one would filter
	 * nothing at all, since DataControls skips the client pass for those fields.
	 */
	function handleFilterChange(fieldKey: string, values: string[]) {
		const tagGroupId = tagGroupIdOf(fieldKey);
		if (tagGroupId) {
			tagGroupFilters = { ...tagGroupFilters, [tagGroupId]: values };
			return;
		}
		switch (fieldKey) {
			case 'category':
				excludeCategories = values;
				break;
			case 'port_bindings':
				ports = values.map(Number).filter((port) => Number.isFinite(port));
				break;
			case 'host': {
				const wanted = new Set(values);
				filterHostIds = allHostsData
					.filter((host) => wanted.has(hostDisplayName(host)))
					.map((host) => host.id);
				break;
			}
			case 'site_id': {
				const wanted = new Set(values);
				filterSiteIds = sitesData.filter((site) => wanted.has(site.name)).map((site) => site.id);
				break;
			}
			case 'service_definition': {
				// The options render friendly names; the server filters on the raw
				// definition id, which is what `getGroupValue` already exposes.
				const wanted = new Set(values);
				filterServiceDefinitions = serviceDefinitions
					.getItems()
					.filter((definition) => definition.name !== null && wanted.has(definition.name))
					.map((definition) => definition.id);
				break;
			}
			case 'containerized_by': {
				// "Not Containerized" is a choice about absence, so it is carried
				// as its own flag rather than as a name nothing would match. The rest are
				// the runtimes' names, which the server filters on directly: no client
				// cache holds every runtime.
				filterIncludeUncontainerized = values.includes(services_notContainerized());
				filterVirtualizationServiceNames = values.filter((v) => v !== services_notContainerized());
				break;
			}
			// Fixture ids are the backend enum's names, emitted from that same enum, so an id
			// is a valid filter value by construction.
			case 'source':
				filterSources = entitySources
					.getItems()
					.filter((source) => values.includes(entitySources.getName(source.id)))
					.map((source) => source.id as EntitySourceType);
				break;
			case 'match_confidence':
				filterMatchConfidences = matchConfidences
					.getItems()
					.filter((confidence) => values.includes(matchConfidences.getName(confidence.id)))
					.map((confidence) => confidence.id as MatchConfidence);
				break;
			default:
				throw new Error(
					`ServiceTab: no server-side filter handles "${fieldKey}". A serverFiltered field ` +
						`without a case here filters nothing — neither the client nor the server.`
				);
		}
	}

	function handleStaleFilterChange(next: boolean | null) {
		stale = next;
	}

	// Search change handler for server-side search (debounced by DataControls)
	function handleSearchChange(query: string) {
		search = query;
	}

	// CSV export handler
	async function handleCsvExport() {
		await downloadCsv('Service', {
			tag_ids: tagIds.length > 0 ? tagIds : undefined,
			tag_sets: tagSetsParam(tagGroupFilters, tagsData),
			group_by_tag_group: groupByTagGroup,
			order_by_tag_group: orderByTagGroup,
			order_by: orderBy,
			order_direction: orderDirection
		});
	}

	let showServiceEditor = $state(false);
	let editingService = $state<Service | null>(null);

	// Deep-link: open service editor from URL (edit-only, validates host exists)
	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'service-editor',
			servicesData,
			showServiceEditor,
			editingService?.id,
			(s) => !!serviceHosts.get(s.id)
		);
		if (result) {
			editingService = result;
			showServiceEditor = true;
		}
	});

	/** Row actions. */
	function serviceActions(service: Service): CardAction[] {
		if (isReadOnly) return [];

		return [
			{ label: common_edit(), icon: Edit, onClick: () => handleEditService(service) },
			{
				label: common_delete(),
				icon: Trash2,
				class: 'btn-icon-danger',
				onClick: () => handleDeleteService(service)
			}
		];
	}

	function handleEditService(service: Service) {
		editingService = service;
		showServiceEditor = true;
	}
	function handleCloseServiceEditor() {
		// Clear URL modal state before setting local state to prevent the deep-link
		// effect from re-opening the modal (parent effects run before child effects,
		// so the deep-link effect would see showServiceEditor=false but $modalState
		// still pointing to this service)
		closeModal();
		showServiceEditor = false;
		editingService = null;
	}

	let serviceHosts = $derived(
		(() => {
			const map = new SvelteMap(
				servicesData.map((service) => {
					const foundHost = hostsData.find((h) => h.id == service.host_id);
					return [service.id, foundHost] as [string, (typeof hostsData)[0] | undefined];
				})
			);
			// Include entityData service (EntityTag navigation)
			const ms = $modalState;
			if (ms.name === 'service-editor' && ms.entityData?.id && ms.entityData?.host_id) {
				if (!map.has(ms.entityData.id as string)) {
					const host = hostsData.find((h) => h.id === ms.entityData!.host_id);
					if (host) map.set(ms.entityData.id as string, host);
				}
			}
			return map;
		})()
	);

	function handleDeleteService(service: Service) {
		if (confirm(common_confirmDeleteName({ name: service.name }))) {
			deleteServiceMutation.mutate(service.id);
		}
	}

	async function handleServiceUpdate(id: string, data: Service) {
		try {
			await updateServiceMutation.mutateAsync(data);
			showServiceEditor = false;
			editingService = null;
		} catch {
			// Error handled by mutation
		}
	}

	async function handleBulkDelete(ids: string[]) {
		if (confirm(common_confirmBulkDelete({ count: ids.length, entity: common_services() }))) {
			await bulkDeleteServicesMutation.mutateAsync(ids);
		}
	}

	function getServiceTags(service: Service): string[] {
		return service.tags;
	}

	// The categories of the definitions some service matched. Category has no column of its own,
	// so it is read off the present definitions. A default exclusion whose category no service
	// holds stays selected and excludes nothing.
	let serviceCategories = $derived(
		labelledFieldValueOptions(definitionValuesQuery.data, (id) =>
			serviceDefinitions.getCategory(id)
		)
	);

	// Related entities the registry's Service fields read, from the collections this tab loads.
	let lookup = $derived(
		collectionLookup({
			Host: hostsData,
			Port: portsData,
			IPAddress: ipAddressesData,
			Service: runtimesData,
			Site: sitesData,
			Subnet: subnetsData
		})
	);

	let serviceFields = $derived(
		serviceColumns({
			lookup: () => lookup,
			serviceHosts: () => serviceHosts,
			sites: () => sitesData,
			tags: () => tagsData,
			discoveryRuns: () => discoveryRunsData,
			runtimes: () => runtimesData,
			hostValues: () => hostValuesQuery.data,
			siteValues: () => siteValuesQuery.data,
			definitionValues: () => definitionValuesQuery.data,
			containerizedByValues: () => containerizedByValuesQuery.data,
			matchConfidenceValues: () => matchConfidenceValuesQuery.data,
			sourceValues: () => sourceValuesQuery.data,
			wellKnownPortNumbers: () => wellKnownPortNumbers,
			serviceCategories: () => serviceCategories,
			portBindingItems,
			ipBindingItems
		})
	);
</script>

<div class="space-y-6">
	{#if !hasDaemon(onboarding)}
		<PreDaemonEmptyState title={daemons_installPromptServices()} {isReadOnly} />
	{:else if isInitialLoading}
		<!-- Loading state (only on initial load) -->
		<Loading />
	{:else if servicesData.length === 0 && !servicesPagination}
		<!-- Empty state -->
		<EmptyState title={common_noEntityYet({ entity: common_services() })} subtitle="" />
	{:else}
		<DataControls
			title={common_services()}
			subtitle={services_subtitle()}
			items={servicesData}
			fields={serviceFields}
			storageKey="scanopy-services-table-state"
			defaults={tableDefaults}
			onBulkDelete={isReadOnly ? undefined : handleBulkDelete}
			entityType="Service"
			canEditTags={!isReadOnly}
			getItemTags={getServiceTags}
			getItemId={(item) => item.id}
			serverPagination={servicesPagination}
			onPageChange={handlePageChange}
			onOrderChange={handleOrderChange}
			onTagFilterChange={handleTagFilterChange}
			onFilterChange={handleFilterChange}
			onStaleFilterChange={handleStaleFilterChange}
			onSearchChange={handleSearchChange}
			onCsvExport={handleCsvExport}
			getActions={serviceActions}
			entityLabel={common_services()}
		></DataControls>
	{/if}
</div>

{#if editingService}
	{@const editingServiceHost = serviceHosts.get(editingService.id)}
	{#if editingServiceHost}
		<ServiceEditModal
			name="service-editor"
			service={editingService}
			host={editingServiceHost}
			isOpen={showServiceEditor}
			onUpdate={handleServiceUpdate}
			onClose={handleCloseServiceEditor}
			onDelete={() => {
				handleDeleteService(editingService!);
				handleCloseServiceEditor();
			}}
		/>
	{/if}
{/if}
