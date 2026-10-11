import { defineRegistryFields } from '$lib/shared/entity-fields/table';
import type { EntityLookup } from '$lib/shared/entity-fields';
import type { components } from '$lib/api/schema';
import { compareTagsByGroup, groupOf, tagGroupLabel, tagIcon } from '../groups';
import { tagItems } from '../columns';
import type { Tag, TagGroup } from '../types/base';
import { common_created, common_updated, tags_tagGroup } from '$lib/paraglide/messages';

type TagOrderField = components['schemas']['TagOrderField'];

/** What the tag columns read from the tab, each as a getter so values stay live. */
export interface TagColumnsContext {
	lookup: () => EntityLookup;
	tagGroups: () => TagGroup[];
	applicationGroup: () => string | null;
}

/**
 * Columns from the entity field registry; this tab adds how each one searches, filters and
 * sorts. Every TagOrderField value must be listed.
 */
export function tagColumns(ctx: TagColumnsContext) {
	return defineRegistryFields<Tag, TagOrderField>(
		'Tag',
		ctx.lookup,
		{
			// Identity field: grouping by it would render a header per tag. Drawn as the tag itself,
			// with its icon and colour, as it appears on every entity carrying it.
			name: {
				searchable: true,
				groupable: false,
				display: {
					primary: true,
					width: 220,
					getItems: (tag) => tagItems([tag.id], [tag], ctx.tagGroups())
				}
			},
			color: {
				searchable: true,
				filterable: true,
				// A colour name is worth showing in its own colour. Hidden by default: the name column
				// already draws it, so this column is there to group and filter by.
				display: {
					hiddenByDefault: true,
					getItems: (tag) => [
						{
							id: tag.color,
							label: tag.color.charAt(0).toUpperCase() + tag.color.slice(1),
							color: tag.color
						}
					]
				}
			},
			tag_group: {
				label: tags_tagGroup(),
				filterable: true,
				// A system group reads under its translated name.
				getValue: (tag) => tagGroupLabel(groupOf(tag, ctx.tagGroups())),
				// Groups in the server's order, each group's tags in its own order.
				compare: compareTagsByGroup(ctx.tagGroups())
			},
			created_at: { label: common_created(), type: 'date', display: { hiddenByDefault: true } },
			updated_at: { label: common_updated(), type: 'date', display: { hiddenByDefault: true } }
		},
		[
			{ key: 'description', searchable: true },
			{
				// Hidden by default for the same reason as colour: the name column draws the icon.
				key: 'icon',
				filterable: true,
				groupable: true,
				display: {
					hiddenByDefault: true,
					getItems: (tag) => {
						const icon = tagIcon(tag, ctx.applicationGroup());
						return icon && tag.icon ? [{ id: tag.icon, label: tag.icon, icon }] : [];
					}
				}
			}
		]
	);
}
