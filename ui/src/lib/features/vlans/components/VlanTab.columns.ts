import { lastSeenItems } from '$lib/shared/utils/freshness';
import { defineRegistryFields } from '$lib/shared/entity-fields/table';
import type { EntityLookup } from '$lib/shared/entity-fields';
import { siteItems } from '$lib/features/sites/columns';
import { entitySources } from '$lib/shared/stores/metadata';
import { entitySourceItems } from '$lib/shared/utils/entity-source';
import { tagNames } from '$lib/features/tags/columns';
import { discoveryRunItems } from '$lib/features/discovery/columns';
import type { Site } from '$lib/features/sites/types';
import type { Tag } from '$lib/features/tags/types/base';
import type { Discovery } from '$lib/features/discovery/types/base';
import type { Vlan, VlanOrderField } from '../types/base';
import {
	common_created,
	common_firstFoundBy,
	common_lastFoundBy,
	common_lastSeen,
	common_site,
	common_source,
	common_tags,
	common_unknownSite,
	common_updated,
	vlans_vlanNumber
} from '$lib/paraglide/messages';

/** What the VLAN columns read from the tab, each as a getter so values stay live. */
export interface VlanColumnsContext {
	lookup: () => EntityLookup;
	sites: () => Site[];
	tags: () => Tag[];
	discoveryRuns: () => Discovery[];
}

/**
 * Columns from the entity field registry; this adds how each one searches, filters and sorts.
 * Every VlanOrderField value must be listed.
 */
export function vlanColumns(ctx: VlanColumnsContext) {
	return defineRegistryFields<Vlan, VlanOrderField>(
		'Vlan',
		ctx.lookup,
		{
			// Identity fields: grouping by one would render a header per VLAN.
			vlan_number: {
				label: vlans_vlanNumber(),
				searchable: true,
				groupable: false
			},
			name: {
				searchable: true,
				groupable: false,
				display: { primary: true, width: 220 }
			},
			created_at: { label: common_created(), type: 'date', display: { hiddenByDefault: true } },
			updated_at: { label: common_updated(), type: 'date', display: { hiddenByDefault: true } }
		},
		[
			{ key: 'description', searchable: true },
			{
				key: 'source',
				label: common_source(),
				type: 'string',
				filterable: true,
				groupable: true,
				sortable: true,
				// Every source the backend can stamp, named the way the column renders them.
				filterOptions: entitySources.getItems().map((source) => entitySources.getName(source.id)),
				getValue: (vlan) => (vlan.source ? entitySources.getName(vlan.source.type) : null),
				display: {
					hiddenByDefault: true,
					getItems: (vlan) => (vlan.source ? entitySourceItems(vlan.source) : [])
				}
			},
			{
				key: 'site_id',
				label: common_site(),
				type: 'string',
				searchable: true,
				filterable: true,
				groupable: true,
				sortable: true,
				getValue: (item) =>
					ctx.sites().find((n) => n.id == item.site_id)?.name || common_unknownSite(),
				display: { getItems: (item) => siteItems(item.site_id, ctx.sites()) }
			},
			{ key: 'subnet_ids', searchable: true, filterable: true },
			{ key: 'interface_ids', searchable: true, filterable: true },
			{
				// Not in VlanOrderField, so display-only with client-side sorting.
				key: 'last_seen_at',
				label: common_lastSeen(),
				type: 'date',
				sortable: true,
				display: { recency: true, getItems: lastSeenItems(ctx.sites, 'Vlan') }
			},
			{
				key: 'first_found_by',
				label: common_firstFoundBy(),
				type: 'string',
				display: {
					hiddenByDefault: true,
					getItems: (item) => discoveryRunItems(item.first_discovery_id, ctx.discoveryRuns())
				}
			},
			{
				key: 'last_found_by',
				label: common_lastFoundBy(),
				type: 'string',
				display: {
					hiddenByDefault: true,
					getItems: (item) => discoveryRunItems(item.last_discovery_id, ctx.discoveryRuns())
				}
			},
			{
				key: 'tags',
				label: common_tags(),
				type: 'array',
				searchable: true,
				filterable: true,
				getValue: (entity) => tagNames(entity.tags, ctx.tags())
			}
		]
	);
}
