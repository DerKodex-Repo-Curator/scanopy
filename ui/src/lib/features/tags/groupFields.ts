import type { components } from '$lib/api/schema';
import type { FieldConfig, FilterCodec } from '$lib/shared/components/data/types';
import type { Snippet } from 'svelte';
import { tagItems } from './columns';
import { compareTagOrder, groupsFor, tagGroupLabel, tagsInGroup } from './groups';
import type { Tag, TagGroup } from './types/base';
import { tags_tagGroups } from '$lib/paraglide/messages';

type EntityDiscriminants = components['schemas']['EntityDiscriminants'];

/** Field keys of tag-group columns start with this, followed by the group's id. */
export const TAG_GROUP_FIELD_PREFIX = 'tag_group:';

export function tagGroupFieldKey(groupId: string): string {
	return `${TAG_GROUP_FIELD_PREFIX}${groupId}`;
}

/** The tag group a field key names, or `null` when the key is not a tag-group column's. */
export function tagGroupIdOf(fieldKey: string | null | undefined): string | null {
	return fieldKey?.startsWith(TAG_GROUP_FIELD_PREFIX)
		? fieldKey.slice(TAG_GROUP_FIELD_PREFIX.length)
		: null;
}

/**
 * The key a server-paged list groups a row under for a tag-group column: the held tag's position
 * padded to ten digits (0 in an unordered group), its name, then its id. It matches the server's
 * `TagGroupOrdering::GROUP_SQL`, so a group finds its total in `group_counts`.
 */
function serverGroupKey(tag: Tag | null): string | null {
	if (!tag) return null;
	return `${String(tag.position ?? 0).padStart(10, '0')}:${tag.name}:${tag.id}`;
}

/**
 * One column per tag group that applies to `entityType`, hidden until the user shows it. Each
 * shows the tag the row holds from its group, sorts and groups in the group's tag order, and
 * filters by the group's tags.
 *
 * On a server-paged list the columns are server-orderable and server-filtered: the parent maps
 * their keys through {@link tagGroupIdOf}.
 */
export function tagGroupFields<T>({
	groups,
	tags,
	entityType,
	getItemTags,
	serverPaginated,
	cell
}: {
	groups: TagGroup[];
	tags: Tag[];
	entityType: EntityDiscriminants;
	getItemTags: (item: T) => string[];
	serverPaginated: boolean;
	/** Renders an editable cell, called with the row and the column id; absent for read-only viewers. */
	cell?: Snippet<[T, string]>;
}): FieldConfig<T>[] {
	return groupsFor(groups, entityType).map((group) => {
		const groupTags = tagsInGroup(tags, group.id);
		const held = (item: T): Tag | null => {
			const ids = getItemTags(item);
			return groupTags.find((tag) => ids.includes(tag.id)) ?? null;
		};
		const key = tagGroupFieldKey(group.id);
		const base = {
			label: tagGroupLabel(group) ?? group.name,
			type: 'string' as const,
			searchable: true,
			filterable: true,
			filterOptions: groupTags.map((tag) => tag.id),
			filterLabel: (id: string) => groupTags.find((tag) => tag.id === id)?.name ?? id,
			filterCodec: tagSetCodec(groupTags.map((tag) => tag.id)),
			getValue: (item: T) => held(item)?.name ?? null,
			getFilterValue: (item: T) => held(item)?.id ?? null,
			// Rows holding no tag of the group have no value, so `compareByField` sorts them last.
			compare: (a: T, b: T) => {
				const [ta, tb] = [held(a), held(b)];
				return ta && tb ? compareTagOrder(ta, tb) : 0;
			},
			display: {
				hiddenByDefault: true,
				section: tags_tagGroups(),
				cell,
				getItems: (item: T) => {
					const tag = held(item);
					return tag ? tagItems([tag.id], tags, groups) : [];
				}
			}
		};
		return serverPaginated
			? {
					...base,
					orderField: key,
					groupable: true,
					serverFiltered: true,
					getGroupValue: (item: T) => serverGroupKey(held(item))
				}
			: { ...base, key, sortable: true, groupable: true };
	});
}

const TAG_SETS_PARAM = 'tag_sets';

/**
 * A tag-group column's filter as the API takes it: one `tag_sets` value per filtered group, its
 * checked tag ids comma-separated. Several groups repeat the param. Reading it back, a group
 * takes the set made of its own tags.
 */
function tagSetCodec(groupTagIds: string[]): FilterCodec {
	const own = new Set(groupTagIds);
	return {
		keys: [TAG_SETS_PARAM],
		encode: (ids) => [[TAG_SETS_PARAM, ids.join(',')]],
		decode: (params) =>
			params
				.getAll(TAG_SETS_PARAM)
				.map((set) => set.split(',').filter(Boolean))
				.find((ids) => ids.length > 0 && ids.every((id) => own.has(id)))
	};
}
