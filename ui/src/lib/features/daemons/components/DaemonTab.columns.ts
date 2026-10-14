import type { Snippet } from 'svelte';
import type { Daemon } from '$lib/features/daemons/types/base';
import {
	daemonStatusTag,
	getDaemonStatus,
	getDaemonStatusTag,
	osLabel,
	daemonOsIds,
	DAEMON_STATUSES
} from '$lib/features/daemons/utils';
import { defineRegistryFields } from '$lib/shared/entity-fields/table';
import type { EntityLookup } from '$lib/shared/entity-fields';
import { tagNames } from '$lib/features/tags/columns';
import { siteItems } from '$lib/features/sites/columns';
import { entityRef } from '$lib/shared/components/data/types';
import { entities } from '$lib/shared/stores/metadata';
import type { Subnet } from '$lib/features/subnets/types/base';
import type { Site } from '$lib/features/sites/types';
import type { Tag } from '$lib/features/tags/types/base';
import type { Host } from '$lib/features/hosts/types/base';
import type { User } from '$lib/features/users/types';
import { hostDisplayName } from '$lib/features/hosts/host-display-name';
import type { components } from '$lib/api/schema';
import {
	common_operatingSystem,
	common_created,
	common_host,
	common_maintainer,
	common_site,
	common_status,
	common_tags,
	common_unknownEntity,
	common_unknownSite,
	common_updated,
	daemons_config_mode,
	daemons_interfacesWith,
	daemons_lastSeen,
	daemons_mode_daemonPoll,
	daemons_mode_daemonPollDescription,
	daemons_mode_serverPoll,
	daemons_mode_serverPollDescription
} from '$lib/paraglide/messages';

type DaemonOrderField = components['schemas']['DaemonOrderField'];

/** What the daemon columns read from the tab, each as a getter or function so values stay live. */
export interface DaemonColumnsContext {
	lookup: () => EntityLookup;
	sites: () => Site[];
	tags: () => Tag[];
	daemonHosts: () => Host[];
	maintainers: () => User[];
	interfacedSubnets: (daemon: Daemon) => Subnet[];
	/** The subnets the management lists show, to label the filter's options. */
	subnets: () => Subnet[];
	osCell: () => Snippet<[Daemon, string]>;
}

/**
 * Columns from the entity field registry; this adds how each one searches, filters and sorts.
 * Every DaemonOrderField value must be listed.
 */
