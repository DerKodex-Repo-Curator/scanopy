<script lang="ts">
	import type { Node } from '@xyflow/svelte';
	import type { RenderableTopology, TopologyNode } from '$lib/features/topology/types/base';
	import { nodeEntity, topologyLookup } from '$lib/features/topology/resolvers';
	import {
		entityDisplayText,
		entityFieldLabel,
		entityFields,
		fieldSource,
		fieldEntityType,
		formatFieldValue,
		resolveField,
		type EntityField
	} from '$lib/shared/entity-fields';
	import { entityRef } from '$lib/shared/components/data/types';
	import { entities } from '$lib/shared/stores/metadata';
	import type { AttributeSource } from '$lib/shared/utils/attribute-source';
	import InfoRow from '$lib/shared/components/data/InfoRow.svelte';
	import EntityTag from '$lib/shared/components/data/EntityTag.svelte';
	import CopyableValue from '$lib/shared/components/data/CopyableValue.svelte';
	import AttributeSourceTag from '$lib/shared/components/data/AttributeSourceTag.svelte';
	import InspectorSection from '../shared/InspectorSection.svelte';

	/**
	 * The selected shape's entity, described by its summary fields from the backend field registry
	 * (`shared/entity-fields`). Which facts appear, and where related ones come from, is the
	 * registry's call; this section only lays them out.
	 */
	let { node, topology }: { node: Node; topology: RenderableTopology } = $props();

	type Row =
		| {
				field: EntityField;
				label: string;
				type: 'entities';
				items: { id: string; label: string; data: object }[];
		  }
		| { field: EntityField; label: string; type: 'identifiers'; values: string[] }
		| { field: EntityField; label: string; type: 'text'; text: string };

	let resolved = $derived(nodeEntity(node.data as TopologyNode, topology));
	let lookup = $derived(topologyLookup(topology));

	let rows = $derived.by((): Row[] => {
		if (!resolved) return [];
		const { entityType, entity } = resolved;
		const out: Row[] = [];
		for (const field of entityFields(entityType).filter((f) => f.summary)) {
			const values = resolveField(field, entity, lookup);
			if (values.length === 0) continue;
			const label = entityFieldLabel(entityType, field);
			const target = fieldEntityType(field);
			if (field.format.type === 'Entity' && target) {
				const items = values.flatMap((value) => {
					const related = value as { id?: string };
					return related?.id
						? [{ id: related.id, label: entityDisplayText(target, related), data: related }]
						: [];
				});
				if (items.length) out.push({ field, label, type: 'entities', items });
			} else if (field.format.type === 'Identifier') {
				out.push({
					field,
					label,
					type: 'identifiers',
					values: values.map((value) => formatFieldValue(field, value)).filter(Boolean)
				});
			} else {
				const text = values
					.map((value) => formatFieldValue(field, value))
					.filter(Boolean)
					.join(', ');
				if (text) out.push({ field, label, type: 'text', text });
			}
		}
		return out;
	});

	function sourceOf(field: EntityField): AttributeSource | null | undefined {
		return resolved ? fieldSource(field, resolved.entity) : undefined;
	}
</script>

{#if resolved && rows.length > 0}
	<InspectorSection id="Details" section="Details">
		<div class="card card-static space-y-1.5">
			{#each rows as row (row.field.id)}
				<InfoRow label={row.label}>
					<span class="inline-flex flex-wrap items-center justify-end gap-1">
						{#if row.type === 'entities'}
							{@const target = fieldEntityType(row.field)!}
							{#each row.items as item (item.id)}
								<EntityTag
									entityRef={entityRef(target, item.id, item.data)}
									label={item.label}
									color={entities.getColorHelper(target).color}
								/>
							{/each}
						{:else if row.type === 'identifiers'}
							{#each row.values as value (value)}
								<CopyableValue {value} variant="inline" />
							{/each}
						{:else}
							<span class="text-primary text-sm">{row.text}</span>
						{/if}
						{#if row.field.source_field}
							<AttributeSourceTag source={sourceOf(row.field)} />
						{/if}
					</span>
				</InfoRow>
			{/each}
		</div>
	</InspectorSection>
{/if}
