<script lang="ts">
	import { Plus } from 'lucide-svelte';
	import Tag from '$lib/shared/components/data/Tag.svelte';
	import TagCompletionGhost from './TagCompletionGhost.svelte';
	import InputGhost from '$lib/shared/components/forms/input/InputGhost.svelte';
	import { tagCompletion } from '$lib/features/tags/completion';
	import {
		useTagsQuery,
		useTagGroupsQuery,
		useCreateTagMutation,
		useBulkAddTagMutation,
		useBulkRemoveTagMutation,
		type EntityDiscriminants
	} from '$lib/features/tags/queries';
	import { createDefaultTag } from '$lib/features/tags/types/base';
	import { AVAILABLE_COLORS, type Color } from '$lib/shared/utils/styling';
	import { useCurrentUserQuery } from '$lib/features/auth/queries';
	import { permissions, billingPlans } from '$lib/shared/stores/metadata';
	import { useOrganizationQuery } from '$lib/features/organizations/queries';
	import { onMount, tick, untrack } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { common_creating, tags_addTag, tags_createTagQuoted } from '$lib/paraglide/messages';
	import CreatableOptionList from '$lib/shared/components/forms/selection/CreatableOptionList.svelte';
	import {
		applicationGroupId,
		compareTagsByGroup,
		groupOf,
		isApplicationTag,
		sameGroup,
		tagAllowedOn,
		tagGroupLabel,
		tagIcon,
		tagTooltip
	} from '$lib/features/tags/groups';

	/**
	 * Compact inline tag picker for use in cards and bulk actions.
	 * Shows tags as pills with remove buttons, and a + button to add tags.
	 *
	 * Two modes of operation:
	 * 1. **Entity mode**: Provide `entityId` and `entityType` - mutations handled internally
	 * 2. **Callback mode**: Provide `onAdd`/`onRemove` callbacks - for bulk operations or custom handling
	 */
	let {
		selectedTagIds = [],
		disabled = false,
		// Entity mode props - when provided, uses generic tag assignment API
		entityId,
		entityType,
		// Callback mode props - for bulk operations or custom handling
		onAdd,
		onRemove,
		// Optional pre-resolved tags (e.g. from topology snapshot on share pages)
		availableTags: availableTagsProp,
		// When true, tags created inline go in the Application group
		createAsApplication = false,
		// When false, hides the inline tag creation option
		allowCreate = true,
		// Bindable: mirrors the dropdown's open state. Set to true to open programmatically;
		// updates back to false when the dropdown closes (e.g. user blurs away).
		open = $bindable(false),
		// When true, hide the "+" affordance. The dropdown input still renders while `open`.
		hideAddButton = false
	}: {
		selectedTagIds?: string[];
		disabled?: boolean;
		entityId?: string;
		entityType?: EntityDiscriminants;
		onAdd?: (tagId: string) => void;
		onRemove?: (tagId: string) => void;
		availableTags?: import('$lib/features/tags/types/base').Tag[];
		createAsApplication?: boolean;
		allowCreate?: boolean;
		open?: boolean;
		hideAddButton?: boolean;
	} = $props();

	// Entity mode: use generic mutations
	const bulkAddTagMutation = useBulkAddTagMutation();
	const bulkRemoveTagMutation = useBulkRemoveTagMutation();

	// Determine if entity mode is enabled
	let isEntityMode = $derived(entityId !== undefined && entityType !== undefined);

	let inputValue = $state('');
	let inputElement: HTMLInputElement | undefined = $state();
	let triggerElement: HTMLDivElement | undefined = $state();
	let dropdownElement: HTMLDivElement | undefined = $state();
	/**
	 * Below the trigger, `top` is set; flipped above it, `bottom` is. Either way the list grows to
	 * fit every tag, up to the screen edge less `EDGE_GAP_PX`, and scrolls past that.
	 */
	let dropdownPosition = $state<{
		top: number | null;
		bottom: number | null;
		left: number;
		maxHeight: number;
	}>({ top: 0, bottom: null, left: 0, maxHeight: 192 });

	/** Same gap the column config panel keeps from the bottom of the screen. */
	const EDGE_GAP_PX = 16;
	/** Below this much room under the trigger, the list opens above it when there's more room there. */
	const MIN_DROPDOWN_PX = 160;
	const TRIGGER_GAP_PX = 4;

	// Portal container for escaping stacking contexts
	let portalContainer: HTMLDivElement | null = $state(null);

	onMount(() => {
		portalContainer = document.createElement('div');
		portalContainer.style.position = 'absolute';
		portalContainer.style.top = '0';
		portalContainer.style.left = '0';
		portalContainer.style.width = '0';
		portalContainer.style.height = '0';
		document.body.appendChild(portalContainer);

		return () => {
			portalContainer?.remove();
		};
	});

	// Portal action to move element to body
	function portal(node: HTMLElement) {
		if (portalContainer) {
			portalContainer.appendChild(node);
		}
		return {
			destroy() {}
		};
	}

	function calculatePosition() {
		if (!triggerElement) return;
		const rect = triggerElement.getBoundingClientRect();
		const viewportHeight = window.innerHeight;
		const viewportWidth = window.innerWidth;

		// Vertical: open below unless the list doesn't fit there and there's more room above.
		const contentHeight = dropdownElement?.scrollHeight ?? MIN_DROPDOWN_PX;
		const spaceBelow = viewportHeight - rect.bottom - TRIGGER_GAP_PX - EDGE_GAP_PX;
		const spaceAbove = rect.top - TRIGGER_GAP_PX - EDGE_GAP_PX;
		const below =
			spaceBelow >= Math.min(contentHeight, MIN_DROPDOWN_PX) || spaceBelow >= spaceAbove;

		// Horizontal: clamp to viewport edges
		let left = rect.left;
		left = Math.max(8, Math.min(left, viewportWidth - 160 - 8)); // min-w-40 = 160px

		dropdownPosition = below
			? { top: rect.bottom + TRIGGER_GAP_PX, bottom: null, left, maxHeight: spaceBelow }
			: {
					top: null,
					bottom: viewportHeight - rect.top + TRIGGER_GAP_PX,
					left,
					maxHeight: spaceAbove
				};
	}

	// Recalculate position once dropdown renders (so we measure real height for flip)
	$effect(() => {
		if (open && dropdownElement) {
			calculatePosition();
		}
	});

	// Reposition dropdown on scroll when open
	$effect(() => {
		if (!open) return;

		const handleScroll = () => calculatePosition();
		window.addEventListener('scroll', handleScroll, true);
		window.addEventListener('resize', handleScroll);
		return () => {
			window.removeEventListener('scroll', handleScroll, true);
			window.removeEventListener('resize', handleScroll);
		};
	});

	// Query and mutation
	const tagsQuery = useTagsQuery();
	const tagGroupsQuery = useTagGroupsQuery();
	let tagGroups = $derived(tagGroupsQuery.data ?? []);
	let applicationGroup = $derived(applicationGroupId(tagGroups));
	const createTagMutation = useCreateTagMutation();
	const organizationQuery = useOrganizationQuery();
	const currentUserQuery = useCurrentUserQuery();

	// Derived state
	let tags = $derived(availableTagsProp ?? tagsQuery.data ?? []);
	let isCreating = $derived(createTagMutation.isPending);
	let organization = $derived(organizationQuery.data);
	let currentUser = $derived(currentUserQuery.data);

	// Demo orgs are read-only for non-owners (mirrors the credentials tab)
	let isDemoOrg = $derived(
		billingPlans.getMetadata(organization?.plan?.type ?? null).is_demo === true
	);
	let isNonOwnerInDemo = $derived(isDemoOrg && currentUser?.permissions !== 'Owner');

	// Check if user can create tags
	let canCreateTags = $derived(
		allowCreate &&
			!isNonOwnerInDemo &&
			currentUser &&
			permissions.getMetadata(currentUser.permissions).manage_org_entities
	);

	// Check if typed value matches an existing tag name exactly. Names are unique across the org, so
	// this checks every tag, including any the caller left out of `availableTags`.
	let exactMatch = $derived(
		(tagsQuery.data ?? tags).some((t) => t.name.toLowerCase() === inputValue.trim().toLowerCase())
	);

	// Show create option if user typed something, can create, and no exact match exists
	let showCreateOption = $derived(inputValue.trim().length > 0 && canCreateTags && !exactMatch);

	// Get tag by ID
	function getTag(id: string) {
		return tags.find((t) => t.id === id) ?? null;
	}

	// Optimistic: an added tag shows as a chip, and a removed one disappears, as soon as it's
	// asked for. The caller's list catches up when its refetch lands; until then these sets
	// bridge the gap, so an accepted ghost tag turns into its chip in place instead of vanishing
	// and reappearing. A failed change drops out of the sets and the chip reverts.
	const pendingAdds = new SvelteSet<string>();
	const pendingRemoves = new SvelteSet<string>();

	let shownTagIds = $derived([
		...selectedTagIds.filter((id) => !pendingRemoves.has(id)),
		...[...pendingAdds].filter((id) => !selectedTagIds.includes(id))
	]);

	// Filter available tags based on input, exclude already selected, and leave out tags whose group
	// doesn't allow the entity type, in group order
	let availableTags = $derived(
		tags
			.filter(
				(tag) =>
					!shownTagIds.includes(tag.id) &&
					(!entityType || tagAllowedOn(tag, tagGroups, entityType)) &&
					tag.name.toLowerCase().includes(inputValue.toLowerCase())
			)
			.sort(compareTagsByGroup(tagGroups))
	);

	let showDropdown = $derived(open && (availableTags.length > 0 || showCreateOption));

	// The existing tag the typed text completes to, ghosted in the input; Tab applies it.
	let completion = $derived(tagCompletion(inputValue, availableTags, shownTagIds));

	function getRandomColor(): Color {
		return AVAILABLE_COLORS[Math.floor(Math.random() * AVAILABLE_COLORS.length)];
	}

	async function handleCreateTag() {
		if (!organization || isCreating) return;
		if (!isEntityMode && !onAdd) return;

		const name = inputValue.trim();
		if (!name) return;

		try {
			const newTag = createDefaultTag(organization.id);
			newTag.name = name;
			newTag.color = getRandomColor();
			if (createAsApplication) {
				newTag.tag_group_id = applicationGroup;
			}

			const result = await createTagMutation.mutateAsync(newTag);
			await handleAddTag(result.id);
			inputValue = '';
			open = false;
		} finally {
			inputElement?.focus();
		}
	}

	// Settle each pending change once the caller's list shows it.
	$effect(() => {
		const held = selectedTagIds;
		untrack(() => {
			for (const id of pendingAdds) if (held.includes(id)) pendingAdds.delete(id);
			for (const id of pendingRemoves) if (!held.includes(id)) pendingRemoves.delete(id);
		});
	});

	/** Run a change, reverting its optimistic state when it fails (throws, or reports `false`). */
	async function settle(pending: SvelteSet<string>, tagId: string, run: () => unknown) {
		pending.add(tagId);
		try {
			if ((await run()) === false) pending.delete(tagId);
		} catch (error) {
			pending.delete(tagId);
			throw error;
		}
	}

	/** `keepOpen` leaves the input up for the next tag, as Tab on a completion does. */
	async function handleAddTag(tagId: string, keepOpen = false) {
		inputValue = '';
		if (!keepOpen) open = false;
		if (isEntityMode && entityId && entityType) {
			await settle(pendingAdds, tagId, () =>
				bulkAddTagMutation.mutateAsync({
					entity_ids: [entityId],
					entity_type: entityType,
					tag_id: tagId
				})
			);
		} else {
			// The server replaces a held tag of the same tag group on assignment; in callback
			// mode the caller holds the list, so drop the displaced tag here to match.
			const added = getTag(tagId);
			for (const heldId of shownTagIds) {
				if (heldId !== tagId && sameGroup(getTag(heldId), added)) {
					void settle(pendingRemoves, heldId, () => onRemove?.(heldId));
				}
			}
			await settle(pendingAdds, tagId, () => onAdd?.(tagId));
		}
	}

	async function handleRemoveTag(tagId: string) {
		if (isEntityMode && entityId && entityType) {
			await settle(pendingRemoves, tagId, () =>
				bulkRemoveTagMutation.mutateAsync({
					entity_ids: [entityId],
					entity_type: entityType,
					tag_id: tagId
				})
			);
		} else {
			await settle(pendingRemoves, tagId, () => onRemove?.(tagId));
		}
	}

	/** Open the input and focus it, for a keyboard shortcut. */
	export function focusInput() {
		if (!disabled) open = true;
	}

	let rootElement: HTMLDivElement | undefined = $state();
	/** Which remove button had focus when its tag was removed, so focus can land on a neighbour. */
	let refocusIndex: number | null = null;

	function removeButtons(): HTMLElement[] {
		return Array.from(rootElement?.querySelectorAll<HTMLElement>('[data-tag-remove]') ?? []);
	}

	function removeTag(tagId: string) {
		const index = removeButtons().indexOf(document.activeElement as HTMLElement);
		refocusIndex = index === -1 ? null : index;
		handleRemoveTag(tagId);
	}

	// A tag removed from the keyboard unmounts the focused button, dropping focus to the page.
	// Once the list updates (in callback mode the caller's refetch lands later), put focus on the
	// tag that took its place, or on the input when none is left.
	$effect(() => {
		void shownTagIds;
		if (refocusIndex === null) return;
		const index = refocusIndex;
		tick().then(() => {
			if (rootElement?.contains(document.activeElement)) return;
			refocusIndex = null;
			const remaining = removeButtons();
			const next = remaining[Math.min(index, remaining.length - 1)];
			if (next) next.focus();
			else focusInput();
		});
	});

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Tab' && !e.shiftKey && completion) {
			e.preventDefault();
			handleAddTag(completion.id, true);
		} else if (e.key === 'Enter') {
			// Enter takes the text as typed: it creates that tag, and only Tab takes the ghosted
			// completion. With nothing to create (the name exists, or creating isn't allowed), it
			// applies the closest match.
			e.preventDefault();
			if (showCreateOption) {
				handleCreateTag();
			} else if (availableTags.length > 0) {
				handleAddTag((completion ?? availableTags[0]).id);
			}
		} else if (e.key === 'Escape') {
			inputValue = '';
			open = false;
			inputElement?.blur();
		}
	}

	function handleBlur() {
		// Delay to allow click on dropdown item
		setTimeout(() => {
			open = false;
		}, 150);
	}

	function handleAddClick() {
		if (disabled) return;
		open = true;
	}

	// When the dropdown opens (via button click or parent programmatically setting `open`),
	// compute position and focus the input.
	$effect(() => {
		if (open && !disabled) {
			calculatePosition();
			setTimeout(() => inputElement?.focus(), 0);
		}
	});
