<script lang="ts" module>
	import { entities } from '$lib/shared/stores/metadata';
	import { tagGroupIcon, tagGroupLabel } from '$lib/features/tags/groups';

	/** A tag group as a row: its name, drawn with the system group's icon when it is one. */
	export const TagGroupDisplay: EntityDisplayComponent<TagGroup, object> = {
		getId: (group) => group.id,
		getLabel: (group) => tagGroupLabel(group) ?? group.name,
		getIcon: (group) => tagGroupIcon(group) ?? entities.getIconComponent('TagGroup'),
		getIconColor: () => entities.getColorHelper('TagGroup').icon,
		getCategory: () => null
	};
</script>

<script lang="ts">
	import ListSelectItem from '$lib/shared/components/forms/selection/ListSelectItem.svelte';
	import type { EntityDisplayComponent } from '../types';
	import type { TagGroup } from '$lib/features/tags/types/base';

	interface Props {
		item: TagGroup;
		context?: object;
	}

	let { item, context = {} }: Props = $props();
</script>

<ListSelectItem {item} {context} displayComponent={TagGroupDisplay} />
