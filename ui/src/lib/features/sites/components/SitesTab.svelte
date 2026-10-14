<script lang="ts">
	import CreateButton from '$lib/shared/components/data/controls/CreateButton.svelte';
	import { siteColumns } from './SitesTab.columns';
	import Loading from '$lib/shared/components/feedback/Loading.svelte';
	import EmptyState from '$lib/shared/components/layout/EmptyState.svelte';
	import type { Site } from '../types';
	import SiteEditModal from './SiteEditModal.svelte';
	import DataControls from '$lib/shared/components/data/DataControls.svelte';
	import type { TableDefaults } from '$lib/shared/components/data/types';
	import type { FieldConfig } from '$lib/shared/components/data/types';
	import type { CardAction } from '$lib/shared/components/data/types';
	import { useCredentialsQuery } from '$lib/features/credentials/queries';
	import { Trash2, Edit } from 'lucide-svelte';
	import { useTagsQuery } from '$lib/features/tags/queries';
	import { useCurrentUserQuery } from '$lib/features/auth/queries';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import { permissions } from '$lib/shared/stores/metadata';
	import UpgradeButton from '$lib/shared/components/UpgradeButton.svelte';
	import type { TabProps } from '$lib/shared/types';
	import {
		common_confirmBulkDelete,
		common_create,
		common_delete,
		common_edit,
		common_sites,
		common_noEntityYet,
		sites_confirmDelete
	} from '$lib/paraglide/messages';

	let { isReadOnly = false }: TabProps = $props();
	import {
		useSitesQuery,
		useCreateSiteMutation,
		useUpdateSiteMutation,
		useDeleteSiteMutation,
		useBulkDeleteSitesMutation
	} from '../queries';
	import { useDaemonsQuery } from '$lib/features/daemons/queries';
	import { useHostsByIds } from '$lib/features/hosts/queries';
	import { isUserManagedSubnet, useSubnetsQuery } from '$lib/features/subnets/queries';
	import { useVlansQuery } from '$lib/features/vlans/queries';
	import { useDependenciesQuery } from '$lib/features/dependencies/queries';
	import { downloadCsv } from '$lib/shared/utils/csvExport';
	import { modalState, resolveModalDeepLink } from '$lib/shared/stores/modal-registry';

	// Queries
	const currentUserQuery = useCurrentUserQuery();
	let currentUser = $derived(currentUserQuery.data);

	const organizationQuery = useOrganizationQuery();
	let org = $derived(organizationQuery.data);
	let siteLimit = $derived(org?.plan?.included_sites ?? null);
	let canBuyMore = $derived(org?.plan?.site_cents !== undefined && org?.plan?.site_cents !== null);

	const tagsQuery = useTagsQuery();
	const sitesQuery = useSitesQuery();
	// What each site contains, resolved here for the table's columns.
	const daemonsQuery = useDaemonsQuery();
	const subnetsQuery = useSubnetsQuery();
	const vlansQuery = useVlansQuery();
	const credentialsQuery = useCredentialsQuery();
	useDependenciesQuery();

	// Only the hosts the daemons run on: each daemon chip needs one host name,
	// and an unpaginated org-wide hosts query is ~1.9MB, shared by key with
	// every other consumer.
	let daemonHostIds = $derived([
		...new Set((daemonsQuery.data ?? []).map((d) => d.host_id).filter((id): id is string => !!id))
	]);
	const daemonHostsQuery = useHostsByIds(() => daemonHostIds);
	let daemonHosts = $derived(daemonHostsQuery.data ?? []);

	// Mutations
	const createSiteMutation = useCreateSiteMutation();
	const updateSiteMutation = useUpdateSiteMutation();
	const deleteSiteMutation = useDeleteSiteMutation();
	const bulkDeleteSitesMutation = useBulkDeleteSitesMutation();

	// Derived data
	let tagsData = $derived(tagsQuery.data ?? []);
	let sitesData = $derived(sitesQuery.data ?? []);
	let daemonsData = $derived(daemonsQuery.data ?? []);
	// Narrowed once, here: both the per-site subnet column and the daemon chips
	// below must show the same list the Subnets and Daemons tabs do.
	let subnetsData = $derived((subnetsQuery.data ?? []).filter(isUserManagedSubnet));
	let vlansData = $derived(vlansQuery.data ?? []);
	let credentialsData = $derived(credentialsQuery.data ?? []);
	let isLoading = $derived(sitesQuery.isPending);
	let isAtSiteLimit = $derived(siteLimit !== null && sitesData.length >= siteLimit && !canBuyMore);
	let isNearSiteLimit = $derived(
		siteLimit !== null && sitesData.length >= siteLimit - 2 && !isAtSiteLimit && !canBuyMore
	);

	let showCreateSiteModal = $state(false);
	let editingSite = $state<Site | null>(null);

	// Deep-link: open site editor from URL (handles both fresh open and entity switch)
	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'site-editor',
			sitesData,
			showCreateSiteModal,
			editingSite?.id
		);
		if (result !== undefined) {
			editingSite = result;
			showCreateSiteModal = true;
		}
	});

	let allowBulkDelete = $derived(
		!isReadOnly && currentUser
			? permissions.getMetadata(currentUser.permissions).manage_org_entities
			: false
	);

	let canManageSites = $derived(
		!isReadOnly &&
			currentUser &&
			permissions.getMetadata(currentUser.permissions).manage_org_entities
	);

	/** Row actions. */
	function siteActions(site: Site): CardAction[] {
		if (!allowBulkDelete) return [];

		return [
			{ label: common_edit(), icon: Edit, onClick: () => handleEditSite(site) },
			{
				label: common_delete(),
				icon: Trash2,
				class: 'btn-icon-danger',
				onClick: () => handleDeleteSite(site)
			}
		];
	}

	function handleDeleteSite(site: Site) {
		if (confirm(sites_confirmDelete({ name: site.name }))) {
			deleteSiteMutation.mutate(site.id);
		}
	}

	function handleCreateSite() {
		editingSite = null;
		showCreateSiteModal = true;
	}

	function handleEditSite(site: Site) {
		editingSite = site;
		showCreateSiteModal = true;
	}

	async function handleBulkDelete(ids: string[]) {
		if (confirm(common_confirmBulkDelete({ count: ids.length, entity: common_sites() }))) {
			await bulkDeleteSitesMutation.mutateAsync(ids);
		}
	}

	function getSiteTags(site: Site): string[] {
		return site.tags;
	}

	async function handleSiteCreate(data: Site) {
		try {
			await createSiteMutation.mutateAsync(data);
			showCreateSiteModal = false;
			editingSite = null;
		} catch {
			// Error handled by mutation
		}
	}

	async function handleSiteUpdate(id: string, data: Site) {
		try {
			await updateSiteMutation.mutateAsync(data);
			showCreateSiteModal = false;
			editingSite = null;
		} catch {
			// Error handled by mutation
		}
	}

	function handleCloseSiteEditor() {
		showCreateSiteModal = false;
		editingSite = null;
	}

	// CSV export handler
	async function handleCsvExport() {
		await downloadCsv('Site', {});
	}

	const tableDefaults: TableDefaults<string> = { sort: { field: 'name', direction: 'asc' } };

	let siteFields = $derived<FieldConfig<Site>[]>(
		siteColumns({
			tags: () => tagsData,
			daemons: () => daemonsData,
			daemonHosts: () => daemonHosts,
			subnets: () => subnetsData,
			vlans: () => vlansData,
			credentials: () => credentialsData
		})
	);
