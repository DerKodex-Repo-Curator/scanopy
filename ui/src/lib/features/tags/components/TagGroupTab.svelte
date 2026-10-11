<script lang="ts">
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
	import { Plus, Trash2, Edit } from 'lucide-svelte';
	import { useCurrentUserQuery } from '$lib/features/auth/queries';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import { entities, permissions, billingPlans } from '$lib/shared/stores/metadata';
	import type { TabProps } from '$lib/shared/types';
	import { isSystemGroup, tagGroupIcon, tagGroupLabel, tagsInGroup } from '../groups';
	import { tagItems } from '../columns';
	import { modalState, resolveModalDeepLink } from '$lib/shared/stores/modal-registry';
	import {
		common_confirmBulkDelete,
		common_confirmDeleteName,
		common_create,
		common_created,
		common_custom,
		common_description,
		common_delete,
		common_edit,
		common_name,
		common_system,
		common_tags,
		common_type,
		common_updated,
		tags_appliesTo,
		tags_appliesToEvery,
		tags_orderByName,
		tags_orderSet,
		tags_tagOrder,
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

	function scopeNames(group: TagGroup): string[] {
		return group.entity_types
			? group.entity_types.map((type) => entities.getMetadata(type)?.entity_name_plural ?? type)
			: [tags_appliesToEvery()];
	}

	let fields = $derived<FieldConfig<TagGroup>[]>([
		{
			key: 'name',
			label: common_name(),
			type: 'string',
			searchable: true,
			sortable: true,
			getValue: (group) => tagGroupLabel(group),
			display: {
				primary: true,
				width: 220,
				getItems: (group) => [
					{
						id: group.id,
						label: tagGroupLabel(group) ?? group.name,
						icon: tagGroupIcon(group) ?? undefined
					}
				]
			}
		},
		{
			key: 'description',
			label: common_description(),
			type: 'string',
			searchable: true,
			getValue: (group) => group.description
		},
		{
			key: 'group_type',
			label: common_type(),
			type: 'string',
			sortable: true,
			filterable: true,
			groupable: true,
			getValue: (group) => (isSystemGroup(group) ? common_system() : common_custom())
		},
		{
			key: 'ordered',
			label: tags_tagOrder(),
			type: 'string',
			sortable: true,
			filterable: true,
			groupable: true,
			getValue: (group) => (group.ordered ? tags_orderSet() : tags_orderByName())
		},
		{
			key: 'entity_types',
			label: tags_appliesTo(),
			type: 'array',
			filterable: true,
			getValue: (group) => scopeNames(group)
		},
		{
			key: 'tags',
			label: common_tags(),
			type: 'array',
			searchable: true,
			filterable: true,
			getValue: (group) => tagsInGroup(tags, group.id).map((tag) => tag.name),
			display: {
				getItems: (group) =>
					tagItems(
						tagsInGroup(tags, group.id).map((tag) => tag.id),
						tags,
						groups
					)
			}
		},
		{
			key: 'created_at',
			label: common_created(),
			type: 'date',
			sortable: true,
			display: { hiddenByDefault: true }
		},
		{
			key: 'updated_at',
			label: common_updated(),
			type: 'date',
			sortable: true,
			display: { hiddenByDefault: true }
		}
	]);
</script>

{#snippet toolbarActions()}
	{#if canManage}
		<button class="btn-primary toolbar-control flex items-center" onclick={handleCreate}>
			<Plus class="h-5 w-5" />{common_create()}
		</button>
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