export function daemonColumns(ctx: DaemonColumnsContext) {
	return defineRegistryFields<Daemon, DaemonOrderField>(
		'Daemon',
		ctx.lookup,
		{
			// Identity field: grouping by it would render a header per daemon.
			name: {
				searchable: true,
				groupable: false,
				display: { order: 0, primary: true, width: 220 }
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
				display: { order: 3, getItems: (item) => siteItems(item.site_id, ctx.sites()) }
			},
			last_seen: {
				label: daemons_lastSeen(),
				display: { order: 2, recency: true }
			},
			created_at: { label: common_created(), type: 'date', display: { hiddenByDefault: true } },
			updated_at: { label: common_updated(), type: 'date', display: { hiddenByDefault: true } }
		},
		[
			{
				// Host, version and subnet interfaces are display-only: none is a
				// DaemonOrderField, so the server cannot
				// order on them and defineRegistryFields rightly refuses them above.
				// One daemon per host, so the host neither groups nor filters; search finds it.
				key: 'host_id',
				label: common_host(),
				type: 'string',
				searchable: true,
				getValue: (daemon) => {
					const host = ctx.daemonHosts().find((h) => h.id === daemon.host_id);
					return host ? hostDisplayName(host) : common_unknownEntity({ entity: common_host() });
				},
				display: {
					hiddenByDefault: true,
					getItems: (daemon) => {
						const host = ctx.daemonHosts().find((h) => h.id === daemon.host_id);
						if (!host) return [];
						return [
							{
								id: host.id,
								label: hostDisplayName(host),
								color: entities.getColorHelper('Host').color,
								entityRef: entityRef('Host', host.id, host)
							}
						];
					}
				}
			},
			{
				// From getDaemonStatusTag, the same helper the home page uses, which
				// also carries version lifecycle (deprecated, unsupported).
				key: 'status',
				label: common_status(),
				type: 'string',
				searchable: true,
				filterable: true,
				groupable: true,
				sortable: true,
				getFilterValue: (daemon) => getDaemonStatus(daemon),
				filterLabel: (value) => {
					const status = DAEMON_STATUSES.find((s) => s === value);
					return status ? daemonStatusTag(status).label : value;
				},
				getValue: (daemon) => getDaemonStatusTag(daemon).label,
				display: {
					order: 1,
					getItems: (daemon) => {
						const tag = getDaemonStatusTag(daemon);
						return [
							{
								id: tag.label,
								label: tag.label,
								color: tag.color,
								icon: tag.icon,
								href: tag.href
							}
						];
					}
				}
			},
			{
				key: 'os',
				label: common_operatingSystem(),
				type: 'string',
				searchable: true,
				filterable: true,
				groupable: true,
				sortable: true,
				filterOptions: daemonOsIds,
				filterLabel: (value) => {
					const os = daemonOsIds.find((id) => id === value);
					return os ? osLabel(os) : value;
				},
				getFilterValue: (daemon) => daemon.os ?? null,
				getValue: (daemon) => (daemon.os ? osLabel(daemon.os) : null),
				display: { order: 4.5, cell: ctx.osCell() }
			},
			{
				key: 'mode',
				label: daemons_config_mode(),
				type: 'string',
				searchable: true,
				filterable: true,
				groupable: true,
				sortable: true,
				getFilterValue: (daemon) => daemon.mode,
				filterLabel: (mode) =>
					mode === 'server_poll' ? daemons_mode_serverPoll() : daemons_mode_daemonPoll(),
				getValue: (daemon) =>
					daemon.mode === 'server_poll' ? daemons_mode_serverPoll() : daemons_mode_daemonPoll(),
				display: {
					order: 5,
					getItems: (daemon) => [
						{
							id: daemon.mode,
							label:
								daemon.mode === 'server_poll'
									? daemons_mode_serverPoll()
									: daemons_mode_daemonPoll(),
							title:
								daemon.mode === 'server_poll'
									? daemons_mode_serverPollDescription()
									: daemons_mode_daemonPollDescription()
						}
					]
				}
			},
			{
				key: 'maintainer',
				label: common_maintainer(),
				type: 'string',
				searchable: true,
				filterable: true,
				groupable: true,
				sortable: true,
				getValue: (daemon) => ctx.maintainers().find((u) => u.id === daemon.user_id)?.email ?? null,
				getFilterValue: (daemon) => daemon.user_id ?? null,
				filterLabel: (id) => ctx.maintainers().find((u) => u.id === id)?.email ?? id,
				display: {
					hiddenByDefault: true,
					getItems: (daemon) => {
						const user = ctx.maintainers().find((u) => u.id === daemon.user_id);
						if (!user) return [];
						return [
							{
								id: user.id,
								label: user.email,
								color: entities.getColorHelper('User').color,
								entityRef: entityRef('User', user.id, user)
							}
						];
					}
				}
			},
			{
				key: 'version',
				searchable: true,
				filterable: true,
				// Numeric collation orders 0.17.10 after 0.17.9.
				sortable: true,
				groupable: true,
				getValue: (daemon) => daemon.version ?? '',
				display: { order: 4 }
			},
			{
				// The address the server dials. A DaemonPoll daemon dials out instead, so
				// its stored url is unused and the cell stays empty.
				key: 'url',
				searchable: true,
				getValue: (daemon) => (daemon.mode === 'server_poll' ? daemon.url : null),
				display: { hiddenByDefault: true }
			},
			{
				key: 'interfaced_subnet_ids',
				label: daemons_interfacesWith(),
				type: 'array',
				searchable: true,
				// Daemons share subnets, so this filters; as an array it neither sorts nor groups.
				filterable: true,
				getValue: (daemon) => ctx.interfacedSubnets(daemon).map((s) => s.name),
				getFilterValue: (daemon) => ctx.interfacedSubnets(daemon).map((s) => s.id),
				filterLabel: (id) => ctx.subnets().find((s) => s.id === id)?.name ?? id,
				display: {
					order: 6,
					getItems: (daemon) =>
						ctx.interfacedSubnets(daemon).map((subnet) => ({
							id: subnet.id,
							label: subnet.name,
							color: entities.getColorHelper('Subnet').color,
							entityRef: entityRef('Subnet', subnet.id, subnet)
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
			}
		]
	);
}
