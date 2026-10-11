import type { components } from '$lib/api/schema';
import type { FieldConfig } from '$lib/shared/components/data/types';
import { tagItems } from './columns';
import { compareTagOrder, groupsFor, tagGroupLabel, tagsInGroup } from './groups';
import type { Tag, TagGroup } from './types/base';

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
	serverPaginated
}: {
	groups: TagGroup[];
	tags: Tag[];
	entityType: EntityDiscriminants;
	getItemTags: (item: T) => string[];
	serverPaginated: boolean;
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
			filterOptions: groupTags.map((tag) => tag.name),
			getValue: (item: T) => held(item)?.name ?? null,
			// Rows holding no tag of the group sort last.
			compare: (a: T, b: T) => {
				const [ta, tb] = [held(a), held(b)];
				if (!ta || !tb) return ta ? -1 : tb ? 1 : 0;
				return compareTagOrder(ta, tb);
			},
			display: {
				hiddenByDefault: true,
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

/**
 * The `tag_sets` query values for the tag-group filters a server-paged list holds: one entry per
 * filtered group, the ids of the tags whose names are checked, comma-separated. Names are unique
 * within an organization.
 */
export function tagSetsParam(filters: Record<string, string[]>, tags: Tag[]): string[] | undefined {
	const sets = Object.entries(filters)
		.map(([groupId, names]) =>
			tags
				.filter((tag) => tag.tag_group_id === groupId && names.includes(tag.name))
				.map((tag) => tag.id)
		)
		.filter((ids) => ids.length > 0)
		.map((ids) => ids.join(','));
	return sets.length > 0 ? sets : undefined;
}
