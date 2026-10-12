<script lang="ts">
	import Loading from '$lib/shared/components/feedback/Loading.svelte';
	import EmptyState from '$lib/shared/components/layout/EmptyState.svelte';
	import PreDaemonEmptyState from '$lib/shared/components/layout/PreDaemonEmptyState.svelte';
	import DataControls from '$lib/shared/components/data/DataControls.svelte';
	import type { TableDefaults } from '$lib/shared/components/data/types';
	import { vlanColumns } from './VlanTab.columns';
	import { collectionLookup } from '$lib/shared/entity-fields';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import { useSitesQuery } from '$lib/features/sites/queries';
	import { isUserManagedSubnet, useSubnetsQuery } from '$lib/features/subnets/queries';
	import { useUpdateVlanMutation, useVlansQuery } from '../queries';
	import VlanEditModal from './VlanEditModal.svelte';
	import { Edit } from 'lucide-svelte';
	import type { CardAction } from '$lib/shared/components/data/types';
	import type { TabProps } from '$lib/shared/types';
	import { modalState, resolveModalDeepLink } from '$lib/shared/stores/modal-registry';
	import { useDiscoveriesByIds } from '$lib/features/discovery/queries';
	import { useInterfacesByIds } from '$lib/features/interfaces/queries';
	import { useTagsQuery } from '$lib/features/tags/queries';
	import { discoveryRunIds } from '$lib/features/discovery/columns';
	import type { Vlan, VlanOrderField } from '../types/base';
	import type { components } from '$lib/api/schema';
	import { downloadCsv } from '$lib/shared/utils/csvExport';
	import {
		common_edit,
		common_noEntityYet,
		common_vlans,
		daemons_installPromptVlans,
		vlans_emptySubtitle
	} from '$lib/paraglide/messages';
	import { hasDaemon } from '$lib/shared/onboarding/checklist';

	type OnboardingOperation = components['schemas']['OnboardingOperationDiscriminants'];

	let { isReadOnly = false }: TabProps = $props();

	let showVlanEditor = $state(false);
	let editingVlan: Vlan | null = $state(null);

	// Deep-link: open the VLAN editor from the URL (fresh open and entity switch)
	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'vlan-editor',
			vlansData,
			showVlanEditor,
			editingVlan?.id
		);
		if (result) {
			editingVlan = result;
			showVlanEditor = true;
		}
	});

	// Organization query for onboarding state
	const organizationQuery = useOrganizationQuery();
	let onboarding = $derived((organizationQuery.data?.onboarding ?? []) as OnboardingOperation[]);

	// Queries
	const vlansQuery = useVlansQuery();
	const sitesQuery = useSitesQuery();
	// Shared full-list subnets cache — used to resolve the hydrated `subnet_ids`
	// on each VLAN into names.
	const subnetsQuery = useSubnetsQuery();
	const tagsQuery = useTagsQuery();
	const discoveryRunsQuery = useDiscoveriesByIds(() => discoveryRunIds(vlansQuery.data ?? []));
	// The interfaces serving each VLAN, named by the hydrated `interface_ids`.
	const interfacesQuery = useInterfacesByIds(() => [
		...new Set((vlansQuery.data ?? []).flatMap((vlan) => vlan.interface_ids ?? []))
	]);

	// Derived data
	let vlansData = $derived(vlansQuery.data ?? []);
	let sitesData = $derived(sitesQuery.data ?? []);
	let discoveryRunsData = $derived(discoveryRunsQuery.data ?? []);
	let interfacesData = $derived(interfacesQuery.data ?? []);
	let tagsData = $derived(tagsQuery.data ?? []);
	let subnetsById = $derived(
		new Map((subnetsQuery.data ?? []).filter(isUserManagedSubnet).map((s) => [s.id, s]))
	);
	let isLoading = $derived(vlansQuery.isPending);

	const updateVlanMutation = useUpdateVlanMutation();

	/** Row actions. VLANs are discovered, so editing is the only action. */
	function vlanActions(vlan: Vlan): CardAction[] {
		if (isReadOnly) return [];
		return [{ label: common_edit(), icon: Edit, onClick: () => handleEditVlan(vlan) }];
	}

	function handleEditVlan(vlan: Vlan) {
		editingVlan = vlan;
		showVlanEditor = true;
	}

	async function handleVlanUpdate(data: Vlan) {
		try {
			await updateVlanMutation.mutateAsync(data);
			handleCloseVlanEditor();
		} catch {
			// Error handled by mutation
		}
	}

	function handleCloseVlanEditor() {
		showVlanEditor = false;
		editingVlan = null;
	}

	function getVlanTags(vlan: Vlan): string[] {
		return vlan.tags;
	}

	// CSV export handler
	async function handleCsvExport() {
		await downloadCsv('Vlan', {});
	}

	const tableDefaults: TableDefaults<VlanOrderField> = {
		sort: { field: 'vlan_number', direction: 'asc' }
	};

	// Related entities the registry's VLAN fields read: the subnets and interfaces a VLAN
	// carries, the site.
	let lookup = $derived(
		collectionLookup({
			Subnet: [...subnetsById.values()],
			Interface: interfacesData,
			Site: sitesData
		})
	);

	let vlanFields = $derived(
		vlanColumns({
			lookup: () => lookup,
			sites: () => sitesData,
			tags: () => tagsData,
			discoveryRuns: () => discoveryRunsData
		})
	);
</script>

<div class="space-y-6">
	<!-- Header: no actions — VLANs are discovery-populated, so there is nothing to create -->

	{#if !hasDaemon(onboarding)}
		<PreDaemonEmptyState title={daemons_installPromptVlans()} />
	{:else if isLoading}
		<!-- Loading state -->
		<Loading />
	{:else if vlansData.length === 0}
		<!-- Empty state: no CTA, there is nothing to create -->
		<EmptyState
			title={common_noEntityYet({ entity: common_vlans() })}
			subtitle={vlans_emptySubtitle()}
		/>
	{:else}
		<DataControls
			title={common_vlans()}
			items={vlansData}
			fields={vlanFields}
			storageKey="scanopy-vlans-table-state"
			defaults={tableDefaults}
			entityType="Vlan"
			canEditTags={!isReadOnly}
			getItemTags={getVlanTags}
			getItemId={(item) => item.id}
			onCsvExport={handleCsvExport}
			getActions={vlanActions}
			entityLabel={common_vlans()}
		></DataControls>
	{/if}
</div>

<VlanEditModal
	name="vlan-editor"
	isOpen={showVlanEditor}
	vlan={editingVlan}
	onUpdate={handleVlanUpdate}
	onClose={handleCloseVlanEditor}
/>
