import type { Snippet } from 'svelte';
import { discoveryPhases, discoveryTypes } from '$lib/shared/stores/metadata';
import type { Discovery } from '../../types/base';
import { formatDuration } from '$lib/shared/utils/formatting';
import { defineRegistryFields } from '$lib/shared/entity-fields/table';
import type { EntityLookup } from '$lib/shared/entity-fields';
import { labelledFieldValueOptions, type FieldValueCount } from '$lib/shared/api/field-values';
import type { components } from '$lib/api/schema';
import { daemonItems } from '$lib/features/daemons/columns';
import { siteItems } from '$lib/features/sites/columns';
import { tagItems, tagNames } from '$lib/features/tags/columns';
import { toColor } from '$lib/shared/utils/styling';
import type { OutcomeTag } from '../../utils/outcome';
import type { Daemon } from '$lib/features/daemons/types/base';
import type { Site } from '$lib/features/sites/types';
import type { Tag, TagGroup } from '$lib/features/tags/types/base';
import {
	common_created,
	common_daemon,
	common_duration,
	common_status,
	common_tags,
	common_warnings,
	common_site,
	common_type,
	common_unknown,
	common_unknownEntity,
	common_unknownSite,
	common_updated,
	discovery_finishedAt,
	discovery_startedAt
} from '$lib/paraglide/messages';

type DiscoveryOrderField = components['schemas']['DiscoveryOrderField'];
type DiscoveryResults = Extract<Discovery['run_type'], { type: 'Historical' }>['results'];

/** What the run history columns read from the tab, each as a getter or function so values stay live. */
export interface DiscoveryHistoryColumnsContext {
	lookup: () => EntityLookup;
	daemons: () => Daemon[];
	sites: () => Site[];
	tags: () => Tag[];
	tagGroups: () => TagGroup[] | undefined;
	daemonValues: () => FieldValueCount[] | undefined;
	siteValues: () => FieldValueCount[] | undefined;
	typeValues: () => FieldValueCount[] | undefined;
	phaseValues: () => FieldValueCount[] | undefined;
	resultsOf: (discovery: Discovery) => DiscoveryResults | null;
	outcomeTag: (discovery: Discovery) => OutcomeTag | null;
	warningsOf: (discovery: Discovery) => NonNullable<DiscoveryResults['warnings']>;
	startedAtCell: () => Snippet<[Discovery, string]>;
	finishedAtCell: () => Snippet<[Discovery, string]>;
}

