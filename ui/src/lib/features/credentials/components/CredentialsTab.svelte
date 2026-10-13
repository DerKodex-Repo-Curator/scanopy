<script lang="ts">
	import CreateButton from '$lib/shared/components/data/controls/CreateButton.svelte';
	import {
		useCredentialsQuery,
		useCreateCredentialMutation,
		useUpdateCredentialMutation,
		useDeleteCredentialMutation,
		useBulkDeleteCredentialsMutation
	} from '../queries';
	import CredentialEditModal from './CredentialEditModal.svelte';
	import Loading from '$lib/shared/components/feedback/Loading.svelte';
	import EmptyState from '$lib/shared/components/layout/EmptyState.svelte';
	import type { Credential } from '../types/base';
	import type { CredentialOrderField } from '../types/base';
	import DataControls from '$lib/shared/components/data/DataControls.svelte';
	import type { TableDefaults } from '$lib/shared/components/data/types';
	import type { CardAction, CardFieldItem } from '$lib/shared/components/data/types';
	import { credentialColumns } from './CredentialsTab.columns';
	import { collectionLookup } from '$lib/shared/entity-fields';
	import type { Site } from '$lib/features/sites/types';
	import { Trash2, Edit } from 'lucide-svelte';
	import { useCurrentUserQuery } from '$lib/features/auth/queries';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import { permissions, credentialTypes, billingPlans } from '$lib/shared/stores/metadata';
	import { getCredentialTypeId } from '$lib/features/credentials/types/base';
	import { modalState, resolveModalDeepLink } from '$lib/shared/stores/modal-registry';
	import type { TabProps } from '$lib/shared/types';
	import { downloadCsv } from '$lib/shared/utils/csvExport';
	import { useSitesQuery } from '$lib/features/sites/queries';
	import { useHostsByIds } from '$lib/features/hosts/queries';
	import type { Host } from '$lib/features/hosts/types/base';
	import {
		common_confirmDeleteName,
		common_create,
		common_delete,
		common_edit,
		credentials_bulkDeleteConfirm,
		credentials_bulkDeleteImpact,
		credentials_deleteImpact,
		credentials_emptySubtitle,
		credentials_subtitle,
		common_credentials,
		common_notApplicable,
		common_noEntityYet
	} from '$lib/paraglide/messages';
	import { useTagsQuery } from '$lib/features/tags/queries';

	let { isReadOnly = false }: TabProps = $props();

	let showCredentialEditor = $state(false);
	let editingCredential: Credential | null = $state(null);

	// Deep-link: open credential editor from URL
	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'credential-editor',
			credentials,
			showCredentialEditor,
			editingCredential?.id
		);
		if (result !== undefined) {
			editingCredential = result;
			showCredentialEditor = true;
		}
	});

	// Queries and mutations
	const currentUserQuery = useCurrentUserQuery();
	let currentUser = $derived(currentUserQuery.data);

	const organizationQuery = useOrganizationQuery();
	let organization = $derived(organizationQuery.data);

	const credentialsQuery = useCredentialsQuery();
	const createCredentialMutation = useCreateCredentialMutation();
	const updateCredentialMutation = useUpdateCredentialMutation();
	const deleteCredentialMutation = useDeleteCredentialMutation();
	const bulkDeleteCredentialsMutation = useBulkDeleteCredentialsMutation();

	const tagsQuery = useTagsQuery();
	let tagsData = $derived(tagsQuery.data ?? []);

	// Sites for delete impact preview
	const sitesQuery = useSitesQuery();
	let sitesData = $derived(sitesQuery.data ?? []);

	// Derived state
	let credentials = $derived(credentialsQuery.data ?? []);

	// Which hosts a credential is assigned to is already on the credential —
	// `host_assignments` is hydrated from the same `host_credentials` junction
	// table that produces the host's `credential_assignments`. So the impact
	// counts need no host data at all, and the hosts column's chips only need those
	// ids resolved to names.
	//
	// This was `useHostsQuery({ limit: 0 })`: every host in the organisation,
	// unpaginated (~1.9MB on a 440-host estate), to label a few chips and put two
	// numbers in a confirm() dialog. Because TanStack dedupes by key it was shared
	// with every other consumer, so it loaded on pages that never showed a
	// credential.
	let assignedHostIds = $derived([
		...new Set(credentials.flatMap((c) => (c.host_assignments ?? []).map((a) => a.host_id)))
	]);
	const assignedHostsQuery = useHostsByIds(() => assignedHostIds);
	let assignedHostsData = $derived(assignedHostsQuery.data ?? []);

	function hostsForCredential(credential: Credential): Host[] {
		const ids = new Set((credential.host_assignments ?? []).map((a) => a.host_id));
		return assignedHostsData.filter((h) => ids.has(h.id));
	}

	function sitesForCredential(credential: Credential): Site[] {
		return sitesData.filter((n) => (n.credential_ids ?? []).includes(credential.id));
	}

	/** The scopes a credential's type can target at all. */
	function targetsFor(credential: Credential): string[] {
		return credentialTypes.getMetadata(getCredentialTypeId(credential))?.targets ?? [];
	}

	/** Reaches a host only as some daemon's host, never hosts in general. */
	function daemonHostOnly(credential: Credential): boolean {
		const targets = targetsFor(credential);
		return targets.includes('DaemonHost') && !targets.includes('Hosts');
	}

	/**
	 * "Not applicable" is not the same as "none assigned".
	 *
	 * A Docker socket credential cannot target sites at all, which is a
	 * different statement from a SNMP credential that targets sites and
	 * happens to have none, so an out-of-scope assignment column says so rather
	 * than sitting empty.
	 */
	function assignmentItems(applicable: boolean, items: CardFieldItem[]): CardFieldItem[] {
		if (applicable) return items;
		return [{ id: 'not-applicable', label: common_notApplicable(), color: 'Gray' }];
	}
	let isLoading = $derived(credentialsQuery.isLoading);

	// Demo mode check
	let isDemoOrg = $derived(
		billingPlans.getMetadata(organization?.plan?.type ?? null).is_demo === true
	);
	let isNonOwnerInDemo = $derived(isDemoOrg && currentUser?.permissions !== 'Owner');

	let canManage = $derived(
		!isReadOnly &&
			!isNonOwnerInDemo &&
			currentUser &&
			permissions.getMetadata(currentUser.permissions).manage_org_entities
	);

	let allowBulkDelete = $derived(
		!isReadOnly && !isNonOwnerInDemo && currentUser
			? permissions.getMetadata(currentUser.permissions).manage_org_entities
			: false
	);

	function handleCreateCredential() {
		editingCredential = null;
		showCredentialEditor = true;
	}

	/** Row actions. */
	function credentialActions(credential: Credential): CardAction[] {
		if (!canManage) return [];

		const edit: CardAction = {
			label: common_edit(),
			icon: Edit,
			onClick: () => handleEditCredential(credential)
		};
		// A default can't be deleted, only unassigned from its sites and hosts.
		if (credential.is_default) return [edit];
		return [
			edit,
			{
				label: common_delete(),
				icon: Trash2,
				class: 'btn-icon-danger',
				onClick: () => handleDeleteCredential(credential)
			}
		];
	}

	function handleEditCredential(credential: Credential) {
		editingCredential = credential;
		showCredentialEditor = true;
	}

	async function handleDeleteCredential(credential: Credential) {
		const affectedSites = sitesData.filter((n) => (n.credential_ids ?? []).includes(credential.id));
		const affectedHostCount = (credential.host_assignments ?? []).length;
		let message: string = common_confirmDeleteName({ name: credential.name });
		if (affectedSites.length > 0 || affectedHostCount > 0) {
			message +=
				'\n\n' +
				credentials_deleteImpact({
					siteCount: affectedSites.length,
					hostCount: affectedHostCount
				});
		}
		if (confirm(message)) {
			await deleteCredentialMutation.mutateAsync(credential.id);
		}
	}

	async function handleCredentialCreate(data: Credential) {
		await createCredentialMutation.mutateAsync(data);
		showCredentialEditor = false;
		editingCredential = null;
	}

	async function handleCredentialUpdate(_id: string, data: Credential) {
		await updateCredentialMutation.mutateAsync(data);
		showCredentialEditor = false;
		editingCredential = null;
	}

	function handleCloseCredentialEditor() {
		showCredentialEditor = false;
		editingCredential = null;
	}

	async function handleBulkDelete(ids: string[]) {
		const affectedSites = sitesData.filter((n) =>
			(n.credential_ids ?? []).some((id) => ids.includes(id))
		);
		// Distinct hosts across the selected credentials — a host assigned two of
		// them must not be counted twice.
		const affectedHostCount = new Set(
			credentials
				.filter((c) => ids.includes(c.id))
				.flatMap((c) => (c.host_assignments ?? []).map((a) => a.host_id))
		).size;
		let message: string = credentials_bulkDeleteConfirm({ count: ids.length });
		if (affectedSites.length > 0 || affectedHostCount > 0) {
			message +=
				'\n\n' +
				credentials_bulkDeleteImpact({
					siteCount: affectedSites.length,
					hostCount: affectedHostCount
				});
		}
		if (confirm(message)) {
			await bulkDeleteCredentialsMutation.mutateAsync(ids);
		}
	}

	// CSV export handler
	async function handleCsvExport() {
		await downloadCsv('Credential', {});
	}

	function getCredentialTags(credential: Credential): string[] {
		return credential.tags;
	}

	const tableDefaults: TableDefaults<CredentialOrderField | 'credential_type' | 'integration'> = {
		group: 'integration',
		sort: { field: 'name', direction: 'asc' }
	};

	// The registry's credential fields are all the credential's own, so the lookup holds nothing.
	const lookup = collectionLookup({});

	const credentialFields = credentialColumns({
		lookup: () => lookup,
		tags: () => tagsData,
		sitesForCredential,
		hostsForCredential,
		targetsFor,
		daemonHostOnly,
		assignmentItems
	});
