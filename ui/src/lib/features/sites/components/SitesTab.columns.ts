import { credentialItems } from '$lib/features/credentials/columns';
import { tagNames } from '$lib/features/tags/columns';
import { entityRef, type FieldConfig } from '$lib/shared/components/data/types';
import { entities } from '$lib/shared/stores/metadata';
import type { Site } from '../types';
import type { Credential } from '$lib/features/credentials/types/base';
import type { Daemon } from '$lib/features/daemons/types/base';
import type { Host } from '$lib/features/hosts/types/base';
import type { Subnet } from '$lib/features/subnets/types/base';
import type { Tag } from '$lib/features/tags/types/base';
import type { Vlan } from '$lib/features/vlans/types/base';
import {
	common_created,
	common_credentials,
	common_daemons,
	common_subnets,
	common_vlans,
	common_name,
	common_tags,
	common_hoursCount,
	common_updated,
	sites_staleAfter,
	sites_staleAfterDefault
} from '$lib/paraglide/messages';

/** What the site columns read from the tab, each as a getter so values stay live. */
export interface SiteColumnsContext {
	tags: () => Tag[];
	daemons: () => Daemon[];
	/** The hosts the daemons run on, for each daemon chip's host name. */
	daemonHosts: () => Host[];
	/** Already narrowed to user-managed subnets. */
	subnets: () => Subnet[];
	vlans: () => Vlan[];
	credentials: () => Credential[];
}

// What a site contains, one resolver per column.
function siteDaemons(ctx: SiteColumnsContext, site: Site): Daemon[] {
	return ctx.daemons().filter((daemon) => daemon.site_id === site.id);
}

function siteSubnets(ctx: SiteColumnsContext, site: Site): Subnet[] {
	return ctx.subnets().filter((subnet) => subnet.site_id === site.id);
}

function siteVlans(ctx: SiteColumnsContext, site: Site): Vlan[] {
	return ctx.vlans().filter((vlan) => vlan.site_id === site.id);
}

function siteCredentials(ctx: SiteColumnsContext, site: Site): Credential[] {
	return (site.credential_ids ?? [])
		.map((id) => ctx.credentials().find((c) => c.id === id))
		.filter((c): c is Credential => Boolean(c));
}

export function siteColumns(ctx: SiteColumnsContext): FieldConfig<Site>[] {
	return [
		{
			key: 'name',
			label: common_name(),
			type: 'string',
			searchable: true,
			sortable: true,
			display: { primary: true, width: 220, order: 0 }
		},
		{
			key: 'vlans',
			label: common_vlans(),
			type: 'array',
			searchable: true,
			getValue: (site) => siteVlans(ctx, site).map((v) => v.name),
			display: {
				order: 1,
				getItems: (site) =>
					siteVlans(ctx, site).map((vlan) => ({
						id: vlan.id,
						label: vlan.name,
						color: entities.getColorHelper('Vlan').color,
						entityRef: entityRef('Vlan', vlan.id, vlan)
					}))
			}
		},
		{
			key: 'tags',
			label: common_tags(),
			type: 'array',
			searchable: true,
			filterable: true,
			getValue: (entity) => tagNames(entity.tags, ctx.tags())
		},
		{
			key: 'daemons',
			label: common_daemons(),
			type: 'array',
			searchable: true,
			getValue: (site) => siteDaemons(ctx, site).map((d) => d.name),
			display: {
				order: 3,
				getItems: (site) =>
					siteDaemons(ctx, site).map((daemon) => ({
						id: daemon.id,
						label: daemon.name,
						color: entities.getColorHelper('Daemon').color,
						entityRef: entityRef('Daemon', daemon.id, daemon, {
							hosts: ctx.daemonHosts(),
							subnets: ctx.subnets()
						})
					}))
			}
		},
		{
			key: 'credentials',
			label: common_credentials(),
			type: 'array',
			searchable: true,
			// Credentials are shared across sites, so this filters. The other arrays here hold
			// members of one site each, which search already finds.
			filterable: true,
			getValue: (site) => siteCredentials(ctx, site).map((c) => c.name),
			display: {
				order: 4,
				getItems: (site) => credentialItems(siteCredentials(ctx, site))
			}
		},
		{
			key: 'subnets',
			label: common_subnets(),
			type: 'array',
			searchable: true,
			getValue: (site) => siteSubnets(ctx, site).map((s) => s.name),
			display: {
				order: 2,
				getItems: (site) =>
					siteSubnets(ctx, site).map((subnet) => ({
						id: subnet.id,
						label: subnet.name,
						color: entities.getColorHelper('Subnet').color,
						entityRef: entityRef('Subnet', subnet.id, subnet)
					}))
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
		},
		{
			// The effective window, with the server default applied, so a site that
			// never set its own still shows the number staleness is judged by.
			key: 'effective_stale_after_hours',
			label: sites_staleAfter(),
			type: 'string',
			// A handful of values across sites; numeric collation orders 24 before 168.
			sortable: true,
			groupable: true,
			filterable: true,
			// Marked when no override is set, so an override equal to the default still reads as one.
			getValue: (site) => {
				if (site.effective_stale_after_hours == null) return null;
				const hours = common_hoursCount({ hours: site.effective_stale_after_hours });
				return site.stale_after_hours == null ? sites_staleAfterDefault({ hours }) : hours;
			},
			display: { hiddenByDefault: true }
		}
	];
}
