import type { FieldConfig } from '$lib/shared/components/data/types';
import { siteItems } from '$lib/features/sites/columns';
import type { Site } from '$lib/features/sites/types';
import type { Tag } from '$lib/features/tags/types/base';
import type { ApiKey } from '../types/base';
import {
	common_enabled,
	common_expired,
	common_expires,
	common_lastUsed,
	common_never,
	common_created,
	common_name,
	common_site,
	common_tags,
	common_updated,
	common_unknownSite
} from '$lib/paraglide/messages';

/** What the daemon API key columns read from the tab, each as a getter so values stay live. */
export interface ApiKeyColumnsContext {
	sites: () => Site[];
	tags: () => Tag[];
}

export function apiKeyColumns(ctx: ApiKeyColumnsContext): FieldConfig<ApiKey>[] {
	return [
		{
			key: 'name',
			label: common_name(),
			type: 'string',
			searchable: true,
			sortable: true
		},
		{
			key: 'site_id',
			type: 'string',
			label: common_site(),
			searchable: true,
			filterable: true,
			getFilterValue: (item) => item.site_id,
			filterLabel: (id) => ctx.sites().find((n) => n.id === id)?.name ?? common_unknownSite(),
			groupable: true,
			sortable: true,
			getValue(item) {
				return ctx.sites().find((n) => n.id == item.site_id)?.name || common_unknownSite();
			},
			display: { getItems: (item) => siteItems(item.site_id, ctx.sites()) }
		},
		{
			key: 'is_enabled',
			label: common_enabled(),
			type: 'boolean',
			filterable: true,
			groupable: true,
			getValue: (key) => key.is_enabled ?? false
		},
		{
			key: 'last_used',
			label: common_lastUsed(),
			type: 'date',
			sortable: true,
			getValue: (key) => key.last_used ?? null,
			display: { recency: true }
		},
		{
			key: 'expires_at',
			label: common_expires(),
			type: 'date',
			sortable: true,
			getValue: (key) => key.expires_at ?? null,
			display: {
				// Expired reads as a state, not a date — the date has stopped being the useful
				// part once it has passed. Returning undefined shows the date itself.
				getItems: (key) =>
					!key.expires_at
						? [{ id: 'never', label: common_never(), color: 'Gray' }]
						: new Date(key.expires_at) < new Date()
							? [{ id: 'expired', label: common_expired(), color: 'Red' }]
							: undefined
			}
		},
		{
			key: 'tags',
			label: common_tags(),
			type: 'array',
			searchable: true,
			filterable: true,
			getValue: (entity) => {
				// Return tag names for search/filter display
				return entity.tags
					.map((id) => ctx.tags().find((t) => t.id === id)?.name)
					.filter((name): name is string => !!name);
			}
		},
		{
			key: 'created_at',
			label: common_created(),
			type: 'date',
			sortable: true
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
