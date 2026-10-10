<script lang="ts">
	import { createForm } from '@tanstack/svelte-form';
	import { submitForm } from '$lib/shared/components/forms/form-context';
	import { required, max } from '$lib/shared/components/forms/validators';
	import GenericModal from '$lib/shared/components/layout/GenericModal.svelte';
	import ModalHeaderIcon from '$lib/shared/components/layout/ModalHeaderIcon.svelte';
	import EntityMetadataSection from '$lib/shared/components/forms/EntityMetadataSection.svelte';
	import TextInput from '$lib/shared/components/forms/input/TextInput.svelte';
	import RadioGroup from '$lib/shared/components/forms/input/RadioGroup.svelte';
	import Tag from '$lib/shared/components/data/Tag.svelte';
	import { ChevronDown, ChevronUp } from 'lucide-svelte';
	import { entities } from '$lib/shared/stores/metadata';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import { pushError } from '$lib/shared/stores/feedback';
	import type { components } from '$lib/api/schema';
	import { createDefaultTagGroup, type Tag as TagType, type TagGroup } from '../types/base';
	import { applicationGroupId, isSystemGroup, tagGroupHelp, tagIcon, tagsInGroup } from '../groups';
	import { useTagGroupsQuery, useTagsQuery } from '../queries';
	import {
		common_cancel,
		common_couldNotLoadOrganization,
		common_create,
		common_delete,
		common_deleting,
		common_editName,
		common_name,
		common_saving,
		common_update,
		tags_appliesTo,
		tags_appliesToEvery,
		tags_appliesToHelp,
		tags_appliesToSome,
		tags_chooseEntityType,
		tags_createTagGroup,
		tags_groupNamePlaceholder,
		tags_moveTagDown,
		tags_moveTagUp,
		tags_noTagsInGroup,
		tags_systemGroupFixed,
		tags_tagOrder,
		tags_tagOrderHelp
	} from '$lib/paraglide/messages';

	type EntityDiscriminants = components['schemas']['EntityDiscriminants'];
	type Scope = 'every' | 'some';

	let {
		group = null,
		isOpen = false,
		onCreate,
		onUpdate,
		onClose,
		onDelete = null,
		name = undefined
	}: {
		group?: TagGroup | null;
		isOpen?: boolean;
		onCreate: (data: TagGroup) => Promise<void> | void;
		/** Saves the group, then its tag order when `tagIds` differs from the group's order. */
		onUpdate: (data: TagGroup, tagIds: string[] | null) => Promise<void> | void;
		onClose: () => void;
		onDelete?: ((id: string) => Promise<void> | void) | null;
		name?: string;
	} = $props();

	const organizationQuery = useOrganizationQuery();
	let organization = $derived(organizationQuery.data);
	const tagsQuery = useTagsQuery();
	const tagGroupsQuery = useTagGroupsQuery();
	let applicationGroup = $derived(applicationGroupId(tagGroupsQuery.data ?? []));

	/** Every entity type a tag can be applied to, from the entity metadata. */
	const taggableTypes = entities
		.getItems()
		.filter((item) => (item.metadata as { is_taggable?: boolean } | null)?.is_taggable)
		.map((item) => item.id as EntityDiscriminants);

	let loading = $state(false);
	let deleting = $state(false);

	let isEditing = $derived(group !== null);
	let isSystem = $derived(isSystemGroup(group));
	let title = $derived(
		isEditing ? common_editName({ name: group?.name ?? '' }) : tags_createTagGroup()
	);

	// Scope, chosen types and tag order are source of truth here and synced into the form:
	// TanStack's field values aren't tracked by Svelte, and these change programmatically.
	let scope = $state<Scope>('every');
	let chosenTypes = $state<EntityDiscriminants[]>([]);
	let orderedTags = $state<TagType[]>([]);
	let initialOrder = $state<string[]>([]);

	const form = createForm(() => ({
		defaultValues: createDefaultTagGroup(''),
		onSubmit: async ({ value }) => {
			if (!organization) {
				pushError(common_couldNotLoadOrganization());
				onClose();
				return;
			}
			if (scope === 'some' && chosenTypes.length === 0) {
				pushError(tags_chooseEntityType());
				return;
			}
			const data: TagGroup = {
				...(value as TagGroup),
				name: value.name.trim(),
				entity_types: scope === 'every' ? null : [...chosenTypes],
				organization_id: organization.id
			};
			const order = orderedTags.map((tag) => tag.id);
			const orderChanged = order.some((id, index) => id !== initialOrder[index]);

			loading = true;
			try {
				if (isEditing) await onUpdate(data, orderChanged ? order : null);
				else await onCreate(data);
			} finally {
				loading = false;
			}
		}
	}));

	function handleOpen() {
		const defaults = group ? { ...group } : createDefaultTagGroup(organization?.id ?? '');
		form.reset(defaults);
		scope = defaults.entity_types ? 'some' : 'every';
		chosenTypes = defaults.entity_types ? [...defaults.entity_types] : [];
		orderedTags = group ? tagsInGroup(tagsQuery.data ?? [], group.id) : [];
		initialOrder = orderedTags.map((tag) => tag.id);
	}

	function handleScopeChange(next: Scope) {
		scope = next;
		form.setFieldValue('entity_types', next === 'every' ? null : [...chosenTypes]);
	}

	function toggleType(entityType: EntityDiscriminants) {
		chosenTypes = chosenTypes.includes(entityType)
			? chosenTypes.filter((t) => t !== entityType)
			: [...chosenTypes, entityType];
		form.setFieldValue('entity_types', [...chosenTypes]);
	}

	function move(index: number, by: -1 | 1) {
		const target = index + by;
		if (target < 0 || target >= orderedTags.length) return;
		const next = [...orderedTags];
		[next[index], next[target]] = [next[target], next[index]];
		orderedTags = next;
	}

	async function handleDelete() {
		if (!onDelete || !group) return;
		deleting = true;
		try {
			await onDelete(group.id);
		} finally {
			deleting = false;
		}
	}
