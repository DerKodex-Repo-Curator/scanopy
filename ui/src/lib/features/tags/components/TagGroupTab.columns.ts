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

function scopeNames(group: TagGroup): string[] {
	return group.entity_types
		? group.entity_types.map((type) => entities.getMetadata(type)?.entity_name_plural ?? type)
		: [tags_appliesToEvery()];
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
			getValue: (group) => tagsInGroup(ctx.tags(), group.id).map((tag) => tag.name),
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
