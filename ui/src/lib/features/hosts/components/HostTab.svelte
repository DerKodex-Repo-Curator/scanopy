<script lang="ts">
	import { tagGroupIdOf, tagSetsParam } from '$lib/features/tags/groupFields';
	import type {
		Host,
		CreateHostWithServicesRequest,
		UpdateHostWithServicesRequest
	} from '../types/base';
	import Loading from '$lib/shared/components/feedback/Loading.svelte';
	import EmptyState from '$lib/shared/components/layout/EmptyState.svelte';
	import PreDaemonEmptyState from '$lib/shared/components/layout/PreDaemonEmptyState.svelte';
	import { hostDisplayName } from '$lib/features/hosts/host-display-name';
	import {
		hostOsFamilyIds,
		hostOsFamilyName,
		type HostOsFamily
	} from '$lib/features/hosts/host-os';
	import OsTag from './OsTag.svelte';
	import HostEditor from './HostEditModal/HostEditor.svelte';
	import HostConsolidationModal from './HostConsolidationModal.svelte';
	import HostExportModal from './HostExportModal.svelte';
	import DataControls from '$lib/shared/components/data/DataControls.svelte';
	import type { TableDefaults } from '$lib/shared/components/data/types';
	import type { CardAction } from '$lib/shared/components/data/types';
	import { hostColumns } from './HostTab.columns';
	import { collectionLookup } from '$lib/shared/entity-fields';
	import { entitySources } from '$lib/shared/stores/metadata';
	import { Plus, Trash2, RefreshCw, Replace, Edit } from 'lucide-svelte';
	import { useTagsQuery } from '$lib/features/tags/queries';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import UpgradeButton from '$lib/shared/components/UpgradeButton.svelte';
	import type { TabProps } from '$lib/shared/types';
	import {
		common_confirmDeleteName,
		common_consolidate,
		common_create,
		common_delete,
		common_edit,
		common_hosts,
		common_confirmBulkDelete,
		common_noEntityYet,
		common_rescan,
		daemons_installPromptHosts,
		hosts_notVirtualized
	} from '$lib/paraglide/messages';

	let { isReadOnly = false }: TabProps = $props();
	import { missingRootIds } from '../virtualization-tree';
	import {
		useHostsQuery,
		useHostsByIds,
		useCreateHostMutation,
		useUpdateHostMutation,
		useDeleteHostMutation,
		useBulkDeleteHostsMutation,
		useConsolidateHostsMutation,
		useRescanHostMutation,
		type HostQueryOptions
	} from '../queries';
	import { useServicesByIds, useServicesCacheQuery } from '$lib/features/services/queries';
	import { useDaemonsQuery } from '$lib/features/daemons/queries';
	import { useIPAddressesQuery } from '$lib/features/ip-addresses/queries';
	import { usePortsQuery } from '$lib/features/ports/queries';
	import { useInterfacesByIds, useInterfacesQuery } from '$lib/features/interfaces/queries';
	import { useDiscoveriesByIds } from '$lib/features/discovery/queries';
	import { discoveryRunIds } from '$lib/features/discovery/columns';
	import { useCredentialsQuery } from '$lib/features/credentials/queries';
	import { useSubnetsQuery } from '$lib/features/subnets/queries';
	import { useSitesQuery } from '$lib/features/sites/queries';
	import { modalState, resolveModalDeepLink } from '$lib/shared/stores/modal-registry';
	import type { components } from '$lib/api/schema';
	import { hasDaemon } from '$lib/shared/onboarding/checklist';
	import { useFieldValuesQuery } from '$lib/shared/api/field-values';

	type OnboardingOperation = components['schemas']['OnboardingOperationDiscriminants'];
	type HostOrderField = components['schemas']['HostOrderField'];
	type OrderDirection = components['schemas']['OrderDirection'];
	type EntitySourceType = components['schemas']['EntitySourceDiscriminants'];

	const HOST_FIELD_VALUES = '/api/v1/hosts/field-values/{field}';
	const SERVICE_FIELD_VALUES = '/api/v1/services/field-values/{field}';

	// Pagination state
	let pageSize = $state(20);
	let currentPage = $state(1);

	// Ordering state (for server-side ordering).
	// The grouping and sort the table opens with. Declared ahead of the query state so the
	// first request already carries them; DataControls replaces them with a saved choice.
	const tableDefaults: TableDefaults<HostOrderField> = {
		group: 'virtualized_by',
		sort: { field: 'name', direction: 'asc' }
	};
	let groupBy = $state<HostOrderField | undefined>(tableDefaults.group);
	let orderBy = $state<HostOrderField | undefined>(tableDefaults.sort?.field);
	let orderDirection = $state<OrderDirection>(tableDefaults.sort?.direction ?? 'asc');

	// Tag filter state (for server-side filtering)
	let tagIds = $state<string[]>([]);
	// Tag-group columns sort, group and filter on the server, by group id.
	let groupByTagGroup = $state<string | undefined>(undefined);
	let orderByTagGroup = $state<string | undefined>(undefined);
	let tagGroupFilters = $state<Record<string, string[]>>({});
	// Staleness filter state (server-side: the list is server-paginated)
	let stale = $state<boolean | null>(null);
	// Search state (server-side, for the same reason)
	let search = $state('');

	// Field filter state. Server-side for the same reason as the two above: the
	// client holds one page of hosts, so filtering here would narrow that page
	// while the total count kept describing every match.
	let filterSiteIds = $state<string[]>([]);
	let filterVirtualizationServiceNames = $state<string[]>([]);
	let filterIncludeUnvirtualized = $state(false);
	let filterServiceNames = $state<string[]>([]);
	let filterSources = $state<EntitySourceType[]>([]);
	let filterManufacturers = $state<string[]>([]);
	let filterModels = $state<string[]>([]);
	let filterSysLocations = $state<string[]>([]);
	let filterOsFamilies = $state<HostOsFamily[]>([]);
	let filterCredentialIds = $state<string[]>([]);

	/** The hardware, OS and credential filters, shared by the list and the export. */
	function fieldFilterParams() {
		return {
			manufacturers: filterManufacturers.length > 0 ? filterManufacturers : undefined,
			models: filterModels.length > 0 ? filterModels : undefined,
			sys_locations: filterSysLocations.length > 0 ? filterSysLocations : undefined,
			os_families: filterOsFamilies.length > 0 ? filterOsFamilies : undefined,
			credential_ids: filterCredentialIds.length > 0 ? filterCredentialIds : undefined
		};
	}

	// Queries
	const organizationQuery = useOrganizationQuery();
	let org = $derived(organizationQuery.data);
	let hostLimit = $derived(org?.plan?.included_hosts ?? null);
	let canBuyMoreHosts = $derived(
		org?.plan?.host_cents !== undefined && org?.plan?.host_cents !== null
	);
	let onboarding = $derived((org?.onboarding ?? []) as OnboardingOperation[]);

	const tagsQuery = useTagsQuery();
	// Paginated hosts with server-side pagination, ordering, and tag filtering.
	//
	// Deliberately NOT gated on `isActive`, unlike the other tabs' list queries.
	// `useHostsQuery` is the last remaining writer of the ip-addresses / ports /
	// services / interfaces caches (it populates them from its nested response),
	// and those caches have no fetcher of their own. Gating this would leave the
	// services tab's binding chips and the host editor's interface lists empty for
	// anyone who never opens the hosts tab. Un-gate it only once those caches have
	// real queries — see planned-work/child-cache-rearchitecture.md.
	const hostsQuery = useHostsQuery((): HostQueryOptions => ({
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
		site_ids: filterSiteIds.length > 0 ? filterSiteIds : undefined,
		// Both values checked is no constraint, so it is sent as nothing.
		virtualization_service_names:
			filterVirtualizationServiceNames.length > 0 ? filterVirtualizationServiceNames : undefined,
		include_unvirtualized: filterIncludeUnvirtualized || undefined,
		service_names: filterServiceNames.length > 0 ? filterServiceNames : undefined,
		sources: filterSources.length > 0 ? filterSources : undefined,
		...fieldFilterParams()
	}));
	const sitesQuery = useSitesQuery();
	useDaemonsQuery();
	const ipAddressesQuery = useIPAddressesQuery();
	const portsQuery = usePortsQuery();
	const interfacesQuery = useInterfacesQuery();
	const credentialsQuery = useCredentialsQuery();
	const subnetsQuery = useSubnetsQuery();
	// Filter options: the values the caller's hosts actually hold, counted by the server. The
	// loaded page would only offer the values on it, and the fixtures and caches would offer values
	// no host holds. None of these takes the tab's active filters, so the options do not shrink as
	// the user filters.
	const siteValuesQuery = useFieldValuesQuery(HOST_FIELD_VALUES, 'site_id');
	const virtualizedByValuesQuery = useFieldValuesQuery(HOST_FIELD_VALUES, 'virtualized_by');
	const sourceValuesQuery = useFieldValuesQuery(HOST_FIELD_VALUES, 'source');
	const osFamilyValuesQuery = useFieldValuesQuery(HOST_FIELD_VALUES, 'os_family');
	const manufacturerValuesQuery = useFieldValuesQuery(HOST_FIELD_VALUES, 'manufacturer');
	const modelValuesQuery = useFieldValuesQuery(HOST_FIELD_VALUES, 'model');
	const sysLocationValuesQuery = useFieldValuesQuery(HOST_FIELD_VALUES, 'sys_location');
	// Every service belongs to a host, so the services' names are the names some host runs.
	const serviceNameValuesQuery = useFieldValuesQuery(SERVICE_FIELD_VALUES, 'name');

	// Selective service lookup - only fetches services needed for virtualization display
	// Extract service IDs from visible hosts for "Virtualized By" field
	const servicesQuery = useServicesByIds(() => {
		return (hostsQuery.data?.items ?? [])
			.map((h) => h.virtualization_service_id)
			.filter((id): id is string => id != null)
			.filter((id, idx, arr) => arr.indexOf(id) === idx);
	});
	// The interface a virtualizing host presents each network identity from. It belongs to the
	// virtualizing host, which is rarely on this page, so fetched by id.
	const presentingInterfacesQuery = useInterfacesByIds(() => [
		...new Set(
			(hostsQuery.data?.items ?? [])
				.map((h) => h.virtualization_interface_id)
				.filter((id): id is string => id != null)
		)
	]);
	const discoveryRunsQuery = useDiscoveriesByIds(() =>
		discoveryRunIds(hostsQuery.data?.items ?? [])
	);
	// The hosts that head a virtualization tree on this page but sit on another, so the group
	// header can name them.
	const virtualizationRootsQuery = useHostsByIds(() =>
		missingRootIds(hostsQuery.data?.items ?? [])
	);

	// Mutations
	const createHostMutation = useCreateHostMutation();
	const updateHostMutation = useUpdateHostMutation();
	const deleteHostMutation = useDeleteHostMutation();
	const bulkDeleteHostsMutation = useBulkDeleteHostsMutation();
	const consolidateHostsMutation = useConsolidateHostsMutation();
	const rescanHostMutation = useRescanHostMutation();

	// Derived data
	let tagsData = $derived(tagsQuery.data ?? []);
	let hostsData = $derived(hostsQuery.data?.items ?? []);
	let hostsPagination = $derived(hostsQuery.data?.pagination ?? null);
	let servicesData = $derived(servicesQuery.data ?? []);
	let presentingInterfacesData = $derived(presentingInterfacesQuery.data ?? []);
	let discoveryRunsData = $derived(discoveryRunsQuery.data ?? []);
	let virtualizationRoots = $derived(
		new Map([...hostsData, ...(virtualizationRootsQuery.data ?? [])].map((h) => [h.id, h]))
	);
	const servicesCacheQuery = useServicesCacheQuery();
	let allServicesData = $derived(servicesCacheQuery.data ?? []);
	let sitesData = $derived(sitesQuery.data ?? []);
	let ipAddressesData = $derived(ipAddressesQuery.data ?? []);
	let portsData = $derived(portsQuery.data ?? []);
	let interfacesData = $derived(interfacesQuery.data ?? []);
	let credentialsData = $derived(credentialsQuery.data ?? []);
	let subnetsData = $derived(subnetsQuery.data ?? []);
	// Only show full loading on initial load (no data yet)
	let isInitialLoading = $derived(hostsQuery.isPending && !hostsQuery.data);

	// Host limit tracking
	let totalHostCount = $derived(hostsPagination?.total_count ?? hostsData.length);
	let isAtHostLimit = $derived(
		hostLimit !== null && totalHostCount >= hostLimit && !canBuyMoreHosts
	);
	let isNearHostLimit = $derived(
		hostLimit !== null &&
			totalHostCount >= hostLimit - 5 &&
			totalHostCount < hostLimit &&
			!canBuyMoreHosts
	);

	// Page change handler for server-side pagination
	function handlePageChange(page: number, newPageSize: number) {
		currentPage = page;
		pageSize = newPageSize;
	}

	// Order change handler for server-side ordering
	// Values are now directly HostOrderField values from the orderField property
	function handleOrderChange(
		groupField: string | null,
		orderField: string | null,
		direction: 'asc' | 'desc'
	) {
		groupByTagGroup = tagGroupIdOf(groupField) ?? undefined;
		orderByTagGroup = tagGroupIdOf(orderField) ?? undefined;
		groupBy = groupByTagGroup ? undefined : ((groupField as HostOrderField) ?? undefined);
		orderBy = orderByTagGroup ? undefined : ((orderField as HostOrderField) ?? undefined);
		orderDirection = direction;
	}

	// Tag filter change handler for server-side filtering
	function handleTagFilterChange(selectedTagIds: string[]) {
		tagIds = selectedTagIds;
		// Reset to page 1 is handled by DataControls
	}

	function handleStaleFilterChange(next: boolean | null) {
		stale = next;
	}

	// Search change handler for server-side search (debounced by DataControls)
	function handleSearchChange(query: string) {
		search = query;
	}

	/**
	 * Server-side field filter handler.
	 *
	 * The panel offers what the user reads — a site's name, a service's name —
	 * while the API filters on ids, so each case resolves the labels back through
	 * the same data the options were built from. Every key here must match a
	 * field marked `serverFiltered`; an unhandled one would filter nothing at
	 * all, since DataControls skips the client pass for those fields.
	 */
	function handleFilterChange(fieldKey: string, values: string[]) {
		const tagGroupId = tagGroupIdOf(fieldKey);
		if (tagGroupId) {
			tagGroupFilters = { ...tagGroupFilters, [tagGroupId]: values };
			return;
		}
		switch (fieldKey) {
			case 'site_id':
				filterSiteIds = idsForNames(values, sitesData);
				break;
			case 'virtualized_by':
				// "Not Virtualized" is a choice about absence, so it is carried as
				// its own flag rather than as a name nothing would match. The rest are the
				// virtualizing services' names, which the server filters on directly: the
				// options come from every host, and no client cache holds every service.
				filterIncludeUnvirtualized = values.includes(hosts_notVirtualized());
				filterVirtualizationServiceNames = values.filter(
					(value) => value !== hosts_notVirtualized()
				);
				break;
			case 'services':
				filterServiceNames = values;
				break;
			case 'source':
				// Fixture ids are the backend's `EntitySourceDiscriminants` names, emitted from
				// that same enum, so an id is a valid filter value by construction.
				filterSources = entitySources
					.getItems()
					.filter((source) => values.includes(entitySources.getName(source.id)))
					.map((source) => source.id as EntitySourceType);
				break;
			// The field-values options are the stored strings themselves, so they pass through.
			case 'manufacturer':
				filterManufacturers = values;
				break;
			case 'model':
				filterModels = values;
				break;
			case 'sys_location':
				filterSysLocations = values;
				break;
			case 'os_family':
				filterOsFamilies = hostOsFamilyIds.filter((family) =>
					values.includes(hostOsFamilyName(family))
				);
				break;
			case 'credentials':
				filterCredentialIds = idsForNames(values, credentialsData);
				break;
			default:
				throw new Error(
					`HostTab: no server-side filter handles "${fieldKey}". A serverFiltered field ` +
						`without a case here filters nothing — neither the client nor the server.`
				);
		}
	}

	/** Resolve display names back to the ids the API filters on. */
	function idsForNames(names: string[], from: { id: string; name: string }[]): string[] {
		const wanted = new Set(names);
		return from.filter((entry) => wanted.has(entry.name)).map((entry) => entry.id);
	}

	// Export modal state
	let showExportModal = $state(false);
	// Carries the field filters too: the export handler applies the same
	// `apply_field_filters` as the list, so an export mirrors what the user was
	// looking at rather than the whole network.
	let exportParams = $derived({
		tag_ids: tagIds.length > 0 ? tagIds : undefined,
		tag_sets: tagSetsParam(tagGroupFilters, tagsData),
		group_by_tag_group: groupByTagGroup,
		order_by_tag_group: orderByTagGroup,
		order_by: orderBy,
		order_direction: orderDirection,
		site_ids: filterSiteIds.length > 0 ? filterSiteIds : undefined,
		virtualization_service_names:
			filterVirtualizationServiceNames.length > 0 ? filterVirtualizationServiceNames : undefined,
		include_unvirtualized: filterIncludeUnvirtualized || undefined,
		service_names: filterServiceNames.length > 0 ? filterServiceNames : undefined,
		sources: filterSources.length > 0 ? filterSources : undefined,
		...fieldFilterParams()
	});

	let showHostEditor = $state(false);
	// Track the host being edited by id + a snapshot, and resolve `editingHost` from the
	// live query cache. So when an external change (e.g. a credential assignment removed
	// elsewhere) invalidates and refetches hosts, the open editor reflects it without a
	// page reload. The snapshot is the fallback for hosts not in the current page.
	let editingHostId = $state<string | null>(null);
	let editingHostSnapshot = $state<Host | null>(null);
	let editingHost = $derived(
		editingHostId ? (hostsData.find((h) => h.id === editingHostId) ?? editingHostSnapshot) : null
	);
	function setEditingHost(host: Host | null) {
		editingHostId = host?.id ?? null;
		editingHostSnapshot = host;
	}

	let otherHost = $state<Host | null>(null);
	let showHostConsolidationModal = $state(false);

	// Deep-link: open host editor from URL (handles both fresh open and entity switch)
	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'host-editor',
			hostsData,
			showHostEditor,
			editingHost?.id
		);
		if (result !== undefined) {
			setEditingHost(result);
			showHostEditor = true;
		}
	});

	function hostIPAddresses(host: Host) {
		return ipAddressesData.filter((i) => i.host_id === host.id);
	}

	// Related entities the registry's host fields read, from the collections this page already
	// loads. Keyed by id, since a virtualizing service or presenting interface can sit in both the
	// page's cache and the by-id fetch, and a child lookup would otherwise list it twice.
	let lookup = $derived(
		collectionLookup({
			IPAddress: ipAddressesData,
			Port: portsData,
			Interface: [
				...new Map([...interfacesData, ...presentingInterfacesData].map((i) => [i.id, i])).values()
			],
			Service: [...new Map([...allServicesData, ...servicesData].map((s) => [s.id, s])).values()],
			Site: sitesData
		})
	);

	let hostFields = $derived(
		hostColumns({
			lookup: () => lookup,
			services: () => servicesData,
			allServices: () => allServicesData,
			ipAddresses: () => ipAddressesData,
			ports: () => portsData,
			interfaces: () => interfacesData,
			credentials: () => credentialsData,
			subnets: () => subnetsData,
			sites: () => sitesData,
			tags: () => tagsData,
			discoveryRuns: () => discoveryRunsData,
			virtualizationRoots: () => virtualizationRoots,
			hostIPAddresses,
			siteValues: () => siteValuesQuery.data,
			virtualizedByValues: () => virtualizedByValuesQuery.data,
			sourceValues: () => sourceValuesQuery.data,
			osFamilyValues: () => osFamilyValuesQuery.data,
			manufacturerValues: () => manufacturerValuesQuery.data,
			modelValues: () => modelValuesQuery.data,
			sysLocationValues: () => sysLocationValuesQuery.data,
			serviceNameValues: () => serviceNameValuesQuery.data,
			osCell
		})
	);

	function handleCreateHost() {
		setEditingHost(null);
		showHostEditor = true;
	}

	/** Row actions. */
	function hostActions(host: Host): CardAction[] {
		if (isReadOnly) return [];

		return [
			{ label: common_edit(), icon: Edit, onClick: () => handleEditHost(host) },
			// A rescan targets the host's known addresses, so a host with none has nothing to
			// scan and the endpoint answers 400. Offering the action anyway meant the only way to
			// find that out was to trigger it and read the error toast. Devices identified purely
			// by a MAC are the reason such a host exists at all.
			...(hostIPAddresses(host).length > 0
				? [{ label: common_rescan(), icon: RefreshCw, onClick: () => handleRescanHost(host) }]
				: []),
			{ label: common_consolidate(), icon: Replace, onClick: () => handleStartConsolidate(host) },
			{
				label: common_delete(),
				icon: Trash2,
				class: 'btn-icon-danger',
				onClick: () => handleDeleteHost(host)
			}
		];
	}

	function handleEditHost(host: Host) {
		setEditingHost(host);
		showHostEditor = true;
	}

	function handleStartConsolidate(host: Host) {
		otherHost = host;
		showHostConsolidationModal = true;
	}

	function handleRescanHost(host: Host) {
		rescanHostMutation.mutate({ id: host.id, name: hostDisplayName(host) });
	}

	function handleDeleteHost(host: Host) {
		if (confirm(common_confirmDeleteName({ name: hostDisplayName(host) }))) {
			deleteHostMutation.mutate(host.id);
		}
	}

	async function handleHostCreate(data: CreateHostWithServicesRequest) {
		try {
			await createHostMutation.mutateAsync(data);
			showHostEditor = false;
			setEditingHost(null);
		} catch {
			// Error handled by mutation
		}
	}

	async function handleHostUpdate(data: UpdateHostWithServicesRequest) {
		try {
			await updateHostMutation.mutateAsync(data);
			showHostEditor = false;
			setEditingHost(null);
		} catch {
			// Error handled by mutation
		}
	}

	async function handleConsolidateHosts(destinationHostId: string, otherHostId: string) {
		try {
			await consolidateHostsMutation.mutateAsync({
				destinationHostId,
				otherHostId,
				otherHostName: otherHost ? hostDisplayName(otherHost) : undefined
			});
			showHostConsolidationModal = false;
			otherHost = null;
		} catch {
			// Error handled by mutation
		}
	}

	async function handleBulkDelete(ids: string[]) {
		if (confirm(common_confirmBulkDelete({ count: ids.length, entity: common_hosts() }))) {
			await bulkDeleteHostsMutation.mutateAsync(ids);
		}
	}

	function getHostTags(host: Host): string[] {
		return host.tags;
	}

	function handleCloseHostEditor() {
		showHostEditor = false;
		setEditingHost(null);
	}
