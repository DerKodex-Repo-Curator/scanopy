/**
 * The homepage's account of its recent scans: how each one went, and what is left to fix.
 *
 * Counts come from the server's per-run `warning_summary`; the rows behind "to fix" come from the
 * same `buildWarningReport` the run's own report uses, so a warning reads the same on both.
 */

import type { components } from '$lib/api/schema';
import warningRemedies from '$lib/data/warning-remedies.json';
import type { Discovery } from '$lib/features/discovery/types/base';
import type { DiscoveryUpdatePayload } from '$lib/features/discovery/types/api';
import { runOutcomeTag, type OutcomeTag } from '$lib/features/discovery/utils/outcome';
import {
	buildWarningReport,
	type EntityNameLookup,
	type WarningEntry,
	type WarningSection
} from '$lib/features/discovery/utils/warnings';
import { toColor } from '$lib/shared/utils/styling';
import { common_clean, home_scanNothingToFix, home_scanToFix } from '$lib/paraglide/messages';

export type RecentDiscovery = components['schemas']['RecentDiscovery'];

/** How one run went. `stopped` is a run that failed or was cancelled. */
export type RunState = 'clean' | 'toFix' | 'nothingToFix' | 'stopped';

export interface RecentRun {
	discovery: Discovery;
	state: RunState;
	tag: OutcomeTag | null;
}

/** One row of the "to fix" list, with the run it came from so its action can read the payload. */
export interface FixItem {
	key: string;
	entry: WarningEntry;
	discovery: Discovery;
	payload: DiscoveryUpdatePayload;
}

export interface FixSection extends Omit<WarningSection, 'entries'> {
	items: FixItem[];
}

export interface RecentScansView {
	runs: RecentRun[];
	/** Runs that completed with nothing to act on, whether or not they recorded warnings. */
	finished: number;
	toFix: number;
	stopped: number;
	sections: FixSection[];
	/** Rows past {@link MAX_FIX_ITEMS}, and the newest run holding one of them. */
	hidden: { count: number; discovery: Discovery } | null;
}

/** The most rows the homepage lists before handing the rest to the scan report. */
export const MAX_FIX_ITEMS = 3;

const ACTIONABLE_REMEDIES = new Set(
	warningRemedies.filter((r) => r.metadata?.actionable === true).map((r) => r.id)
);

function payloadOf(discovery: Discovery): DiscoveryUpdatePayload | null {
	return discovery.run_type.type === 'Historical' ? discovery.run_type.results : null;
}

function stateOf(recent: RecentDiscovery): RunState {
	const payload = payloadOf(recent.discovery);
	if (payload && payload.phase !== 'Complete') return 'stopped';
	if (recent.warning_summary.actionable > 0) return 'toFix';
	if (recent.warning_summary.total > 0) return 'nothingToFix';
	return 'clean';
}

function tagOf(recent: RecentDiscovery, state: RunState): OutcomeTag | null {
	switch (state) {
		case 'clean':
			return { label: common_clean(), color: toColor('Green') };
		case 'toFix':
			return {
				label: home_scanToFix({ count: recent.warning_summary.actionable }),
				color: toColor('Amber')
			};
		case 'nothingToFix':
			return {
				label: home_scanNothingToFix({ count: recent.warning_summary.total }),
				color: toColor('Gray')
			};
		case 'stopped':
			return runOutcomeTag(payloadOf(recent.discovery));
	}
}

/**
 * The scan a run belongs to. A newer run of the same scan replaces an older one's warnings, since
 * whatever the older run reported and the newer did not is no longer true. A rescan, or a run
 * recorded before the payload carried its scan, stands on its own.
 */
function scanKeyOf(discovery: Discovery, payload: DiscoveryUpdatePayload): string {
	if (payload.discovery_type.type === 'Rescan') return discovery.id;
	return payload.discovery_id ?? discovery.id;
}

export function recentScansView(
	recent: RecentDiscovery[],
	nameOfEntity?: EntityNameLookup
): RecentScansView {
	const runs = recent.map((r) => {
		const state = stateOf(r);
		return { discovery: r.discovery, state, tag: tagOf(r, state) };
	});

	// Runs arrive newest first, so the first run seen for a scan is its latest.
	const seenScans = new Set<string>();
	const latest: { discovery: Discovery; payload: DiscoveryUpdatePayload }[] = [];
	for (const r of recent) {
		const payload = payloadOf(r.discovery);
		if (!payload) continue;
		const key = scanKeyOf(r.discovery, payload);
		if (seenScans.has(key)) continue;
		seenScans.add(key);
		if (r.warning_summary.actionable > 0) latest.push({ discovery: r.discovery, payload });
	}

	// One section per actionable remedy, holding every latest run's rows on that rung. The same
	// row from two scans (one credential refused by both) is listed once.
	const sections = new Map<string, FixSection>();
	const seenItems = new Set<string>();
	for (const { discovery, payload } of latest) {
		for (const section of buildWarningReport(payload.warnings ?? [], nameOfEntity)) {
			if (!ACTIONABLE_REMEDIES.has(section.remedy)) continue;
			const { entries, ...heading } = section;
			const target = sections.get(section.remedy) ?? { ...heading, items: [] };
			sections.set(section.remedy, target);
			for (const entry of entries) {
				const key = [entry.code, ...entry.subjects.map((s) => s.label).sort()].join('|');
				if (seenItems.has(key)) continue;
				seenItems.add(key);
				target.items.push({ key, entry, discovery, payload });
			}
		}
	}

	// Sections in remedy order, then the cap applied across them.
	const ordered = warningRemedies.flatMap((r) => {
		const section = sections.get(r.id);
		return section ? [section] : [];
	});
	let room = MAX_FIX_ITEMS;
	const overflow: FixItem[] = [];
	const shown = ordered
		.map((section) => {
			const kept = section.items.slice(0, room);
			overflow.push(...section.items.slice(room));
			room -= kept.length;
			return { ...section, items: kept };
		})
		.filter((section) => section.items.length > 0);

	const newestHidden = overflow.reduce<Discovery | null>(
		(newest, item) =>
			!newest || item.discovery.created_at > newest.created_at ? item.discovery : newest,
		null
	);

	return {
		runs,
		finished: runs.filter((r) => r.state === 'clean' || r.state === 'nothingToFix').length,
		toFix: runs.filter((r) => r.state === 'toFix').length,
		stopped: runs.filter((r) => r.state === 'stopped').length,
		sections: shown,
		hidden: newestHidden ? { count: overflow.length, discovery: newestHidden } : null
	};
}
