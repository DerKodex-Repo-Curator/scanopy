import type { Snippet } from 'svelte';
import { discoveryTypes } from '$lib/shared/stores/metadata';
import type { Discovery } from '../../types/base';
import {
	discoveryFields,
	formatScheduleDisplay,
	type DiscoveryFieldEntry,
	type DiscoveryConfigOrderField
} from '../../queries';
import type { DisplayConfig } from '$lib/shared/components/data/types';
import { defineRegistryFields } from '$lib/shared/entity-fields/table';
import type { EntityLookup } from '$lib/shared/entity-fields';
import type { DiscoveryUpdatePayload } from '../../types/api';
import type { Daemon } from '$lib/features/daemons/types/base';
import type { Site } from '$lib/features/sites/types';
import type { Tag } from '$lib/features/tags/types/base';
import {
	common_lastRun,
	common_legacy,
	common_manual,
	common_progress,
	common_schedule,
	common_status,
	discovery_schedulePausedFreePlan,
	discovery_schedulePausedLapsed,
	common_tags,
	discovery_completedScans,
	discovery_forceFullScan,
	discovery_legacyTagTitle,
	discovery_runType
} from '$lib/paraglide/messages';

/**
 * How the shared discovery fields sit on this tab. `discovery_type` is the same
 * on every scheduled scan, so it is off by default and serves mostly as a filter
 * and group axis. The audit dates stay available from the column menu.
 */
const SHARED_FIELD_DISPLAY: Partial<Record<DiscoveryConfigOrderField, DisplayConfig<Discovery>>> = {
	name: { order: 0 },
	site_id: { order: 2 },
	daemon_id: { order: 3 },
	discovery_type: { hiddenByDefault: true },
	created_at: { hiddenByDefault: true },
	updated_at: { hiddenByDefault: true }
};

function withSharedDisplay(
	shared: Record<DiscoveryConfigOrderField, DiscoveryFieldEntry>
): Record<DiscoveryConfigOrderField, DiscoveryFieldEntry> {
	const fields = { ...shared };
	for (const [key, overrides] of Object.entries(SHARED_FIELD_DISPLAY)) {
		const orderField = key as DiscoveryConfigOrderField;
		fields[orderField] = {
			...fields[orderField],
			display: { ...fields[orderField].display, ...overrides }
		};
	}
	return fields;
}

/** What the scheduled scan columns read from the tab, each as a getter or function so values stay live. */
export interface DiscoveryScheduledColumnsContext {
	lookup: () => EntityLookup;
	daemons: () => Daemon[];
	sites: () => Site[];
	tags: () => Tag[];
	scheduleLapsed: () => boolean;
	schedulePaused: () => boolean;
	getActiveSession: (discovery: Discovery) => DiscoveryUpdatePayload | null;
	progressCell: () => Snippet<[Discovery, string]>;
}

export function discoveryScheduledColumns(ctx: DiscoveryScheduledColumnsContext) {
	return defineRegistryFields<Discovery, DiscoveryConfigOrderField>(
		'Discovery',
		ctx.lookup,
		withSharedDisplay(discoveryFields(ctx.daemons(), ctx.sites())),
		[
			{
				key: 'scan_count',
				label: discovery_completedScans(),
				type: 'string',
				// A count: it sorts, but every value would be its own group.
				sortable: true,
				getValue: (item) => String(item.scan_count ?? 0),
				display: { hiddenByDefault: true, align: 'right' }
			},
			{
				key: 'force_full_scan',
				label: discovery_forceFullScan(),
				type: 'boolean',
				filterable: true,
				groupable: true,
				getValue: (item) => item.force_full_scan ?? false,
				display: { hiddenByDefault: true }
			},
			{
				key: 'legacy',
				label: common_status(),
				type: 'string',
				filterable: true,
				groupable: true,
				sortable: true,
				// Legacy-ness comes from the backend's own `is_legacy`, not a local list —
				// a `!== 'Unified'` check flagged Rescan, which is new rather than frozen.
				getValue: (item) =>
					discoveryTypes.getMetadata(item.discovery_type.type).is_legacy ? common_legacy() : '',
				getFilterValue: (item) =>
					discoveryTypes.getMetadata(item.discovery_type.type).is_legacy ? 'legacy' : null,
				filterLabel: () => common_legacy(),
				display: {
					getItems: (item) =>
						discoveryTypes.getMetadata(item.discovery_type.type).is_legacy
							? [
									{
										id: 'legacy',
										label: common_legacy(),
										color: 'Yellow',
										title: discovery_legacyTagTitle()
									}
								]
							: []
				}
			},
			{
				key: 'run_type',
				label: discovery_runType(),
				searchable: true,
				filterable: true,
				groupable: true,
				sortable: true,
				display: { order: 1 }
			},
			{
				key: 'schedule',
				label: common_schedule(),
				type: 'string',
				searchable: true,
				getValue: (item) =>
					item.run_type.type !== 'Scheduled'
						? common_manual()
						: ctx.scheduleLapsed()
							? discovery_schedulePausedLapsed()
							: ctx.schedulePaused()
								? discovery_schedulePausedFreePlan()
								: formatScheduleDisplay(item.run_type.cron_schedule, item.run_type.timezone),
				display: { hiddenByDefault: true }
			},
			{
				key: 'last_run',
				label: common_lastRun(),
				// A date column so it sorts by time. The ISO string rather than a Date, so the cell
				// renders it as recent activity (a Date renders as a compact date).
				type: 'date',
				sortable: true,
				getValue: (item) =>
					item.run_type.type !== 'Historical' && item.run_type.last_run
						? item.run_type.last_run
						: null,
				display: { recency: true }
			},
			{
				key: 'progress',
				label: common_progress(),
				// Progress belongs to the run, not the record, so there is nothing to
				// sort, filter or group by — but it is still a field, so a running scan
				// shows its tracker in the table.
				type: 'string',
				getValue: (item) => ctx.getActiveSession(item)?.phase ?? '',
				// After the tags column, immediately before the row actions: it is the
				// row's live state rather than one of its attributes.
				display: { trailing: true, cell: ctx.progressCell() }
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
			}
		]
	);
}
