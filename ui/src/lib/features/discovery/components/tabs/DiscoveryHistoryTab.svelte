<script lang="ts">
	import { discoveryPhases, discoveryTypes } from '$lib/shared/stores/metadata';
	import EmptyState from '$lib/shared/components/layout/EmptyState.svelte';
	import PreDaemonEmptyState from '$lib/shared/components/layout/PreDaemonEmptyState.svelte';
	import DataControls from '$lib/shared/components/data/DataControls.svelte';
	import type { TableDefaults } from '$lib/shared/components/data/types';
	import type { Discovery } from '../../types/base';
	import DiscoveryEditModal from '../DiscoveryModal/DiscoveryEditModal.svelte';
	import Loading from '$lib/shared/components/feedback/Loading.svelte';
	import { formatTimestamp } from '$lib/shared/utils/formatting';
	import { discoveryHistoryColumns } from './DiscoveryHistoryTab.columns';
	import { collectionLookup } from '$lib/shared/entity-fields';
	import {
		useDiscoveryHistoryQuery,
		useCreateDiscoveryMutation,
		useUpdateDiscoveryMutation,
		useBulkDeleteDiscoveriesMutation,
		type DiscoveryHistoryQueryParams
	} from '../../queries';
	import { useDaemonsQuery } from '$lib/features/daemons/queries';
	import { useSitesQuery } from '$lib/features/sites/queries';
	import { useHostsByIds } from '$lib/features/hosts/queries';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import { hasDaemon } from '$lib/shared/onboarding/checklist';
	import { useFieldValuesQuery } from '$lib/shared/api/field-values';
	import type { components } from '$lib/api/schema';
	import type { TabProps } from '$lib/shared/types';
	import { downloadCsv } from '$lib/shared/utils/csvExport';
	import {
		modalState,
		openModal,
		closeModal,
		resolveModalDeepLink
	} from '$lib/shared/stores/modal-registry';
	import { Info } from 'lucide-svelte';
	import { useTagGroupsQuery, useTagsQuery } from '$lib/features/tags/queries';
	import type { CardAction } from '$lib/shared/components/data/types';
	import { runOutcomeTag, TERMINAL_PHASES, type OutcomeTag } from '../../utils/outcome';
	import {
		common_details,
		common_none,
		daemons_installPromptDiscoveries,
		discovery_confirmDeleteHistorical,
		discovery_historyTitle,
		discovery_noHistorySessions,
		discovery_noHistorySessionsSubtitle
	} from '$lib/paraglide/messages';

	type OnboardingOperation = components['schemas']['OnboardingOperationDiscriminants'];
	type DiscoveryOrderField = components['schemas']['DiscoveryOrderField'];
	type OrderDirection = components['schemas']['OrderDirection'];
	type DiscoveryPhase = components['schemas']['DiscoveryPhase'];

	const DISCOVERY_FIELD_VALUES = '/api/v1/discovery/field-values/{field}';

	let { isReadOnly = false, isActive = false }: TabProps = $props();

	// Organization query for onboarding state
	const organizationQuery = useOrganizationQuery();
	let onboarding = $derived((organizationQuery.data?.onboarding ?? []) as OnboardingOperation[]);

	// Pagination state (managed by DataControls, updated via callback)
	let pageSize = $state(20);
	let currentPage = $state(1);

	// Ordering state (server-side)
	// The grouping and sort the table opens with. Declared ahead of the query state so the
	// first request already carries them; DataControls replaces them with a saved choice.
	const tableDefaults: TableDefaults<DiscoveryOrderField> = {
		sort: { field: 'started_at', direction: 'desc' }
	};
	let groupBy = $state<DiscoveryOrderField | undefined>(tableDefaults.group);
	let orderBy = $state<DiscoveryOrderField | undefined>(tableDefaults.sort?.field);
	let orderDirection = $state<OrderDirection>(tableDefaults.sort?.direction ?? 'asc');

	// Search state (server-side: the run history is paginated, so a client-side
	// search would only ever match the page in hand)
	let search = $state('');

	// Field filter state, server-side for the same reason.
	let filterSiteIds = $state<string[]>([]);
	let filterDaemonIds = $state<string[]>([]);
	let filterDiscoveryTypes = $state<string[]>([]);
	let filterPhases = $state<DiscoveryPhase[]>([]);
	let hasServerFilters = $derived(
		filterSiteIds.length > 0 ||
			filterDaemonIds.length > 0 ||
			filterDiscoveryTypes.length > 0 ||
			filterPhases.length > 0
	);

	// Queries
	const discoveriesQuery = useDiscoveryHistoryQuery(
		(): DiscoveryHistoryQueryParams => ({
			limit: pageSize,
			offset: (currentPage - 1) * pageSize,
			group_by: groupBy,
			order_by: orderBy,
			order_direction: orderDirection,
			search: search || undefined,
			site_ids: filterSiteIds.length > 0 ? filterSiteIds : undefined,
			daemon_ids: filterDaemonIds.length > 0 ? filterDaemonIds : undefined,
			discovery_types: filterDiscoveryTypes.length > 0 ? filterDiscoveryTypes : undefined,
			phases: filterPhases.length > 0 ? filterPhases : undefined
		}),
		() => isActive
	);
	// Filter options: the values the run history actually holds. Scoped to history rows the way
	// the list is, so scheduled configurations add none, and never narrowed by the active filters,
	// so the options do not shrink as the user filters. Gated like the list.
	const historyScope = () => ({ historical: true });
	const daemonValuesQuery = useFieldValuesQuery(
		DISCOVERY_FIELD_VALUES,
		'daemon_id',
		historyScope,
		() => isActive
	);
	const siteValuesQuery = useFieldValuesQuery(
		DISCOVERY_FIELD_VALUES,
		'site_id',
		historyScope,
		() => isActive
	);
	const typeValuesQuery = useFieldValuesQuery(
		DISCOVERY_FIELD_VALUES,
		'discovery_type',
		historyScope,
		() => isActive
	);
	const phaseValuesQuery = useFieldValuesQuery(
		DISCOVERY_FIELD_VALUES,
		'phase',
		historyScope,
		() => isActive
	);
	const tagsQuery = useTagsQuery();
	const tagGroupsQuery = useTagGroupsQuery();
	const daemonsQuery = useDaemonsQuery();
	const sitesQuery = useSitesQuery();

	// Mutations
	const createDiscoveryMutation = useCreateDiscoveryMutation();
	const updateDiscoveryMutation = useUpdateDiscoveryMutation();
	const bulkDeleteDiscoveriesMutation = useBulkDeleteDiscoveriesMutation();

	// Derived data
	let discoveriesData = $derived(discoveriesQuery.data?.items ?? []);
	let discoveriesPagination = $derived(discoveriesQuery.data?.pagination ?? null);
	let tagsData = $derived(tagsQuery.data ?? []);
	let daemonsData = $derived(daemonsQuery.data ?? []);
	let sitesData = $derived(sitesQuery.data ?? []);

	// Only the hosts the daemons run on. This was an unpaginated org-wide hosts
	// query (~1.9MB on a few hundred hosts), issued so the edit modal's daemon
	// picker could label each daemon with its host name — and because TanStack
	// dedupes by key, it was shared with every other consumer, so it loaded on
	// pages that never opened the modal. Scoped to the ids in hand.
	let daemonHostIds = $derived([
		...new Set(daemonsData.map((d) => d.host_id).filter((id): id is string => !!id))
	]);
	const hostsQuery = useHostsByIds(() => daemonHostIds);
	let hostsData = $derived(hostsQuery.data ?? []);

	// Host names are decoration inside the modal, so the list must not block on
	// them — and with no daemons the by-ids query is disabled, which in TanStack
	// means it stays `isPending` forever. The server already returns only
	// historical rows (`historical: true`), so no client-side filter is needed.
	let isLoading = $derived(discoveriesQuery.isPending || daemonsQuery.isPending);

	// Page change handler for server-side pagination
	function handlePageChange(page: number, newPageSize: number) {
		currentPage = page;
		pageSize = newPageSize;
	}

	// Order change handler for server-side ordering
	function handleOrderChange(
		groupField: string | null,
		orderField: string | null,
		direction: 'asc' | 'desc'
	) {
		groupBy = (groupField as DiscoveryOrderField) ?? undefined;
		orderBy = (orderField as DiscoveryOrderField) ?? undefined;
		orderDirection = direction;
	}

	// Search change handler for server-side search (debounced by DataControls)
	function handleSearchChange(query: string) {
		search = query;
	}

	/**
	 * Server-side field filter handler.
	 *
	 * The panel offers what the user reads — a daemon's name, a site's name —
	 * while the API filters on ids, so each case resolves the labels back
	 * through the same data the options were built from. Every key here must
	 * match a field marked `serverFiltered`; an unhandled one would filter
	 * nothing at all, since DataControls skips the client pass for those fields.
	 */
	function handleFilterChange(fieldKey: string, values: string[]) {
		const wanted = new Set(values);
		switch (fieldKey) {
			case 'daemon_id':
				filterDaemonIds = daemonsData.filter((d) => wanted.has(d.name)).map((d) => d.id);
				break;
			case 'site_id':
				filterSiteIds = sitesData.filter((n) => wanted.has(n.name)).map((n) => n.id);
				break;
			case 'discovery_type':
				filterDiscoveryTypes = discoveryTypes
					.getItems()
					.filter((type) => wanted.has(discoveryTypes.getName(type.id)))
					.map((type) => type.id);
				break;
			case 'phase':
				filterPhases = TERMINAL_PHASES.filter((phase) =>
					wanted.has(discoveryPhases.getName(phase))
				);
				break;
			default:
				throw new Error(
					`DiscoveryHistoryTab: no server-side filter handles "${fieldKey}". A serverFiltered ` +
						`field without a case here filters nothing — neither the client nor the server.`
				);
		}
	}

	let showDiscoveryModal = $state(false);
	let editingDiscovery: Discovery | null = $state(null);

	// Deep-link: open detail modal from URL (fresh open and entity switch). A run linked from
	// another tab (the scan that found a host) is rarely on this page, so the helper falls back to
	// the run the link carried.
	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'discovery-history-detail',
			discoveriesData,
			showDiscoveryModal,
			editingDiscovery?.id
		);
		// No create mode here: a run without an id has nothing to show.
		if (result) {
			editingDiscovery = result;
			showDiscoveryModal = true;
		}
	});

	function handleEditDiscovery(discovery: Discovery) {
		editingDiscovery = discovery;
		showDiscoveryModal = true;
		openModal('discovery-history-detail', { id: discovery.id });
	}

	async function handleDiscoveryCreate(data: Discovery) {
		await createDiscoveryMutation.mutateAsync(data);
		showDiscoveryModal = false;
		editingDiscovery = null;
	}

	async function handleDiscoveryUpdate(id: string, data: Discovery) {
		await updateDiscoveryMutation.mutateAsync(data);
		showDiscoveryModal = false;
		editingDiscovery = null;
	}

	function handleCloseEditor() {
		showDiscoveryModal = false;
		editingDiscovery = null;
		closeModal();
	}

	async function handleBulkDelete(ids: string[]) {
		if (confirm(discovery_confirmDeleteHistorical({ count: ids.length }))) {
			await bulkDeleteDiscoveriesMutation.mutateAsync(ids);
		}
	}

	// CSV export handler
	async function handleCsvExport() {
		await downloadCsv('Discovery', {});
	}

	/**
	 * How a run ended, as a tag. A clean completion returns null and the column renders empty.
	 * Warnings have a column of their own: a tag here said only that the run had some, which made
	 * a single informational note look like a broken credential.
	 */
	function outcomeTag(discovery: Discovery): OutcomeTag | null {
		return runOutcomeTag(resultsOf(discovery));
	}

	/** A completed run's results, or null for a row that is not one. */
	function resultsOf(discovery: Discovery) {
		return discovery.run_type.type === 'Historical' ? discovery.run_type.results : null;
	}

	/** A run's recorded warnings, or none for a row that is not a completed run. */
	function warningsOf(discovery: Discovery) {
		return resultsOf(discovery)?.warnings ?? [];
	}

	/** Row actions. */
	function discoveryActions(discovery: Discovery): CardAction[] {
		return [{ label: common_details(), icon: Info, onClick: () => handleEditDiscovery(discovery) }];
	}

	// Related entities the registry's discovery fields read: the daemon, the site.
	let lookup = $derived(collectionLookup({ Daemon: daemonsData, Site: sitesData }));

	let fields = $derived(
		discoveryHistoryColumns({
			lookup: () => lookup,
			daemons: () => daemonsData,
			sites: () => sitesData,
			tags: () => tagsData,
			tagGroups: () => tagGroupsQuery.data,
			daemonValues: () => daemonValuesQuery.data,
			siteValues: () => siteValuesQuery.data,
			typeValues: () => typeValuesQuery.data,
			phaseValues: () => phaseValuesQuery.data,
			resultsOf,
			outcomeTag,
			warningsOf,
			startedAtCell: () => startedAtCell,
			finishedAtCell: () => finishedAtCell
		})
	);
