import type { FieldConfig } from '$lib/shared/components/data/types';
import { siteItems } from '$lib/features/sites/columns';
import { permissions } from '$lib/shared/stores/metadata';
import type { Site } from '$lib/features/sites/types';
import type { Tag } from '$lib/features/tags/types/base';
import type { UserApiKey } from '../queries';
import {
	common_enabled,
	common_expired,
	common_expires,
	common_lastUsed,
	common_never,
	common_created,
	common_name,
	common_sites,
	common_permissions,
	common_tags,
	common_updated
} from '$lib/paraglide/messages';

/** What the user API key columns read from the tab, each as a getter so values stay live. */
export interface UserApiKeyColumnsContext {
	sites: () => Site[];
	tags: () => Tag[];
}

export function userApiKeyColumns(ctx: UserApiKeyColumnsContext): FieldConfig<UserApiKey>[] {
	return [
		{
			key: 'name',
			label: common_name(),
			type: 'string',
			searchable: true,
			sortable: true
		},
		{
			key: 'permissions',
			type: 'string',
			label: common_permissions(),
			searchable: true,
			filterable: true,
			groupable: true,
			sortable: true,
			// Every role the backend defines.
			filterOptions: permissions.getItems().map((role) => role.id),
			filterLabel: (role) => permissions.getName(role) || role,
			getFilterValue: (item) => item.permissions ?? null,
			getValue: (item) =>
				item.permissions ? permissions.getName(item.permissions) || item.permissions : null,
			display: {
				getItems: (item) => {
					const role = item.permissions;
					if (!role) return [];
					return [
						{
							id: role,
							label: permissions.getName(role) || role,
							color: permissions.getColorHelper(role).color
						}
					];
				}
			}
		},
		{
			key: 'site_ids',
			type: 'array',
			label: common_sites(),
			searchable: true,
			// Keys share sites, so this filters; as an array it neither sorts nor groups.
			filterable: true,
			getFilterValue: (item) => item.site_ids ?? [],
			filterLabel: (id) => ctx.sites().find((n) => n.id === id)?.name ?? id,
			getValue(item) {
				const ids = item.site_ids ?? [];
				return ids
					.map((id) => ctx.sites().find((n) => n.id === id)?.name)
					.filter((name): name is string => !!name);
			},
			display: { getItems: (item) => siteItems(item.site_ids, ctx.sites()) }
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
				return (entity.tags ?? [])
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
