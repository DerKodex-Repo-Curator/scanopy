<script lang="ts">
	import CreateButton from '$lib/shared/components/data/controls/CreateButton.svelte';
	import { isProvisionalCidr } from '$lib/shared/utils/cidr-source';
	import { cidrContains } from '$lib/shared/utils/cidr';
	import SubnetUtilization from './SubnetUtilization.svelte';
	import { subnetNesting } from '../nesting';
	import SubnetEditModal from './SubnetEditModal/SubnetEditModal.svelte';
	import ProvisionalRangeModal from './ProvisionalRangeModal.svelte';
	import Loading from '$lib/shared/components/feedback/Loading.svelte';
	import EmptyState from '$lib/shared/components/layout/EmptyState.svelte';
	import PreDaemonEmptyState from '$lib/shared/components/layout/PreDaemonEmptyState.svelte';
	import type { Subnet, SubnetResponse } from '../types/base';
	import DataControls from '$lib/shared/components/data/DataControls.svelte';
	import { STALE_PARAM } from '$lib/shared/components/data/controls/listQuery';
	import type { TableDefaults } from '$lib/shared/components/data/types';
	import type { CardAction } from '$lib/shared/components/data/types';
	import { subnetColumns } from './SubnetTab.columns';
	import { collectionLookup } from '$lib/shared/entity-fields';
	import { Trash2, Edit, CloudAlert } from 'lucide-svelte';
	import { useTagsQuery } from '$lib/features/tags/queries';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import {
		isUserManagedSubnet,
		useSubnetsQuery,
		useCreateSubnetMutation,
		useMergeSubnetMutation,
		useUpdateSubnetMutation,
		useDeleteSubnetMutation,
		useBulkDeleteSubnetsMutation
	} from '../queries';
	import { useSitesQuery } from '$lib/features/sites/queries';
	import { useServicesByIds } from '$lib/features/services/queries';
	import type { Service } from '$lib/features/services/types/base';
	import { useDiscoveriesByIds } from '$lib/features/discovery/queries';
	import { discoveryRunIds } from '$lib/features/discovery/columns';
	import type { TabProps } from '$lib/shared/types';
	import type { components } from '$lib/api/schema';
	import { downloadCsv } from '$lib/shared/utils/csvExport';
	import { modalState, resolveModalDeepLink } from '$lib/shared/stores/modal-registry';
	import {
		common_confirmDeleteName,
		common_create,
		common_confirmBulkDelete,
		common_noEntityYet,
		common_delete,
		common_edit,
		common_subnets,
		daemons_installPromptSubnets,
		subnets_resolveRange
	} from '$lib/paraglide/messages';
	import { hasDaemon } from '$lib/shared/onboarding/checklist';

	type OnboardingOperation = components['schemas']['OnboardingOperationDiscriminants'];
	type SubnetOrderField = components['schemas']['SubnetOrderField'];

	let { isReadOnly = false }: TabProps = $props();

	// Organization query for onboarding state
	const organizationQuery = useOrganizationQuery();
	let onboarding = $derived((organizationQuery.data?.onboarding ?? []) as OnboardingOperation[]);

	// Queries
	const tagsQuery = useTagsQuery();
	// Staleness filter state. Server-side so it applies to the whole set, and
	// keyed separately from the shared subnets cache.
	let stale = $state<boolean | null>(null);
	const subnetsQuery = useSubnetsQuery(undefined, () => stale ?? undefined);

	// The list sends only staleness to the endpoint; it filters, searches and orders the rest itself.
	function handleQueryChange(query: URLSearchParams) {
		stale = query.get(STALE_PARAM) === 'true' ? true : null;
	}
	const sitesQuery = useSitesQuery();
	// The container runtimes that manage the bridge subnets, fetched by id.
	const runtimesQuery = useServicesByIds(() => [
		...new Set(
			(subnetsQuery.data ?? [])
				.map((s) => s.virtualization_service_id)
				.filter((id): id is string => id != null)
		)
	]);
	const discoveryRunsQuery = useDiscoveriesByIds(() => discoveryRunIds(subnetsQuery.data ?? []));

	// Mutations
	const createSubnetMutation = useCreateSubnetMutation();
	const updateSubnetMutation = useUpdateSubnetMutation();
	const mergeSubnetMutation = useMergeSubnetMutation();
	const deleteSubnetMutation = useDeleteSubnetMutation();
	const bulkDeleteSubnetsMutation = useBulkDeleteSubnetsMutation();

	// Derived data
	let tagsData = $derived(tagsQuery.data ?? []);
	let subnetsData = $derived((subnetsQuery.data ?? []).filter(isUserManagedSubnet));
	let sitesData = $derived(sitesQuery.data ?? []);
	let runtimesData = $derived(runtimesQuery.data ?? []);
	let discoveryRunsData = $derived(discoveryRunsQuery.data ?? []);

	let nesting = $derived(subnetNesting(subnetsData));

	function runtimeOf(subnet: Subnet): Service | undefined {
		return runtimesData.find((s) => s.id === subnet.virtualization_service_id);
	}
	let isLoading = $derived(subnetsQuery.isPending);

	let showSubnetEditor = $state(false);
	let editingSubnet = $state<Subnet | null>(null);
	let showProvisionalRange = $state(false);
	let resolvingSubnet = $state<SubnetResponse | null>(null);

	// Deep-link: open subnet editor from URL (handles both fresh open and entity switch)
	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'subnet-editor',
			subnetsData,
			showSubnetEditor,
			editingSubnet?.id
		);
		if (result !== undefined) {
			editingSubnet = result;
			showSubnetEditor = true;
		}
	});

	/**
	 * Deep-link: resolve an assumed range from anywhere that names one.
	 *
	 * A scan warning is the other place an operator meets a guessed range, and it is where they
	 * meet it first — this is what lets that row hand off to the same four-way choice instead of
	 * growing a second copy of it. Same shape as the editor's deep link above.
	 */
	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'provisional-range',
			subnetsData,
			showProvisionalRange,
			resolvingSubnet?.id,
			isProvisionalCidr
		);
		// Create mode is meaningless here — there is no range to resolve without a subnet — so a
		// link that names none is ignored rather than opening an empty modal.
		if (result) {
			resolvingSubnet = result;
			showProvisionalRange = true;
		}
	});

	function handleCreateSubnet() {
		editingSubnet = null;
		showSubnetEditor = true;
	}

	/** Row actions. */
	function subnetActions(subnet: SubnetResponse): CardAction[] {
		if (isReadOnly) return [];

		const actions: CardAction[] = [];

		// Only where there is something to resolve. A badge that says a range was guessed and
		// offers no way to settle it leaves the operator with the question and no answer.
		if (isProvisionalCidr(subnet)) {
			// Neutral, like Edit. The badge in the row already carries the colour, and the two
			// variants that would stand out are spoken for: amber means stale and red means broken,
			// which is exactly what an assumed range is not.
			actions.push({
				label: subnets_resolveRange(),
				icon: CloudAlert,
				onClick: () => handleResolveRange(subnet)
			});
		}

		actions.push(
			{ label: common_edit(), icon: Edit, onClick: () => handleEditSubnet(subnet) },
			{
				label: common_delete(),
				icon: Trash2,
				class: 'btn-icon-danger',
				onClick: () => handleDeleteSubnet(subnet)
			}
		);

		return actions;
	}

	function handleResolveRange(subnet: SubnetResponse) {
		resolvingSubnet = subnet;
		showProvisionalRange = true;
	}

	/**
	 * A settled range that already covers the one being resolved, if the site holds one.
	 *
	 * This is the state discovery deliberately leaves alone: where a reading covers *several*
	 * assumed ranges it corrects none of them, because folding them into one means deleting rows.
	 * Offering the merge here is how that gets resolved, one deliberate click at a time.
	 *
	 * Searching `subnetsData` rather than the raw query is what keeps the `0.0.0.0/0` catch-alls
	 * out: they are synthetic, so `isUserManagedSubnet` has already dropped them, and either would
	 * otherwise "cover" every range on the site.
	 */
	let coveringSubnet = $derived.by(() => {
		if (!resolvingSubnet) return null;
		const target = resolvingSubnet;
		return (
			subnetsData.find(
				(candidate) =>
					candidate.id !== target.id &&
					candidate.site_id === target.site_id &&
					!isProvisionalCidr(candidate) &&
					cidrContains(candidate.cidr, target.cidr)
			) ?? null
		);
	});

	async function handleMergeRange(subnet: Subnet, into: Subnet) {
		try {
			await mergeSubnetMutation.mutateAsync({ id: subnet.id, into: into.id });
			handleCloseProvisionalRange();
		} catch {
			// Error handled by mutation
		}
	}

	function handleCloseProvisionalRange() {
		showProvisionalRange = false;
		resolvingSubnet = null;
	}

	/**
	 * Keep the range as it stands. The server takes the higher rung of the ladder on update, so a
	 * range a person confirmed sticks and the next scan cannot put the row back to an inference.
	 *
	 * `Manual` is that rung on the shared ladder — the same one a value typed into any other field
	 * carries, which is what makes "nothing discovery reads displaces it" one rule rather than a
	 * per-field convention.
	 */
	async function handleConfirmRange(subnet: Subnet) {
		try {
			await updateSubnetMutation.mutateAsync({ ...subnet, cidr_source: 'Manual' });
			handleCloseProvisionalRange();
		} catch {
			// Error handled by mutation
		}
	}

	/** Straight into the editor, which is where a corrected CIDR is typed. */
	function handleCorrectRange(subnet: Subnet) {
		handleCloseProvisionalRange();
		handleEditSubnet(subnet);
	}

	function handleEditSubnet(subnet: Subnet) {
		editingSubnet = subnet;
		showSubnetEditor = true;
	}

	function handleDeleteSubnet(subnet: Subnet) {
		if (confirm(common_confirmDeleteName({ name: subnet.name }))) {
			deleteSubnetMutation.mutate(subnet.id);
		}
	}

	async function handleSubnetCreate(data: Subnet) {
		try {
			await createSubnetMutation.mutateAsync(data);
			showSubnetEditor = false;
			editingSubnet = null;
		} catch {
			// Error handled by mutation
		}
	}

	async function handleSubnetUpdate(_id: string, data: Subnet) {
		try {
			await updateSubnetMutation.mutateAsync(data);
			showSubnetEditor = false;
			editingSubnet = null;
		} catch {
			// Error handled by mutation
		}
	}

	function handleCloseSubnetEditor() {
		showSubnetEditor = false;
		editingSubnet = null;
	}

	async function handleBulkDelete(ids: string[]) {
		if (confirm(common_confirmBulkDelete({ count: ids.length, entity: common_subnets() }))) {
			await bulkDeleteSubnetsMutation.mutateAsync(ids);
		}
	}

	function getSubnetTags(subnet: Subnet): string[] {
		return subnet.tags;
	}

	// CSV export handler
	async function handleCsvExport() {
		await downloadCsv('Subnet', {});
	}

	const tableDefaults: TableDefaults<SubnetOrderField> = {
		group: 'cidr',
		sort: { field: 'cidr', direction: 'asc' }
	};

	// Related entities the registry's subnet fields read: the managing runtime, the site.
	let lookup = $derived(collectionLookup({ Service: runtimesData, Site: sitesData }));

	let subnetFields = $derived(
		subnetColumns({
			lookup: () => lookup,
			sites: () => sitesData,
			tags: () => tagsData,
			discoveryRuns: () => discoveryRunsData,
			nesting: () => nesting,
			runtimeOf,
			runtimes: () => runtimesData,
			utilizationCell
		})
	);