</script>

<!-- The page's own actions, last in the table toolbar beside the filter and columns. -->
{#snippet toolbarActions()}
	<div class="flex items-center gap-3">
		{#if siteLimit !== null && !canBuyMore}
			<span
				class="text-sm {isAtSiteLimit
					? 'text-amber-400'
					: isNearSiteLimit
						? 'text-yellow-400'
						: 'text-tertiary'}"
			>
				{sitesData.length} / {siteLimit}
			</span>
		{/if}
		{#if canManageSites}
			{#if isAtSiteLimit}
				<UpgradeButton feature="sites" surface="sites_tab" gate_type="limit_hit" />
			{:else}
				{#if isNearSiteLimit}
					<UpgradeButton feature="sites" surface="sites_tab" gate_type="limit_hit" />
				{/if}
				<CreateButton onclick={handleCreateSite} />
			{/if}
		{/if}
	</div>
{/snippet}

<div class="space-y-6">
	<!-- Loading state -->
	{#if isLoading}
		<Loading />
	{:else if sitesData.length === 0}
		<!-- Empty state -->
		<EmptyState
			title={common_noEntityYet({ entity: common_sites() })}
			subtitle=""
			onClick={handleCreateSite}
			cta={common_create()}
		/>
	{:else}
		<DataControls
			title={common_sites()}
			{toolbarActions}
			items={sitesData}
			fields={siteFields}
			onBulkDelete={handleBulkDelete}
			entityType="Site"
			canEditTags={allowBulkDelete}
			getItemTags={getSiteTags}
			{allowBulkDelete}
			storageKey="scanopy-sites-table-state"
			defaults={tableDefaults}
			getItemId={(item) => item.id}
			onCsvExport={handleCsvExport}
			getActions={siteActions}
			entityLabel={common_sites()}
		></DataControls>
	{/if}
</div>

<SiteEditModal
	name="site-editor"
	isOpen={showCreateSiteModal}
	site={editingSite}
	onCreate={handleSiteCreate}
	onUpdate={handleSiteUpdate}
	onClose={handleCloseSiteEditor}
	onDelete={editingSite
		? () => {
				handleDeleteSite(editingSite!);
				handleCloseSiteEditor();
			}
		: null}
/>
