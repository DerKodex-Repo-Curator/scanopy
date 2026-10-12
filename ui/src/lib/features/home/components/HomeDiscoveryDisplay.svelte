<script lang="ts" module>
	import { entities } from '$lib/shared/stores/metadata';
	import { formatTimestamp } from '$lib/shared/utils/formatting';
	import type { Daemon } from '$lib/features/daemons/types/base';
	import type { components } from '$lib/api/schema';
	import type { RecentRun } from '../utils/recentScans';

	type SiteSummary = components['schemas']['SiteSummary'];

	export interface HomeDiscoveryContext {
		daemons: Daemon[];
		sites: SiteSummary[];
	}

	export const HomeDiscoveryDisplay: EntityDisplayComponent<RecentRun, HomeDiscoveryContext> = {
		getId: (run) => run.discovery.id,
		getLabel: ({ discovery }, context) => {
			// New records already have enriched names ("Type — Site").
			// Old records missing the separator get enriched client-side.
			if (discovery.name.includes(' — ')) return discovery.name;
			const site = context?.sites.find((n) => n.id === discovery.site_id);
			if (site) return `${discovery.name} — ${site.name}`;
			return discovery.name;
		},
		getDescription: ({ discovery }, context) => {
			const daemon = context.daemons.find((d) => d.id === discovery.daemon_id);
			const daemonName = daemon?.name ?? 'Unknown Daemon';
			return `${daemonName} · ${formatTimestamp(discovery.created_at)}`;
		},
		getIcon: () => entities.getIconComponent('Discovery'),
		getIconColor: () => entities.getColorHelper('Discovery').icon,
		getTags: (run) => (run.tag ? [run.tag] : [])
	};
</script>

<script lang="ts">
	import type { EntityDisplayComponent } from '$lib/shared/components/forms/selection/types';
	import ListSelectItem from '$lib/shared/components/forms/selection/ListSelectItem.svelte';

	let {
		item,
		context = { daemons: [], sites: [] }
	}: { item: RecentRun; context?: HomeDiscoveryContext } = $props();
</script>

<ListSelectItem {item} {context} displayComponent={HomeDiscoveryDisplay} />
