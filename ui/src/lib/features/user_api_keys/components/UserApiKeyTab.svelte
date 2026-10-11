<script lang="ts">
	import { Edit, Trash2 } from 'lucide-svelte';
	import type { CardAction } from '$lib/shared/components/data/types';
	import Loading from '$lib/shared/components/feedback/Loading.svelte';
	import EmptyState from '$lib/shared/components/layout/EmptyState.svelte';
	import DataControls from '$lib/shared/components/data/DataControls.svelte';
	import type { TableDefaults } from '$lib/shared/components/data/types';
	import { userApiKeyColumns } from './UserApiKeyTab.columns';
	import type { FieldConfig } from '$lib/shared/components/data/types';
	import { Plus } from 'lucide-svelte';
	import { useCurrentUserQuery } from '$lib/features/auth/queries';
	import { tooltip } from '$lib/shared/actions/tooltip';
	import { useTagsQuery } from '$lib/features/tags/queries';
	import { useSitesQuery } from '$lib/features/sites/queries';
	import UserApiKeyModal from './UserApiKeyModal.svelte';
	import {
		useUserApiKeysQuery,
		useUpdateUserApiKeyMutation,
		useDeleteUserApiKeyMutation,
		useBulkDeleteUserApiKeysMutation,
		type UserApiKey
	} from '../queries';
	import type { TabProps } from '$lib/shared/types';
	import { downloadCsv } from '$lib/shared/utils/csvExport';

	import {
		common_edit,
		common_delete,
		common_apiKeys,
		common_confirmBulkDelete,
		common_confirmDeleteName,
		common_create,
		userApiKeys_apiAccessUnavailableSubtitle,
		userApiKeys_apiAccessUnavailableTitle,
		userApiKeys_noApiKeysSubtitle,
		userApiKeys_noApiKeysYet,
		userApiKeys_subtitle,
		userApiKeys_verifyEmailToCreate
	} from '$lib/paraglide/messages';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import { billingPlans } from '$lib/shared/stores/metadata';
	import UpgradeButton from '$lib/shared/components/UpgradeButton.svelte';
	import { modalState, resolveModalDeepLink } from '$lib/shared/stores/modal-registry';

	let { isReadOnly = false }: TabProps = $props();

	const currentUserQuery = useCurrentUserQuery();
	let currentUser = $derived(currentUserQuery.data);
	let isEmailVerified = $derived(currentUser?.email_verified ?? true);

	// Check if plan has api_access feature before querying
	const organizationQuery = useOrganizationQuery();
	let hasApiAccess = $derived.by(() => {
		const org = organizationQuery.data;
		if (!org?.plan) return false;
		return billingPlans.getMetadata(org.plan.type).features.api_access;
	});

	// Queries
	const tagsQuery = useTagsQuery();
	const userApiKeysQuery = useUserApiKeysQuery({ enabled: () => hasApiAccess });
	const sitesQuery = useSitesQuery();

	// Mutations
	const updateMutation = useUpdateUserApiKeyMutation();
	const deleteMutation = useDeleteUserApiKeyMutation();
	const bulkDeleteMutation = useBulkDeleteUserApiKeysMutation();

	// Derived data
	let tagsData = $derived(tagsQuery.data ?? []);
	let userApiKeysData = $derived(userApiKeysQuery.data ?? []);
	let sitesData = $derived(sitesQuery.data ?? []);
	let isLoading = $derived(userApiKeysQuery.isPending);

	let showModal = $state(false);
	let editingApiKey = $state<UserApiKey | null>(null);

	// Deep-link: open user API key editor from URL (handles both fresh open and entity switch)
	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'user-api-key',
			userApiKeysData,
			showModal,
			editingApiKey?.id
		);
		if (result !== undefined) {
			editingApiKey = result;
			showModal = true;
		}
	});

	async function handleDelete(apiKey: UserApiKey) {
		if (confirm(common_confirmDeleteName({ name: apiKey.name }))) {
			deleteMutation.mutate(apiKey.id);
		}
	}

	async function handleUpdate(apiKey: UserApiKey) {
		await updateMutation.mutateAsync(apiKey);
		showModal = false;
		editingApiKey = null;
	}

	function handleCreate() {
		showModal = true;
		editingApiKey = null;
	}

	function handleClose() {
		showModal = false;
		editingApiKey = null;
	}

	function handleEdit(apiKey: UserApiKey) {
		showModal = true;
		editingApiKey = apiKey;
	}

	async function handleBulkDelete(ids: string[]) {
		if (confirm(common_confirmBulkDelete({ count: ids.length, entity: common_apiKeys() }))) {
			await bulkDeleteMutation.mutateAsync(ids);
		}
	}

	function getUserApiKeyTags(apiKey: UserApiKey): string[] {
		return apiKey.tags ?? [];
	}

	// CSV export handler
	async function handleCsvExport() {
		await downloadCsv('UserApiKey', {});
	}

	/** Row actions. */
	function userApiKeyActions(apiKey: UserApiKey): CardAction[] {
		return [
			{ label: common_edit(), icon: Edit, onClick: () => handleEdit(apiKey) },
			{
				label: common_delete(),
				icon: Trash2,
				class: 'btn-icon-danger',
				onClick: () => handleDelete(apiKey)
			}
		];
	}

	const tableDefaults: TableDefaults<string> = { sort: { field: 'name', direction: 'asc' } };

	const apiKeyFields: FieldConfig<UserApiKey>[] = userApiKeyColumns({
		sites: () => sitesData,
		tags: () => tagsData
	});
