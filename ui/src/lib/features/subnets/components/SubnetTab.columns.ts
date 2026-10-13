import type { Snippet } from 'svelte';
import { lastSeenItems } from '$lib/shared/utils/freshness';
import { cidrSourceItems } from '$lib/shared/utils/cidr-source';
import { compareCidr } from '$lib/shared/utils/cidr';
import { nestedRangeOf, utilizationRatio, type SubnetNesting } from '../nesting';
import type { Subnet, SubnetResponse } from '../types/base';
import { entityRef } from '$lib/shared/components/data/types';
import { defineRegistryFields } from '$lib/shared/entity-fields/table';
import type { EntityLookup } from '$lib/shared/entity-fields';
import { tagNames } from '$lib/features/tags/columns';
import { siteItems } from '$lib/features/sites/columns';
import { discoveryRunItems } from '$lib/features/discovery/columns';
import type { Service } from '$lib/features/services/types/base';
import type { Site } from '$lib/features/sites/types';
import type { Tag } from '$lib/features/tags/types/base';
import type { Discovery } from '$lib/features/discovery/types/base';
import type { components } from '$lib/api/schema';
import {
	common_nested,
	common_created,
	common_lastSeen,
	common_site,
	common_source,
	common_firstFoundBy,
	common_lastFoundBy,
	common_tags,
	common_unknownSite,
	common_updated,
	subnets_managedBy,
	subnets_subnetType,
	common_utilization
} from '$lib/paraglide/messages';
import { concepts, entitySources, subnetTypes } from '$lib/shared/stores/metadata';
import { entitySourceItems } from '$lib/shared/utils/entity-source';

type SubnetOrderField = components['schemas']['SubnetOrderField'];

/** What the subnet columns read from the tab, each as a getter so values stay live. */
export interface SubnetColumnsContext {
	lookup: () => EntityLookup;
	sites: () => Site[];
	tags: () => Tag[];
	discoveryRuns: () => Discovery[];
	/** How the loaded subnets nest inside one another. */
	nesting: () => SubnetNesting;
	/** The container runtime that manages a subnet, if it has one. */
	runtimeOf: (subnet: Subnet) => Service | undefined;
	/** The container runtimes that manage the loaded subnets. */
	runtimes: () => Service[];
	/** The utilization column's cell. */
	utilizationCell: Snippet<[SubnetResponse, string]>;
}

/**
 * Columns from the entity field registry; this adds how each one searches, filters and sorts.
 * Every SubnetOrderField value must be listed.
 */
export function subnetColumns(ctx: SubnetColumnsContext) {
	return defineRegistryFields<SubnetResponse, SubnetOrderField>(
		'Subnet',
		ctx.lookup,
		{
			// Identity fields: grouping by one would render a header per subnet.
			name: {
				searchable: true,
				groupable: false,
				display: { order: 0, primary: true, width: 220 }
			},
			cidr: {
				searchable: true,
				// Groups each range with every range nested inside it, as nested sections. A subnet
				// that nests with nothing falls in the ungrouped bucket rather than heading a
				// group of one.
				getGroupLabel: (subnet) => nestedRangeOf(subnet, ctx.nesting()),
				// Address order, not text order, which is also the tree order: each range sorts
				// directly before the ranges inside it.
				compare: (a, b) => compareCidr(a.cidr, b.cidr),
				tree: {
					key: (subnet) => subnet.id,
					parentKey: (subnet) => subnet.parent_subnet_id ?? null,
					rootsLabel: () => common_nested()
				},
				display: { order: 3, getItems: cidrSourceItems() }
			},
			subnet_type: {
				label: subnets_subnetType(),
				searchable: true,
				filterable: true,
				// Every type the backend defines.
				filterOptions: subnetTypes.getItems().map((type) => type.id),
				filterLabel: (id) => subnetTypes.getName(id),
				getFilterValue: (subnet) => subnet.subnet_type,
				display: {
					order: 4,
					getItems: (subnet) => [
						{
							id: subnet.subnet_type,
							...subnetTypes.getTag(subnet.subnet_type),
							icon: subnetTypes.getIconComponent(subnet.subnet_type)
						}
					]
				}
			},
			site_id: {
				label: common_site(),
				type: 'string',
				searchable: true,
				filterable: true,
				queryParam: 'site_id',
				getFilterValue: (item) => item.site_id,
				filterLabel: (id) => ctx.sites().find((n) => n.id === id)?.name ?? common_unknownSite(),
				groupable: true,
				getValue: (item) =>
					ctx.sites().find((n) => n.id == item.site_id)?.name || common_unknownSite(),
				display: { order: 2, getItems: (item) => siteItems(item.site_id, ctx.sites()) }
			},
			created_at: { label: common_created(), type: 'date', display: { hiddenByDefault: true } },
			updated_at: { label: common_updated(), type: 'date', display: { hiddenByDefault: true } },
			last_seen_at: {
				label: common_lastSeen(),
				type: 'date',
				staleFilter: true,
				display: {
					recency: true,
					order: 1,
					getItems: lastSeenItems(ctx.sites, 'Subnet')
				}
			}
		},
		[
			{
				key: 'utilization',
				label: common_utilization(),
				type: 'string',
				sortable: true,
				getValue: (subnet) => `${Math.round(utilizationRatio(subnet) * 100)}%`,
				compare: (a, b) => utilizationRatio(a) - utilizationRatio(b),
				display: { order: 5, cell: ctx.utilizationCell }
			},
			{
				key: 'description',
				searchable: true,
				display: { hiddenByDefault: true }
			},
			{
				key: 'source',
				label: common_source(),
				type: 'string',
				filterable: true,
				groupable: true,
				sortable: true,
				// Every source the backend can stamp.
				filterOptions: entitySources.getItems().map((source) => source.id),
				filterLabel: (id) => entitySources.getName(id),
				getFilterValue: (subnet) => subnet.source.type,
				getValue: (subnet) => entitySources.getName(subnet.source.type),
				display: {
					hiddenByDefault: true,
					getItems: (subnet) => entitySourceItems(subnet.source)
				}
			},
			{
				// The container runtime that created this bridge network. Only bridge subnets have one.
				key: 'managed_by',
				label: subnets_managedBy(),
				type: 'string',
				searchable: true,
				filterable: true,
				groupable: true,
				sortable: true,
				getValue: (subnet) => ctx.runtimeOf(subnet)?.name ?? null,
				getFilterValue: (subnet) => ctx.runtimeOf(subnet)?.id ?? null,
				filterLabel: (id) => ctx.runtimes().find((s) => s.id === id)?.name ?? id,
				display: {
					hiddenByDefault: true,
					getItems: (subnet) => {
						const runtime = ctx.runtimeOf(subnet);
						if (!runtime) return [];
						return [
							{
								id: runtime.id,
								label: runtime.name,
								color: concepts.getColorHelper('Containerization').color,
								entityRef: entityRef('Service', runtime.id, runtime)
							}
						];
					}
				}
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
