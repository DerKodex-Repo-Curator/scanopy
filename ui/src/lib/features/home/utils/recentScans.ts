/**
 * The homepage's tag for each recent scan: how many warnings need the reader, how many need
 * nothing, or how the run stopped. A run that finished without warnings carries no tag.
 *
 * Counts come from the server's per-run `warning_summary`, which classifies each warning by its
 * remedy, so the homepage holds no copy of that classification.
 */

import type { components } from '$lib/api/schema';
import type { Discovery } from '$lib/features/discovery/types/base';
import { runOutcomeTag, type OutcomeTag } from '$lib/features/discovery/utils/outcome';
import { toColor } from '$lib/shared/utils/styling';
import { home_scanNothingToFix, home_scanToFix } from '$lib/paraglide/messages';

export type RecentDiscovery = components['schemas']['RecentDiscovery'];

/** How one run went. `stopped` is a run that failed or was cancelled. */
export type RunState = 'clean' | 'toFix' | 'nothingToFix' | 'stopped';

export interface RecentRun {
	discovery: Discovery;
	state: RunState;
	tag: OutcomeTag | null;
}

function stateOf(recent: RecentDiscovery): RunState {
	const { run_type } = recent.discovery;
	if (run_type.type === 'Historical' && run_type.results.phase !== 'Complete') return 'stopped';
	if (recent.warning_summary.actionable > 0) return 'toFix';
	if (recent.warning_summary.total > 0) return 'nothingToFix';
	return 'clean';
}

function tagOf(recent: RecentDiscovery, state: RunState): OutcomeTag | null {
	switch (state) {
		case 'clean':
			return null;
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
		case 'stopped': {
			const { run_type } = recent.discovery;
			return runOutcomeTag(run_type.type === 'Historical' ? run_type.results : null);
		}
	}
}

export function recentScansView(recent: RecentDiscovery[]): RecentRun[] {
	return recent.map((r) => {
		const state = stateOf(r);
		return { discovery: r.discovery, state, tag: tagOf(r, state) };
	});
}
