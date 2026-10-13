import type { FieldConfig } from '$lib/shared/components/data/types';
import { entities } from '$lib/shared/stores/metadata';
import { isSystemGroup, tagGroupIcon, tagGroupLabel, tagsInGroup } from '../groups';
import { tagItems } from '../columns';
import type { Tag, TagGroup } from '../types/base';
import {
	common_created,
	common_custom,
	common_description,
	common_name,
	common_system,
	common_tags,
	common_type,
	common_updated,
	tags_appliesTo,
	tags_appliesToEvery,
	tags_orderByName,
	tags_orderSet,
	tags_tagOrder
} from '$lib/paraglide/messages';

/** What the tag group columns read from the tab, each as a getter so values stay live. */
export interface TagGroupColumnsContext {
	tags: () => Tag[];
	groups: () => TagGroup[];
}

/** The filter value of a group that applies to every entity type, which stores none. */
const EVERY_ENTITY_TYPE = '*';

function scopeName(type: string): string {
	if (type === EVERY_ENTITY_TYPE) return tags_appliesToEvery();
	const entityType = entities.getItems().find((item) => item.id === type);
	return entityType ? (entities.getMetadata(entityType.id)?.entity_name_plural ?? type) : type;
}

function scopeNames(group: TagGroup): string[] {
	return (group.entity_types ?? [EVERY_ENTITY_TYPE]).map(scopeName);
}

export function tagGroupColumns(ctx: TagGroupColumnsContext): FieldConfig<TagGroup>[] {
	return [
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
			getValue: (group) => group.description,
			display: { wrap: true }
		},
		{
			key: 'group_type',
			label: common_type(),
			type: 'string',
			sortable: true,
			filterable: true,
			groupable: true,
			getValue: (group) => (isSystemGroup(group) ? common_system() : common_custom()),
			getFilterValue: (group) => (isSystemGroup(group) ? 'system' : 'custom'),
			filterLabel: (value) => (value === 'system' ? common_system() : common_custom())
		},
		{
			key: 'ordered',
			label: tags_tagOrder(),
			type: 'string',
			sortable: true,
			filterable: true,
			groupable: true,
			getValue: (group) => (group.ordered ? tags_orderSet() : tags_orderByName()),
			getFilterValue: (group) => (group.ordered ? 'set' : 'name'),
			filterLabel: (value) => (value === 'set' ? tags_orderSet() : tags_orderByName())
		},
		{
			key: 'entity_types',
			label: tags_appliesTo(),
			type: 'array',
			filterable: true,
			getValue: (group) => scopeNames(group),
			getFilterValue: (group) => group.entity_types ?? [EVERY_ENTITY_TYPE],
			filterLabel: scopeName
		},
		{
			key: 'tags',
			label: common_tags(),
			type: 'array',
			searchable: true,
			filterable: true,
			getValue: (group) => tagsInGroup(ctx.tags(), group.id).map((tag) => tag.name),
			getFilterValue: (group) => tagsInGroup(ctx.tags(), group.id).map((tag) => tag.id),
			display: {
				getItems: (group) =>
					tagItems(
						tagsInGroup(ctx.tags(), group.id).map((tag) => tag.id),
						ctx.tags(),
						ctx.groups()
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
	];
}