</script>

{#snippet utilizationCell(subnet: SubnetResponse)}
	<SubnetUtilization {subnet} />
{/snippet}

<!-- The page's own actions, last in the table toolbar beside the filter and columns. -->
{#snippet toolbarActions()}
	{#if hasDaemon(onboarding) && !isReadOnly}
		<CreateButton onclick={handleCreateSubnet} />
	{/if}
{/snippet}

<div class="space-y-6">
	{#if !hasDaemon(onboarding)}
		<PreDaemonEmptyState title={daemons_installPromptSubnets()} {isReadOnly} />
	{:else if isLoading}
		<!-- Loading state -->
		<Loading />
	{:else if subnetsData.length === 0 && stale === null}
		<!--
			"Nothing configured yet" only when nothing is narrowing the list. `stale` is a
			server-side filter, so an empty response under it means "no matches", not "no
			subnets" — and this branch replaces the controls that would let the user clear
			it. DataControls renders the filtered-empty state instead.
		-->
		<EmptyState
			title={common_noEntityYet({ entity: common_subnets() })}
			subtitle=""
			onClick={handleCreateSubnet}
			cta={common_create()}
		/>
	{:else}
		<DataControls
			title={common_subnets()}
			{toolbarActions}
			items={subnetsData}
			fields={subnetFields}
			storageKey="scanopy-subnets-table-state"
			defaults={tableDefaults}
			onBulkDelete={isReadOnly ? undefined : handleBulkDelete}
			entityType="Subnet"
			canEditTags={!isReadOnly}
			getItemTags={getSubnetTags}
			getItemId={(item) => item.id}
			onQueryChange={handleQueryChange}
			serverQuery={{ search: false, tags: false, order: false, fields: false }}
			onCsvExport={handleCsvExport}
			getActions={subnetActions}
			entityLabel={common_subnets()}
		></DataControls>
	{/if}
</div>

<SubnetEditModal
	name="subnet-editor"
	isOpen={showSubnetEditor}
	subnet={editingSubnet}
	onCreate={handleSubnetCreate}
	onUpdate={handleSubnetUpdate}
	onClose={handleCloseSubnetEditor}
	onDelete={editingSubnet
		? () => {
				handleDeleteSubnet(editingSubnet!);
				handleCloseSubnetEditor();
			}
		: null}
/>

<ProvisionalRangeModal
	isOpen={showProvisionalRange}
	subnet={resolvingSubnet}
	coveredBy={coveringSubnet}
	onConfirm={handleConfirmRange}
	onCorrect={handleCorrectRange}
	onMerge={handleMergeRange}
	onClose={handleCloseProvisionalRange}
/>