export function discoveryHistoryColumns(ctx: DiscoveryHistoryColumnsContext) {
	return defineRegistryFields<Discovery, DiscoveryOrderField>(
		'Discovery',
		ctx.lookup,
		{
			// Runs of one scan share its name, so grouping by it gathers a scan's history.
			name: {
				searchable: true,
				display: { order: 0 }
			},
			daemon_id: {
				label: common_daemon(),
				type: 'string',
				searchable: true,
				filterable: true,
				serverFiltered: true,
				// The daemons some run was made by, by name.
				filterOptions: labelledFieldValueOptions(
					ctx.daemonValues(),
					(id) => ctx.daemons().find((d) => d.id === id)?.name
				),
				groupable: true,
				// Displayed as a name, but grouped by id on the server.
				getGroupValue: (item) => item.daemon_id,
				getValue: (item) =>
					ctx.daemons().find((d) => d.id === item.daemon_id)?.name ??
					common_unknownEntity({ entity: common_daemon() }),
				display: { order: 7, getItems: (item) => daemonItems(item.daemon_id, ctx.daemons()) }
			},
			site_id: {
				label: common_site(),
				type: 'string',
				searchable: true,
				filterable: true,
				serverFiltered: true,
				// The sites some run scanned, by name.
				filterOptions: labelledFieldValueOptions(
					ctx.siteValues(),
					(id) => ctx.sites().find((n) => n.id === id)?.name
				),
				groupable: true,
				getGroupValue: (item) => item.site_id,
				getValue: (item) =>
					ctx.sites().find((n) => n.id === item.site_id)?.name ?? common_unknownSite(),
				display: { order: 6, getItems: (item) => siteItems(item.site_id, ctx.sites()) }
			},
			discovery_type: {
				label: common_type(),
				type: 'string',
				searchable: true,
				filterable: true,
				serverFiltered: true,
				// The types some run had, by name.
				filterOptions: labelledFieldValueOptions(ctx.typeValues(), (id) =>
					discoveryTypes.getName(id)
				),
				groupable: true,
				// The server groups on the raw discriminant; the column renders its name.
				getGroupValue: (item) => item.discovery_type.type,
				getValue: (item) => discoveryTypes.getName(item.discovery_type.type),
				display: { hiddenByDefault: true }
			},
			created_at: { label: common_created(), type: 'date', display: { hiddenByDefault: true } },
			updated_at: { label: common_updated(), type: 'date', display: { hiddenByDefault: true } },
			// The run's outcome. Sorted, grouped and
			// filtered by the terminal phase; the cell shows the outcome tag, whose hover carries
			// the reason the run ended.
			phase: {
				label: common_status(),
				type: 'string',
				searchable: true,
				filterable: true,
				serverFiltered: true,
				// The phases some run ended in, by name.
				filterOptions: labelledFieldValueOptions(ctx.phaseValues(), (id) =>
					discoveryPhases.getName(id)
				),
				getValue: (item) => {
					const phase = ctx.resultsOf(item)?.phase;
					return phase ? discoveryPhases.getName(phase) : null;
				},
				// The server groups on the raw phase id.
				getGroupValue: (item) => ctx.resultsOf(item)?.phase ?? null,
				display: {
					order: 1,
					getItems: (item) => {
						const tag = ctx.outcomeTag(item);
						return tag
							? [{ id: tag.label, label: tag.label, color: tag.color, title: tag.title }]
							: [];
					}
				}
			},
			// Read out of the run's results. Near-unique timestamps and continuous values, so
			// they sort but do not group.
			started_at: {
				label: discovery_startedAt(),
				type: 'date',
				getValue: (item) => {
					const startedAt = ctx.resultsOf(item)?.started_at;
					return startedAt ? new Date(startedAt) : null;
				},
				display: { order: 3, cell: ctx.startedAtCell() }
			},
			finished_at: {
				label: discovery_finishedAt(),
				type: 'date',
				getValue: (item) => {
					const finishedAt = ctx.resultsOf(item)?.finished_at;
					return finishedAt ? new Date(finishedAt) : null;
				},
				display: { order: 4, cell: ctx.finishedAtCell() }
			},
			duration: {
				label: common_duration(),
				type: 'string',
				groupable: false,
				getValue: (item) => {
					const results = ctx.resultsOf(item);
					if (results && results.finished_at && results.started_at) {
						return formatDuration(results.started_at, results.finished_at);
					}
					return common_unknown();
				},
				display: { order: 5 }
			},
			// How much the run had to say, next to how it ended — the two questions asked
			// together. Always amber, never red: this column counts warnings, and a run
			// that failed outright says so in Status. A clean run falls back to its plain
			// 0 rather than wearing a chip that means nothing. A count, so it sorts but does
			// not group.
			warnings: {
				label: common_warnings(),
				type: 'string',
				groupable: false,
				getValue: (item) => String(ctx.warningsOf(item).length),
				display: {
					order: 2,
					getItems: (item) => {
						const count = ctx.warningsOf(item).length;
						if (count === 0) return undefined;
						return [{ id: 'warnings', label: String(count), color: toColor('amber') }];
					}
				}
			}
		},
		[
			{
				// The server writes runs with no tags, but a tag can be added to one through the API.
				key: 'tags',
				label: common_tags(),
				type: 'array',
				getValue: (item) => tagNames(item.tags, ctx.tags()),
				display: {
					hiddenByDefault: true,
					getItems: (item) => tagItems(item.tags, ctx.tags(), ctx.tagGroups() ?? [])
				}
			}
		],
		// Every run here is historical, so a run-type column would read the same on every row.
		['run_type']
	);
}