</script>

<GenericModal
	{isOpen}
	{title}
	{name}
	entityId={group?.id}
	{form}
	size="lg"
	{onClose}
	onOpen={handleOpen}
	showCloseButton={true}
>
	{#snippet headerIcon()}
		<ModalHeaderIcon Icon={entities.getIconComponent('TagGroup')} color="Yellow" />
	{/snippet}

	<form
		onsubmit={(e) => {
			e.preventDefault();
			e.stopPropagation();
			submitForm(form);
		}}
		class="flex min-h-0 flex-1 flex-col"
	>
		<div class="min-h-0 flex-1 overflow-auto p-6">
			<div class="space-y-8">
				<div class="space-y-4">
					<form.Field
						name="name"
						validators={{ onBlur: ({ value }) => required(value) || max(100)(value) }}
					>
						{#snippet children(field)}
							<TextInput
								label={common_name()}
								id="name"
								{field}
								placeholder={tags_groupNamePlaceholder()}
								disabled={isSystem}
								helpText={isSystem ? (tagGroupHelp(group) ?? tags_systemGroupFixed()) : ''}
								required
							/>
						{/snippet}
					</form.Field>

					<form.Field name="entity_types">
						{#snippet children(field)}
							<div class="space-y-3">
								<RadioGroup
									label={tags_appliesTo()}
									id="entity_types"
									{field}
									value={scope}
									onChange={handleScopeChange}
									variant="inline"
									options={[
										{ value: 'every', label: tags_appliesToEvery() },
										{ value: 'some', label: tags_appliesToSome() }
									]}
								/>
								{#if scope === 'some'}
									<div class="grid grid-cols-2 gap-2 sm:grid-cols-3">
										{#each taggableTypes as entityType (entityType)}
											<label class="text-secondary flex items-center gap-2 text-sm">
												<input
													type="checkbox"
													class="checkbox-card h-4 w-4"
													checked={chosenTypes.includes(entityType)}
													onchange={() => toggleType(entityType)}
												/>
												{entities.getMetadata(entityType)?.entity_name_plural ?? entityType}
											</label>
										{/each}
									</div>
									<p class="text-tertiary text-xs">{tags_appliesToHelp()}</p>
								{/if}
							</div>
						{/snippet}
					</form.Field>
				</div>

				{#if isEditing}
					<div class="space-y-2">
						<div class="text-secondary text-sm font-medium">{tags_tagOrder()}</div>
						{#if orderedTags.length === 0}
							<p class="text-tertiary text-xs">{tags_noTagsInGroup()}</p>
						{:else}
							<ol class="space-y-1">
								{#each orderedTags as tag, index (tag.id)}
									<li class="flex items-center gap-2">
										<span class="text-tertiary w-5 text-right text-xs">{index + 1}</span>
										<Tag
											label={tag.name}
											color={tag.color}
											icon={tagIcon(tag, applicationGroup)}
											pill={true}
										/>
										<div class="ml-auto flex gap-1">
											<button
												type="button"
												class="btn-icon"
												disabled={index === 0}
												aria-label={tags_moveTagUp({ name: tag.name })}
												title={tags_moveTagUp({ name: tag.name })}
												onclick={() => move(index, -1)}
											>
												<ChevronUp class="h-4 w-4" />
											</button>
											<button
												type="button"
												class="btn-icon"
												disabled={index === orderedTags.length - 1}
												aria-label={tags_moveTagDown({ name: tag.name })}
												title={tags_moveTagDown({ name: tag.name })}
												onclick={() => move(index, 1)}
											>
												<ChevronDown class="h-4 w-4" />
											</button>
										</div>
									</li>
								{/each}
							</ol>
							<p class="text-tertiary text-xs">{tags_tagOrderHelp()}</p>
						{/if}
					</div>
				{/if}
			</div>
		</div>

		{#if isEditing && group}
			<EntityMetadataSection entities={[group]} />
		{/if}

		<div class="modal-footer">
			<div class="flex items-center justify-between">
				<div>
					{#if isEditing && onDelete && !isSystem}
						<button
							type="button"
							disabled={deleting || loading}
							onclick={handleDelete}
							class="btn-danger"
						>
							{deleting ? common_deleting() : common_delete()}
						</button>
					{/if}
				</div>
				<div class="flex items-center gap-3">
					<button
						type="button"
						disabled={loading || deleting}
						onclick={onClose}
						class="btn-secondary"
					>
						{common_cancel()}
					</button>
					<button type="submit" disabled={loading || deleting} class="btn-primary">
						{loading ? common_saving() : isEditing ? common_update() : common_create()}
					</button>
				</div>
			</div>
		</div>
	</form>
</GenericModal>
