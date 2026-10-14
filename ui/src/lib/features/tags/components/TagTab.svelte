<script lang="ts">
	import CreateButton from '$lib/shared/components/data/controls/CreateButton.svelte';
	import {
		useTagsQuery,
		useTagGroupsQuery,
		useCreateTagMutation,
		useUpdateTagMutation,
		useDeleteTagMutation,
		useBulkDeleteTagsMutation
	} from '../queries';
	import TagEditModal from './TagEditModal.svelte';
	import Loading from '$lib/shared/components/feedback/Loading.svelte';
	import EmptyState from '$lib/shared/components/layout/EmptyState.svelte';
	import type { Tag } from '../types/base';
	import DataControls from '$lib/shared/components/data/DataControls.svelte';
	import type { TableDefaults } from '$lib/shared/components/data/types';
	import type { CardAction } from '$lib/shared/components/data/types';
	import { tagColumns } from './TagTab.columns';
	import { collectionLookup } from '$lib/shared/entity-fields';
	import { Trash2, Edit } from 'lucide-svelte';
	import { useCurrentUserQuery } from '$lib/features/auth/queries';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import { permissions, billingPlans } from '$lib/shared/stores/metadata';
	import type { TabProps } from '$lib/shared/types';
	import { applicationGroupId } from '../groups';
	import type { components } from '$lib/api/schema';
	import { downloadCsv } from '$lib/shared/utils/csvExport';
	import { modalState, resolveModalDeepLink } from '$lib/shared/stores/modal-registry';
	import {
		common_confirmBulkDelete,
		common_confirmDeleteName,
		common_create,
		common_delete,
		common_edit,
		common_noEntityYet,
		common_tags,
		tags_noTagsHelp,
		tags_subtitle
	} from '$lib/paraglide/messages';

	type TagOrderField = components['schemas']['TagOrderField'];

	let { isReadOnly = false }: TabProps = $props();

	let showTagEditor = $state(false);
	let editingTag: Tag | null = $state(null);

	// Deep-link: open tag editor from URL (handles both fresh open and entity switch)
	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'tag-editor',
			tags,
			showTagEditor,
			editingTag?.id
		);
		if (result !== undefined) {
			editingTag = result;
			showTagEditor = true;
		}
	});

	// Queries and mutations
	const currentUserQuery = useCurrentUserQuery();
	let currentUser = $derived(currentUserQuery.data);

	const organizationQuery = useOrganizationQuery();
	let organization = $derived(organizationQuery.data);

	// Demo orgs are read-only for non-owners (mirrors the credentials tab)
	let isDemoOrg = $derived(
		billingPlans.getMetadata(organization?.plan?.type ?? null).is_demo === true
	);
	let isNonOwnerInDemo = $derived(isDemoOrg && currentUser?.permissions !== 'Owner');

	const tagsQuery = useTagsQuery();
	const tagGroupsQuery = useTagGroupsQuery();
	let tagGroups = $derived(tagGroupsQuery.data ?? []);
	let applicationGroup = $derived(applicationGroupId(tagGroups));
	const createTagMutation = useCreateTagMutation();
	const updateTagMutation = useUpdateTagMutation();
	const deleteTagMutation = useDeleteTagMutation();
	const bulkDeleteTagsMutation = useBulkDeleteTagsMutation();

	// Derived state
	let tags = $derived(tagsQuery.data ?? []);
	let isLoading = $derived(tagsQuery.isLoading);

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

	function handleCreateTag() {
		editingTag = null;
		showTagEditor = true;
	}

	/** Row actions. */
	function tagActions(tag: Tag): CardAction[] {
		if (!canManage) return [];

		return [
			{ label: common_edit(), icon: Edit, onClick: () => handleEditTag(tag) },
			{
				label: common_delete(),
				icon: Trash2,
				class: 'btn-icon-danger',
				onClick: () => handleDeleteTag(tag)
			}
		];
	}

	function handleEditTag(tag: Tag) {
		editingTag = tag;
		showTagEditor = true;
	}

	async function handleDeleteTag(tag: Tag) {
		if (confirm(common_confirmDeleteName({ name: tag.name }))) {
			await deleteTagMutation.mutateAsync(tag.id);
		}
	}

	async function handleTagCreate(data: Tag) {
		await createTagMutation.mutateAsync(data);
		showTagEditor = false;
		editingTag = null;
	}

	async function handleTagUpdate(_id: string, data: Tag) {
		await updateTagMutation.mutateAsync(data);
		showTagEditor = false;
		editingTag = null;
	}

	function handleCloseTagEditor() {
		showTagEditor = false;
		editingTag = null;
	}

	async function handleBulkDelete(ids: string[]) {
		if (confirm(common_confirmBulkDelete({ count: ids.length, entity: common_tags() }))) {
			await bulkDeleteTagsMutation.mutateAsync(ids);
		}
	}

	// CSV export handler
	async function handleCsvExport() {
		await downloadCsv('Tag', {});
	}

	const tableDefaults: TableDefaults<TagOrderField> = {
		group: 'tag_group',
		// Within a group, the group's own order.
		sort: { field: 'tag_group', direction: 'asc' }
	};

	let lookup = $derived(collectionLookup({ TagGroup: tagGroups }));
	let tagFields = $derived(
		tagColumns({
			lookup: () => lookup,
			tagGroups: () => tagGroups,
			applicationGroup: () => applicationGroup
		})
	);
</script>

<!-- The page's own actions, last in the table toolbar beside the filter and columns. -->
{#snippet toolbarActions()}
	{#if canManage}
		<CreateButton onclick={handleCreateTag} />
	{/if}
{/snippet}

<div class="space-y-6">
	{#if isLoading}
		<Loading />
	{:else if tags.length === 0}
		<EmptyState
			title={common_noEntityYet({ entity: common_tags() })}
			subtitle={tags_noTagsHelp()}
			onClick={canManage ? handleCreateTag : undefined}
			cta={canManage ? common_create() : ''}
		/>
	{:else}
		<DataControls
			title={common_tags()}
			subtitle={tags_subtitle()}
			{toolbarActions}
			items={tags}
			fields={tagFields}
			{allowBulkDelete}
			storageKey="scanopy-tags-table-state"
			defaults={tableDefaults}
			onBulkDelete={handleBulkDelete}
			getItemId={(item) => item.id}
			onCsvExport={handleCsvExport}
			getActions={tagActions}
			entityLabel={common_tags()}
		></DataControls>
	{/if}
</div>

<TagEditModal
	name="tag-editor"
	isOpen={showTagEditor}
	tag={editingTag}
	onCreate={handleTagCreate}
	onUpdate={handleTagUpdate}
	onClose={handleCloseTagEditor}
	onDelete={editingTag ? () => handleDeleteTag(editingTag!) : null}
/>