</script>

<!-- The page's own actions, last in the table toolbar beside the filter and columns. -->
{#snippet toolbarActions()}
	{#if !isReadOnly && hasApiAccess}
		{#if !isEmailVerified}
			<span data-tooltip={userApiKeys_verifyEmailToCreate()} use:tooltip>
				<button class="btn-primary toolbar-control flex items-center opacity-50" disabled>
					<Plus class="h-5 w-5" />{common_create()}
				</button>
			</span>
		{:else}
			<button class="btn-primary toolbar-control flex items-center" onclick={handleCreate}>
				<Plus class="h-5 w-5" />{common_create()}
			</button>
		{/if}
	{/if}
{/snippet}

<div class="space-y-6">
	{#if !hasApiAccess}
		<EmptyState
			title={userApiKeys_apiAccessUnavailableTitle()}
			subtitle={userApiKeys_apiAccessUnavailableSubtitle()}
		>
			<UpgradeButton feature="api_access" surface="api_keys_tab" />
		</EmptyState>
	{:else if isLoading}
		<Loading />
	{:else if userApiKeysData.length === 0}
		<EmptyState
			title={userApiKeys_noApiKeysYet()}
			subtitle={userApiKeys_noApiKeysSubtitle()}
			onClick={isEmailVerified ? handleCreate : undefined}
			cta={isEmailVerified ? common_create() : undefined}
		/>
	{:else}
		<DataControls
			title={common_apiKeys()}
			subtitle={userApiKeys_subtitle()}
			{toolbarActions}
			items={userApiKeysData}
			fields={apiKeyFields}
			onBulkDelete={isReadOnly ? undefined : handleBulkDelete}
			entityType="UserApiKey"
			canEditTags={!isReadOnly}
			getItemTags={getUserApiKeyTags}
			storageKey="scanopy-user-api-keys-table-state"
			defaults={tableDefaults}
			getItemId={(item) => item.id}
			getActions={userApiKeyActions}
			onCsvExport={handleCsvExport}
		></DataControls>
	{/if}
</div>

<UserApiKeyModal
	name="user-api-key"
	isOpen={showModal}
	onClose={handleClose}
	onUpdate={handleUpdate}
	apiKey={editingApiKey}
/>