</script>

<div bind:this={rootElement} class="flex min-w-0 flex-wrap items-center gap-1">
	<!-- Selected tags -->
	{#each shownTagIds.filter((id) => getTag(id)) as tagId (tagId)}
		{@const tag = getTag(tagId)}
		<Tag
			label={tag?.name}
			color={tag?.color}
			icon={tagIcon(tag, applicationGroup)}
			title={tagTooltip(tag, tagGroups)}
			isShiny={isApplicationTag(tag, applicationGroup)}
			pill={!disabled}
			removable={!disabled && !!(onRemove || isEntityMode)}
			onRemove={() => removeTag(tagId)}
		/>
	{/each}

	<!-- Add button / dropdown (hide when no tags to add and creation disabled).
	     `hideAddButton` hides only the "+" affordance — the input still renders while `open`. -->
	{#if (onAdd || isEntityMode) && !disabled && (canCreateTags || tags.some((t) => !shownTagIds.includes(t.id)))}
		<div bind:this={triggerElement} class="relative flex min-h-5 items-center">
			{#if open}
				<!-- Input for searching/creating tags. It widens while it shows a completion, so the
				     ghosted tag fits inside it after the typed text, as in global search. -->
				<div class="relative">
					<input
						bind:this={inputElement}
						bind:value={inputValue}
						type="text"
						placeholder={tags_addTag()}
						class="h-6 rounded-full px-2 text-xs transition-[width] focus:outline-none focus:ring-1 focus:ring-blue-500 {completion
							? 'w-56'
							: 'w-24'}"
						style="border: 1px solid var(--color-border-input); background: var(--color-bg-input); color: var(--color-text-primary)"
						onfocus={() => (open = true)}
						onblur={handleBlur}
						onkeydown={handleKeydown}
					/>
					{#if completion}
						{@const tag = completion}
						<!-- px-[9px]: the input's px-2 plus its 1px border. -->
						<InputGhost value={inputValue} class="px-[9px] text-xs">
							<TagCompletionGhost
								{tag}
								{applicationGroup}
								onAccept={() => handleAddTag(tag.id, true)}
							/>
						</InputGhost>
					{/if}
				</div>
			{:else if !hideAddButton}
				<!-- Add button -->
				<button
					type="button"
					onclick={handleAddClick}
					class="text-tertiary hover:text-secondary inline-flex h-5 w-5 items-center justify-center rounded-full border border-dashed border-gray-400 transition-colors dark:border-gray-500"
				>
					<Plus class="h-3 w-3" />
				</button>
			{/if}
		</div>
	{/if}
</div>

<!-- Portal dropdown to body to escape stacking contexts -->
{#if showDropdown && portalContainer}
	<div
		bind:this={dropdownElement}
		use:portal
		class="select-dropdown fixed z-[9999] min-w-40 overflow-y-auto rounded-md shadow-lg"
		style:top={dropdownPosition.top === null ? undefined : `${dropdownPosition.top}px`}
		style:bottom={dropdownPosition.bottom === null ? undefined : `${dropdownPosition.bottom}px`}
		style:left="{dropdownPosition.left}px"
		style:max-height="{dropdownPosition.maxHeight}px"
	>
		<CreatableOptionList
			options={availableTags.map((tag) => ({
				id: tag.id,
				label: tag.name,
				group: tagGroupLabel(groupOf(tag, tagGroups))
			}))}
			onSelect={handleAddTag}
			createLabel={showCreateOption
				? isCreating
					? common_creating()
					: tags_createTagQuoted({ name: inputValue.trim() })
				: null}
			onCreate={handleCreateTag}
			createOnEnter
			creating={isCreating}
		>
			{#snippet row(option)}
				{@const tag = getTag(option.id)}
				<Tag
					label={tag?.name}
					color={tag?.color}
					icon={tagIcon(tag, applicationGroup)}
					title={tagTooltip(tag, tagGroups)}
					isShiny={isApplicationTag(tag, applicationGroup)}
				/>
			{/snippet}
		</CreatableOptionList>
	</div>
{/if}