</script>

{#snippet osCell(host: Host)}
	{#if host.os}
		<OsTag os={host.os} source={host.os_source} />
	{:else}
		<span class="text-tertiary text-sm">—</span>
	{/if}
{/snippet}

<!-- The page's own actions, last in the table toolbar beside the filter and columns. -->
{#snippet toolbarActions()}
	{#if hasDaemon(onboarding)}
		<div class="flex items-center gap-3">
			{#if hostLimit !== null && !canBuyMoreHosts}
				<span
					class="text-sm {isAtHostLimit
						? 'text-amber-400'
						: isNearHostLimit
							? 'text-yellow-400'
							: 'text-tertiary'}"
				>
					{totalHostCount} / {hostLimit}
				</span>
			{/if}
			{#if !isReadOnly}
				{#if isAtHostLimit}
					<UpgradeButton feature="hosts" surface="hosts_tab" gate_type="limit_hit" />
				{:else}
					{#if isNearHostLimit}
						<UpgradeButton feature="hosts" surface="hosts_tab" gate_type="limit_hit" />
					{/if}
					<button class="btn-primary toolbar-control flex items-center" onclick={handleCreateHost}
						><Plus class="h-5 w-5" />{common_create()}</button
					>
				{/if}
			{/if}
		</div>
	{/if}
{/snippet}

<div class="space-y-6">
	{#if !hasDaemon(onboarding)}
		<PreDaemonEmptyState title={daemons_installPromptHosts()} {isReadOnly} />
	{:else if isInitialLoading}
		<!-- Loading state (only on initial load) -->
		<Loading />
	{:else if hostsData.length === 0 && !hostsPagination}
		<!-- Empty state -->
		<EmptyState
			title={common_noEntityYet({ entity: common_hosts() })}
			subtitle=""
			onClick={handleCreateHost}
			cta={common_create()}
		/>
	{:else}
		<DataControls
			title={common_hosts()}
			{toolbarActions}
			items={hostsData}
			fields={hostFields}
			storageKey="scanopy-hosts-table-state"
			defaults={tableDefaults}
			onBulkDelete={isReadOnly ? undefined : handleBulkDelete}
			entityType="Host"
			canEditTags={!isReadOnly}
			getItemTags={getHostTags}
			getItemId={(item) => item.id}
			serverPagination={hostsPagination}
			onPageChange={handlePageChange}
			onOrderChange={handleOrderChange}
			onTagFilterChange={handleTagFilterChange}
			onFilterChange={handleFilterChange}
			onStaleFilterChange={handleStaleFilterChange}
			onSearchChange={handleSearchChange}
			onExportClick={() => {
				showExportModal = true;
			}}
			getActions={hostActions}
			entityLabel={common_hosts()}
		></DataControls>
	{/if}
</div>

<HostEditor
	isOpen={showHostEditor}
	name="host-editor"
	host={editingHost}
	onCreate={handleHostCreate}
	onDelete={handleDeleteHost}
	onUpdate={handleHostUpdate}
	onClose={handleCloseHostEditor}
/>

<HostConsolidationModal
	isOpen={showHostConsolidationModal}
	{otherHost}
	onConsolidate={handleConsolidateHosts}
	onClose={() => (showHostConsolidationModal = false)}
/>

<HostExportModal
	isOpen={showExportModal}
	onClose={() => (showExportModal = false)}
	{exportParams}
/>
