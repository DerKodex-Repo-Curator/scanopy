import { describe, expect, it } from 'vitest';
import { recentScansView, type RecentDiscovery } from '$lib/features/home/utils/recentScans';
import { runOutcomeTag } from '$lib/features/discovery/utils/outcome';
import { toColor } from '$lib/shared/utils/styling';
import type { DiscoveryWarning } from '$lib/features/discovery/utils/warnings';
import type { DiscoveryUpdatePayload } from '$lib/features/discovery/types/api';

const credentialFailure = {
	code: 'CredentialRejected',
	integration: 'Snmp',
	address: '10.0.0.1',
	detail: null,
	credential_id: '00000000-0000-0000-0000-0000000000c1'
} as DiscoveryWarning;

const deviceQuirk = {
	code: 'SnmpWalkUnsupported',
	address: '10.0.0.2',
	group: 'Lldp'
} as DiscoveryWarning;

let nextId = 0;

/**
 * A historical run as the dashboard returns it. The summary counts are given rather than derived:
 * the server computes them, and these tests cover what the homepage does with them.
 */
function run(opts: {
	phase?: DiscoveryUpdatePayload['phase'];
	reason?: DiscoveryUpdatePayload['reason'];
	warnings?: DiscoveryWarning[];
	actionable?: number;
}): RecentDiscovery {
	const id = `run-${nextId++}`;
	const warnings = opts.warnings ?? [];
	return {
		discovery: {
			id,
			created_at: '2026-10-01T00:00:00Z',
			updated_at: '2026-10-01T00:00:00Z',
			name: 'Network Scan',
			daemon_id: 'daemon',
			site_id: 'site',
			tags: [],
			discovery_type: { type: 'Network' },
			run_type: {
				type: 'Historical',
				results: {
					phase: opts.phase ?? 'Complete',
					reason: opts.reason ?? 'Completed',
					discovery_id: 'scan',
					discovery_type: { type: 'Network' },
					daemon_id: 'daemon',
					site_id: 'site',
					session_id: id,
					progress: 100,
					warnings
				}
			}
		},
		warning_summary: {
			by_remedy: [],
			actionable: opts.actionable ?? 0,
			total: warnings.length
		}
	} as unknown as RecentDiscovery;
}

describe('recentScansView', () => {
	it('has nothing to show before any scan has finished', () => {
		expect(recentScansView([])).toEqual([]);
	});

	it('leaves a run without warnings untagged', () => {
		const [view] = recentScansView([run({})]);

		expect(view.state).toBe('clean');
		expect(view.tag).toBeNull();
	});

	it('tags a run with actionable warnings for attention', () => {
		const [view] = recentScansView([
			run({ warnings: [credentialFailure, deviceQuirk], actionable: 1 })
		]);

		expect(view.state).toBe('toFix');
		expect(view.tag?.color).toBe(toColor('Amber'));
	});

	it('tags a run whose warnings ask nothing of the reader quietly', () => {
		const [view] = recentScansView([run({ warnings: [deviceQuirk] })]);

		expect(view.state).toBe('nothingToFix');
		expect(view.tag?.color).toBe(toColor('Gray'));
	});

	it('tags a failed run with its outcome, whatever its warnings', () => {
		const [view] = recentScansView([
			run({
				phase: 'Failed',
				reason: 'DaemonUnreachable',
				warnings: [credentialFailure],
				actionable: 1
			})
		]);

		expect(view.state).toBe('stopped');
		expect(view.tag).toEqual(runOutcomeTag({ phase: 'Failed', reason: 'DaemonUnreachable' }));
	});
});
