import { describe, expect, it } from 'vitest';
import {
	MAX_FIX_ITEMS,
	recentScansView,
	type RecentDiscovery
} from '$lib/features/home/utils/recentScans';
import { runOutcomeTag } from '$lib/features/discovery/utils/outcome';
import type { DiscoveryWarning } from '$lib/features/discovery/utils/warnings';
import type { DiscoveryUpdatePayload } from '$lib/features/discovery/types/api';

const SCAN_A = '00000000-0000-0000-0000-00000000000a';
const SCAN_B = '00000000-0000-0000-0000-00000000000b';
const CRED = '00000000-0000-0000-0000-0000000000c1';

const credentialFailure = (code: string, address = '10.0.0.1'): DiscoveryWarning =>
	({ code, integration: 'Snmp', address, detail: null, credential_id: CRED }) as DiscoveryWarning;

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
	scan?: string;
	phase?: DiscoveryUpdatePayload['phase'];
	reason?: DiscoveryUpdatePayload['reason'];
	warnings?: DiscoveryWarning[];
	actionable?: number;
	createdAt?: string;
}): RecentDiscovery {
	const id = `run-${nextId++}`;
	const warnings = opts.warnings ?? [];
	return {
		discovery: {
			id,
			created_at: opts.createdAt ?? '2026-10-01T00:00:00Z',
			updated_at: opts.createdAt ?? '2026-10-01T00:00:00Z',
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
					discovery_id: opts.scan ?? SCAN_A,
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
		const view = recentScansView([]);

		expect(view.runs).toEqual([]);
		expect([view.finished, view.toFix, view.stopped]).toEqual([0, 0, 0]);
		expect(view.sections).toEqual([]);
		expect(view.hidden).toBeNull();
	});

	it('counts a run without warnings as finished, with nothing to fix', () => {
		const view = recentScansView([run({})]);

		expect(view.runs[0].state).toBe('clean');
		expect(view.finished).toBe(1);
		expect(view.sections).toEqual([]);
	});

	it('lists the rows to fix for a run with actionable warnings', () => {
		const view = recentScansView([
			run({ warnings: [credentialFailure('CredentialRejected'), deviceQuirk], actionable: 1 })
		]);

		expect(view.runs[0].state).toBe('toFix');
		expect(view.toFix).toBe(1);
		expect(view.finished).toBe(0);
		const codes = view.sections.flatMap((s) => s.items.map((i) => i.entry.code));
		expect(codes).toEqual(['CredentialRejected']);
		expect(view.sections[0].items[0].payload.discovery_id).toBe(SCAN_A);
	});

	it('keeps warnings that ask nothing of the reader on the run tag only', () => {
		const view = recentScansView([run({ warnings: [deviceQuirk] })]);

		expect(view.runs[0].state).toBe('nothingToFix');
		expect(view.finished).toBe(1);
		expect(view.sections).toEqual([]);
	});

	it('shows a failed run with its outcome and counts it apart', () => {
		const failed = run({ phase: 'Failed', reason: 'DaemonUnreachable' });
		const view = recentScansView([failed]);

		expect(view.runs[0].state).toBe('stopped');
		expect(view.runs[0].tag).toEqual(
			runOutcomeTag({ phase: 'Failed', reason: 'DaemonUnreachable' })
		);
		expect(view.stopped).toBe(1);
		expect(view.finished).toBe(0);
	});

	it('drops what an older run reported once a newer run of the same scan is clean', () => {
		const view = recentScansView([
			run({ scan: SCAN_A }),
			run({ scan: SCAN_A, warnings: [credentialFailure('CredentialRejected')], actionable: 1 })
		]);

		expect(view.runs.map((r) => r.state)).toEqual(['clean', 'toFix']);
		expect(view.sections).toEqual([]);
	});

	it('lists the same row from two scans once', () => {
		const view = recentScansView([
			run({ scan: SCAN_A, warnings: [credentialFailure('CredentialRejected')], actionable: 1 }),
			run({ scan: SCAN_B, warnings: [credentialFailure('CredentialRejected')], actionable: 1 })
		]);

		expect(view.sections.flatMap((s) => s.items)).toHaveLength(1);
	});

	it('caps the list and points the rest at the newest run holding them', () => {
		const codes = [
			'CredentialRejected',
			'CredentialMalformed',
			'CredentialTlsFailed',
			'CredentialUnreachable',
			'CredentialTimedOut'
		];
		const older = run({
			scan: SCAN_B,
			warnings: codes.slice(0, 3).map((c) => credentialFailure(c)),
			actionable: 3,
			createdAt: '2026-09-01T00:00:00Z'
		});
		const newer = run({
			scan: SCAN_A,
			warnings: codes.slice(3).map((c) => credentialFailure(c)),
			actionable: 2,
			createdAt: '2026-10-01T00:00:00Z'
		});

		const view = recentScansView([newer, older]);

		expect(view.sections.flatMap((s) => s.items)).toHaveLength(MAX_FIX_ITEMS);
		expect(view.hidden?.count).toBe(codes.length - MAX_FIX_ITEMS);
		expect(view.hidden?.discovery.id).toBe(older.discovery.id);
	});
});
