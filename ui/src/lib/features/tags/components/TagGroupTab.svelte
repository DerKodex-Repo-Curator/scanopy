<script lang="ts">
	import CreateButton from '$lib/shared/components/data/controls/CreateButton.svelte';
	import {
		useTagsQuery,
		useTagGroupsQuery,
		useCreateTagGroupMutation,
		useUpdateTagGroupMutation,
		useDeleteTagGroupMutation,
		useBulkDeleteTagGroupsMutation,
		useSetTagOrderMutation
	} from '../queries';
	import TagGroupEditModal from './TagGroupEditModal.svelte';
	import Loading from '$lib/shared/components/feedback/Loading.svelte';
	import type { TagGroup } from '../types/base';
	import DataControls from '$lib/shared/components/data/DataControls.svelte';
	import type { CardAction, FieldConfig } from '$lib/shared/components/data/types';
	import { Trash2, Edit } from 'lucide-svelte';
	import { useCurrentUserQuery } from '$lib/features/auth/queries';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import { permissions, billingPlans } from '$lib/shared/stores/metadata';
	import type { TabProps } from '$lib/shared/types';
	import { isSystemGroup } from '../groups';
	import { tagGroupColumns } from './TagGroupTab.columns';
	import { modalState, resolveModalDeepLink } from '$lib/shared/stores/modal-registry';
	import {
		common_confirmBulkDelete,
		common_confirmDeleteName,
		common_delete,
		common_edit,
		tags_tagGroups,
		tags_tagGroupsSubtitle
	} from '$lib/paraglide/messages';

	let { isReadOnly = false }: TabProps = $props();

	let showEditor = $state(false);
	let editingGroup: TagGroup | null = $state(null);

	$effect(() => {
		const result = resolveModalDeepLink(
			$modalState,
			'tag-group-editor',
			groups,
			showEditor,
			editingGroup?.id
		);
		if (result !== undefined) {
			editingGroup = result;
			showEditor = true;
		}
	});

	const currentUserQuery = useCurrentUserQuery();
	let currentUser = $derived(currentUserQuery.data);
	const organizationQuery = useOrganizationQuery();
	let organization = $derived(organizationQuery.data);

	// Demo orgs are read-only for non-owners (mirrors the tags tab)
	let isNonOwnerInDemo = $derived(
		billingPlans.getMetadata(organization?.plan?.type ?? null).is_demo === true &&
			currentUser?.permissions !== 'Owner'
	);

	const tagsQuery = useTagsQuery();
	const tagGroupsQuery = useTagGroupsQuery();
	const createMutation = useCreateTagGroupMutation();
	const updateMutation = useUpdateTagGroupMutation();
	const deleteMutation = useDeleteTagGroupMutation();
	const bulkDeleteMutation = useBulkDeleteTagGroupsMutation();
	const setTagOrderMutation = useSetTagOrderMutation();

	let tags = $derived(tagsQuery.data ?? []);
	let groups = $derived(tagGroupsQuery.data ?? []);
	let isLoading = $derived(tagGroupsQuery.isLoading);

	let canManage = $derived(
		!isReadOnly &&
			!isNonOwnerInDemo &&
			!!currentUser &&
			permissions.getMetadata(currentUser.permissions).manage_org_entities
	);

	function handleCreate() {
		editingGroup = null;
		showEditor = true;
	}

	function handleEdit(group: TagGroup) {
		editingGroup = group;
		showEditor = true;
	}

	function close() {
		showEditor = false;
		editingGroup = null;
	}

	async function handleDelete(group: TagGroup) {
		if (confirm(common_confirmDeleteName({ name: group.name }))) {
			await deleteMutation.mutateAsync(group.id);
			close();
		}
	}

	function actions(group: TagGroup): CardAction[] {
		if (!canManage) return [];
		const edit = { label: common_edit(), icon: Edit, onClick: () => handleEdit(group) };
		if (isSystemGroup(group)) return [edit];
		return [
			edit,
			{
				label: common_delete(),
				icon: Trash2,
				class: 'btn-icon-danger',
				onClick: () => handleDelete(group)
			}
		];
	}

	async function handleCreateSubmit(data: TagGroup) {
		await createMutation.mutateAsync(data);
		close();
	}

	async function handleUpdateSubmit(data: TagGroup, tagIds: string[] | null) {
		await updateMutation.mutateAsync(data);
		if (tagIds) await setTagOrderMutation.mutateAsync({ groupId: data.id, tagIds });
		close();
	}

	async function handleBulkDelete(ids: string[]) {
		if (confirm(common_confirmBulkDelete({ count: ids.length, entity: tags_tagGroups() }))) {
			await bulkDeleteMutation.mutateAsync(ids);
		}
	}

	let fields = $derived<FieldConfig<TagGroup>[]>(
		tagGroupColumns({
			tags: () => tags,
			groups: () => groups
		})
	);
</script>

{#snippet toolbarActions()}
	{#if canManage}
		<CreateButton onclick={handleCreate} />
	{/if}
{/snippet}

<div class="space-y-6">
	{#if isLoading}
		<Loading />
	{:else}
		<DataControls
			title={tags_tagGroups()}
			subtitle={tags_tagGroupsSubtitle()}
			{toolbarActions}
			items={groups}
			{fields}
			allowBulkDelete={canManage}
			storageKey="scanopy-tag-groups-table-state"
			onBulkDelete={handleBulkDelete}
			getItemId={(item) => item.id}
			getActions={actions}
			entityLabel={tags_tagGroups()}
		></DataControls>
	{/if}
</div>

<TagGroupEditModal
	name="tag-group-editor"
	isOpen={showEditor}
	group={editingGroup}
	onCreate={handleCreateSubmit}
	onUpdate={handleUpdateSubmit}
	onClose={close}
	onDelete={editingGroup && !isSystemGroup(editingGroup) ? () => handleDelete(editingGroup!) : null}
/>
