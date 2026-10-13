import type { DiscoveryType } from '../types/api';

/**
 * Whether a session or run scans all 65,535 ports. The server decides this at dispatch and
 * records it on the session's scan settings; only Unified discoveries port-scan.
 */
export function isFullScan(discoveryType: DiscoveryType): boolean {
	return discoveryType.type === 'Unified' && discoveryType.scan_settings?.is_full_scan === true;
}
