<script lang="ts">
	import { ArrowUpCircle } from 'lucide-svelte';
	import AppBanner from './AppBanner.svelte';
	import { docsUrl } from '$lib/shared/utils/docs';
	import { VERSION } from '$lib/version';
	import {
		upgradeBanner_body,
		upgradeBanner_howToUpgrade,
		upgradeBanner_releaseNotes
	} from '$lib/paraglide/messages';
	import type { PublicServerConfig } from '$lib/shared/stores/config-query';

	let { upgrade }: { upgrade: NonNullable<PublicServerConfig['available_upgrade']> } = $props();
</script>

<!-- Keyed per version: dismissing hides this release, and the next one shows again. -->
<AppBanner
	variant="info"
	icon={ArrowUpCircle}
	body={upgradeBanner_body({ latest: upgrade.version, current: VERSION })}
	dismissableKey={`upgrade_available:v${upgrade.version}`}
>
	{#snippet actions()}
		<!-- eslint-disable svelte/no-navigation-without-resolve -->
		<a
			href={upgrade.release_notes_url}
			target="_blank"
			rel="noopener noreferrer"
			class="ml-2 rounded px-2 py-0.5 text-xs font-medium underline hover:no-underline"
		>
			{upgradeBanner_releaseNotes()}
		</a>
		<a
			href={docsUrl('/docs/self-hosted-server/upgrading/')}
			target="_blank"
			rel="noopener noreferrer"
			class="ml-2 rounded px-2 py-0.5 text-xs font-medium underline hover:no-underline"
		>
			{upgradeBanner_howToUpgrade()}
		</a>
		<!-- eslint-enable svelte/no-navigation-without-resolve -->
	{/snippet}
</AppBanner>
