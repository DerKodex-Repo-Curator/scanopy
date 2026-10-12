<!--
	A named entity is a real entity, so its chip is drawn the way one is drawn everywhere — and its
	icon and colour come from the entity metadata rather than being fixed to Host, which is what
	lets a credential use the same chip instead of a near-copy of it.
-->
<script lang="ts">
	import EntityTag from '$lib/shared/components/data/EntityTag.svelte';
	import Tag from '$lib/shared/components/data/Tag.svelte';
	import { entityRef } from '$lib/shared/components/data/types';
	import { entities } from '$lib/shared/stores/metadata';
	import type { WarningSubject } from '../../utils/warnings';

	let { subject }: { subject: WarningSubject } = $props();
</script>

{#if subject.entity}
	{@const type = subject.entity.type}
	<EntityTag
		entityRef={entityRef(type, subject.entity.id, {
			id: subject.entity.id,
			name: subject.label
		})}
		label={subject.label}
		icon={entities.getIconComponent(type)}
		color={entities.getColorHelper(type).color}
	/>
{:else}
	<!--
		Nothing to point at, so no colour, no hover state and no click: a bare address, or a
		device deleted since the scan. The decision lives here rather than at each call site
		because it used to live at one of the three — the evidence lines rendered a chip
		unconditionally, and a subject with no entity fell through to an EntityTag carrying an
		empty id, which is a link to nowhere dressed as a device.
	-->
	<Tag label={subject.label} color={null} />
{/if}