</script>

{#snippet startedAtCell(discovery: Discovery)}
	{@render timestampCell(resultsOf(discovery)?.started_at)}
{/snippet}

{#snippet finishedAtCell(discovery: Discovery)}
	{@render timestampCell(resultsOf(discovery)?.finished_at)}
{/snippet}

<!-- The full timestamp: runs are often minutes apart, so a date alone cannot tell them apart. -->
{#snippet timestampCell(timestamp: string | null | undefined)}
	{#if timestamp}
		{formatTimestamp(timestamp)}
	{:else}
		<span class="text-muted" aria-hidden="true">—</span>
		<span class="sr-only">{common_none()}</span>
	{/if}
{/snippet}

<div class="space-y-6">
	{#if !hasDaemon(onboarding)}
		<PreDaemonEmptyState title={daemons_installPromptDiscoveries()} {isReadOnly} />
	{:else if isLoading}
		<Loading />
	{:else if discoveriesData.length === 0 && !search && !hasServerFilters}
		<!--
			"No sessions yet" only when nothing is narrowing the list. The search and the
			field filters run server-side and are restored from storage on mount, so an
			empty response under one means "no matches" — and this branch would take the
			controls away with it, re-arming the same no-match query on every reload.
			DataControls renders the filtered-empty state instead.
		-->
		<EmptyState
			title={discovery_noHistorySessions()}
			subtitle={discovery_noHistorySessionsSubtitle()}
		/>
	{:else}
		<DataControls
			title={discovery_historyTitle()}
			items={discoveriesData}
			{fields}
			onBulkDelete={isReadOnly ? undefined : handleBulkDelete}
			storageKey="scanopy-discovery-historical-table-state"
			defaults={tableDefaults}
			getItemId={(item) => item.id}
			serverPagination={discoveriesPagination}
			onPageChange={handlePageChange}
			onOrderChange={handleOrderChange}
			onSearchChange={handleSearchChange}
			onFilterChange={handleFilterChange}
			onCsvExport={handleCsvExport}
			getActions={discoveryActions}
			entityLabel={discovery_historyTitle()}
		></DataControls>
	{/if}
</div>

<DiscoveryEditModal
	name="discovery-history-detail"
	isOpen={showDiscoveryModal}
	hosts={hostsData}
	daemons={daemonsData}
	discovery={editingDiscovery}
	onCreate={handleDiscoveryCreate}
	onUpdate={handleDiscoveryUpdate}
	onClose={handleCloseEditor}
/>
