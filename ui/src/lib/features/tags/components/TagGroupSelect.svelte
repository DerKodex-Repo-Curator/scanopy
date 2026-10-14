<script lang="ts">
	import { ChevronDown } from 'lucide-svelte';
	import CreatableOptionList, {
		type CreatableOption
	} from '$lib/shared/components/forms/selection/CreatableOptionList.svelte';
	import { groupNamed, tagGroupIcon, tagGroupLabel, type TagGroup } from '../groups';
	import { common_none, tags_createTagQuoted } from '$lib/paraglide/messages';

	/**
	 * Choose the tag group a tag belongs to: one of the organization's groups, a new group typed
	 * here, or none. A tag sits in at most one group, so this is a single value rather than a list.
	 */
	let {
		value,
		groups,
		onChange,
		onCreate = null,
		id,
		placeholder,
		disabled = false
	}: {
		/** The selected group's id. */
		value: string | null;
		/** The organization's groups, system groups first. */
		groups: TagGroup[];
		onChange: (groupId: string | null) => void;
		/** Creates a group named as typed and selects it. Omit to offer no creation. */
		onCreate?: ((name: string) => Promise<void>) | null;
		id: string;
		placeholder: string;
		disabled?: boolean;
	} = $props();

	const NONE = 'none';

	let open = $state(false);
	let query = $state('');
	let creating = $state(false);

	let needle = $derived(query.trim().toLowerCase());
	let selected = $derived(groups.find((group) => group.id === value) ?? null);

	let options = $derived.by<CreatableOption[]>(() => {
		const result: CreatableOption[] = [];
		if (value && !needle) result.push({ id: NONE, label: common_none() });
		for (const group of groups) {
			const label = tagGroupLabel(group) ?? group.name;
			if (!label.toLowerCase().includes(needle)) continue;
			result.push({ id: group.id, label, icon: tagGroupIcon(group) ?? undefined });
		}
		return result;
	});

	// Offer to create what was typed unless it already names a group.
	let canCreate = $derived(
		onCreate !== null && needle.length > 0 && groupNamed(groups, needle) === null
	);

	function select(optionId: string) {
		onChange(optionId === NONE ? null : optionId);
		close();
	}

	async function create() {
		const name = query.trim();
		if (!name || !onCreate || creating) return;
		creating = true;
		try {
			await onCreate(name);
		} finally {
			creating = false;
			close();
		}
	}

	function close() {
		open = false;
		query = '';
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			if (options.length > 0) select(options[0].id);
			else if (canCreate) create();
		} else if (e.key === 'Escape') {
			close();
		}
	}
</script>

<div class="relative">
	<input
		{id}
		type="text"
		class="input-field w-full pr-8"
		disabled={disabled || creating}
		placeholder={open ? '' : placeholder}
		value={open ? query : (tagGroupLabel(selected) ?? '')}
		onfocus={() => (open = true)}
		onblur={() => setTimeout(close, 150)}
		oninput={(e) => (query = e.currentTarget.value)}
		onkeydown={handleKeydown}
		autocomplete="off"
	/>
	<ChevronDown
		class="text-tertiary pointer-events-none absolute right-2 top-1/2 h-4 w-4 -translate-y-1/2"
	/>

	{#if open && (options.length > 0 || canCreate)}
		<div
			class="select-dropdown absolute left-0 right-0 z-50 mt-1 max-h-48 overflow-y-auto rounded-md shadow-lg"
		>
			<CreatableOptionList
				{options}
				onSelect={select}
				createLabel={canCreate ? tags_createTagQuoted({ name: query.trim() }) : null}
				onCreate={create}
				createOnEnter={options.length === 0}
			/>
		</div>
	{/if}
</div>
