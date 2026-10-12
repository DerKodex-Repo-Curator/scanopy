/**
 * Where a warning row hands off to, when Scanopy owns the fix.
 *
 * Shared by the run's warning report and the homepage's recent-scans panel, so a warning's button
 * goes to the same place from both.
 */

import { navigateToEntity, openModal } from '$lib/shared/stores/modal-registry';
import { createIconComponent } from '$lib/shared/utils/styling';
import type { IconComponent } from '$lib/shared/utils/types';
import type { Credential } from '$lib/features/credentials/types/base';
import {
	common_credentials,
	daemons_upgradeDaemon,
	discovery_scanSettings,
	subnets_resolveRange
} from '$lib/paraglide/messages';
import type { DiscoveryUpdatePayload } from '../types/api';
import { credentialIdsOf, type WarningEntry } from './warnings';

export interface WarningAction {
	label: string;
	icon: IconComponent;
	run: () => void;
}

/** The codes whose answer is a scan setting rather than a credential. */
const SCAN_SETTING_CODES = new Set<WarningEntry['code']>([
	'ScanTimeLimit',
	'ScanTimeLimitWithEstimate',
	'WarningsTruncated'
]);

/**
 * Whether the run still has a scan to send the reader to.
 *
 * The historical row carries no parent, but the payload does. It is absent on records written
 * before the field existed, and it dangles for a rescan — that discovery is deleted the moment
 * the run finishes.
 */
function scanSettingsTarget(payload: DiscoveryUpdatePayload): string | null {
	return payload.discovery_type.type === 'Rescan' ? null : (payload.discovery_id ?? null);
}

/**
 * The action for one row of a run's warnings, or `null` where there is none.
 *
 * Only four destinations are real, and a row without one renders no button rather than a dead
 * one. The credential family is recognised by the `integration` field on the payload rather
 * than by a list of code names here — those eleven variants are exactly the ones that carry it,
 * so the set cannot drift out of step with the backend.
 *
 * `credentials` is whatever the caller has loaded, so a credential's editor opens on data already
 * in hand.
 */
export function warningAction(
	entry: WarningEntry,
	payload: DiscoveryUpdatePayload,
	credentials: Credential[]
): WarningAction | null {
	if (entry.code === 'OutdatedDaemonFormat') {
		return {
			label: daemons_upgradeDaemon(),
			icon: createIconComponent('circle-arrow-up'),
			run: () => {
				// The daemon to upgrade is the one that ran this scan, so the id comes off the
				// payload rather than the warning — one session has exactly one daemon, and
				// repeating it per warning would let the two disagree.
				window.location.hash = 'daemons';
				openModal('upgrade-daemon', { id: payload.daemon_id });
			}
		};
	}

	if (entry.code === 'ProvisionalSubnetInferred') {
		const subnetIds = entry.warnings.flatMap((w) => ('subnet_id' in w ? [w.subnet_id] : []));
		return {
			label: subnets_resolveRange(),
			icon: createIconComponent('cloud-alert'),
			run: () => {
				window.location.hash = 'subnets';
				// One range resolves in place; several is a trip to the list, because the
				// four-way choice is made per subnet and there is no "all of them" answer.
				if (subnetIds.length === 1) openModal('provisional-range', { id: subnetIds[0] });
			}
		};
	}

	const scanId = scanSettingsTarget(payload);
	if (scanId && SCAN_SETTING_CODES.has(entry.code)) {
		return {
			label: discovery_scanSettings(),
			icon: createIconComponent('sliders-horizontal'),
			run: () => {
				window.location.hash = 'discovery-scans';
				// Max Discovery Duration lives on Detection, not Performance.
				openModal('discovery-editor', { id: scanId, tab: 'detection' });
			}
		};
	}

	if (entry.warnings.some((w) => 'integration' in w)) {
		const [credentialId] = credentialIdsOf(entry);
		return {
			label: common_credentials(),
			icon: createIconComponent('key-round'),
			run: () => {
				// Any known id opens that credential's editor. Unlike an inferred range above,
				// where the four-way choice is genuinely per subnet, every credential on this
				// row needs the same thing done to it — so landing on the first is a head start
				// on the list, and the row's chips reach the others directly. Only a row with
				// no id at all — written before ids were carried, or posted by a daemon that
				// predates them — still goes to the list.
				if (!credentialId) {
					window.location.hash = 'credentials';
					return;
				}
				// The credential itself where it has loaded, so the editor opens on the id
				// alone rather than waiting for the credentials tab's own query to settle —
				// `resolveModalDeepLink` falls back to `entityData` for exactly this.
				navigateToEntity(
					'Credential',
					credentialId,
					credentials.find((c) => c.id === credentialId)
				);
			}
		};
	}

	return null;
}
