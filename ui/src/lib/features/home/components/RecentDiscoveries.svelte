<!--
	Recent scans, with a tag on each run that did not finish cleanly: how many warnings need the
	reader, how many need nothing, or how the run stopped. The rows themselves live in the run's
	own report, one click away.
-->
<script lang="ts">
	import type { Discovery } from '$lib/features/discovery/types/base';
	import type { Daemon } from '$lib/features/daemons/types/base';
	import type { components } from '$lib/api/schema';
	import HomeDiscoveryDisplay from './HomeDiscoveryDisplay.svelte';
	import { recentScansView, type RecentDiscovery } from '../utils/recentScans';
	import { home_noScansYet, home_recentScans } from '$lib/paraglide/messages';

	type SiteSummary = components['schemas']['SiteSummary'];

	let {
		recent,
		daemons = [],
		sites = [],
		onOpenRun
	}: {
		recent: RecentDiscovery[];
		daemons?: Daemon[];
		sites?: SiteSummary[];
		onOpenRun?: (discovery: Discovery) => void;
	} = $props();

	let runs = $derived(recentScansView(recent));
</script>

<section>
	<h3 class="text-primary mb-3 text-base font-semibold">{home_recentScans()}</h3>
	{#if runs.length === 0}
		<p class="text-tertiary text-sm">{home_noScansYet()}</p>
	{:else}
		<div class="grid grid-cols-[repeat(auto-fill,minmax(360px,1fr))] gap-4">
			{#each runs as run (run.discovery.id)}
				<button
					class="card card-static cursor-pointer text-left"
					onclick={() => onOpenRun?.(run.discovery)}
				>
					<HomeDiscoveryDisplay item={run} context={{ daemons, sites }} />
				</button>
			{/each}
		</div>
	{/if}
</section>
