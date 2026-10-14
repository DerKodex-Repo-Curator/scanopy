<script lang="ts">
	import { ShieldAlert } from 'lucide-svelte';
	import {
		license_expiredBanner,
		license_instanceRefusedBanner,
		license_invalidBanner
	} from '$lib/paraglide/messages';
	import AppBanner from './AppBanner.svelte';

	let {
		status,
		refusedFor = null
	}: {
		status: 'expired' | 'invalid';
		// Set when the cloud refused this server for running on more servers
		// than the plan covers.
		refusedFor?: { active: number; limit: number } | null;
	} = $props();
</script>

<AppBanner
	variant="danger"
	icon={ShieldAlert}
	body={status === 'expired'
		? license_expiredBanner()
		: refusedFor
			? license_instanceRefusedBanner(refusedFor)
			: license_invalidBanner()}
/>