</script>

<!-- The page's own actions, last in the table toolbar beside the filter and columns. -->
{#snippet toolbarActions()}
	{#if canManage}
		<CreateButton onclick={handleCreateCredential} />
	{/if}
{/snippet}

<div class="space-y-6">
	{#if isLoading}
		<Loading />
	{:else if credentials.length === 0}
		<EmptyState
			title={common_noEntityYet({ entity: common_credentials() })}
			subtitle={credentials_emptySubtitle()}
			onClick={canManage ? handleCreateCredential : undefined}
			cta={canManage ? common_create() : ''}
		/>
	{:else}
		<DataControls
			title={common_credentials()}
			subtitle={credentials_subtitle()}
			{toolbarActions}
			items={credentials}
			fields={credentialFields}
			{allowBulkDelete}
			canDeleteItem={(item) => !item.is_default}
			storageKey="scanopy-credentials-table-state"
			defaults={tableDefaults}
			onBulkDelete={handleBulkDelete}
			entityType="Credential"
			canEditTags={allowBulkDelete}
			getItemTags={getCredentialTags}
			getItemId={(item) => item.id}
			onCsvExport={handleCsvExport}
			getActions={credentialActions}
			entityLabel={common_credentials()}
		></DataControls>
	{/if}
</div>

<CredentialEditModal
	name="credential-editor"
	isOpen={showCredentialEditor}
	credential={editingCredential}
	onCreate={handleCredentialCreate}
	onUpdate={handleCredentialUpdate}
	onClose={handleCloseCredentialEditor}
	onDelete={editingCredential && !editingCredential.is_default
		? () => {
				handleDeleteCredential(editingCredential!);
				handleCloseCredentialEditor();
			}
		: null}
/>
