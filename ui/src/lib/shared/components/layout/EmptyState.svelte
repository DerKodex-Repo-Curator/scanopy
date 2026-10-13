<script lang="ts">
	import { Search } from 'lucide-svelte';
	import type { Snippet } from 'svelte';
	import type { IconComponent as IconComponentType } from '$lib/shared/utils/types';
	import KbdKey from '$lib/shared/components/feedback/KbdKey.svelte';
	import { isOnVisiblePage } from '$lib/shared/stores/modal-registry';
	import { isFreeKeydown, isPlainKey } from '$lib/shared/components/data/controls/listShortcuts';

	let {
		IconComponent = Search,
		title = '',
		subtitle = '',
		onClick = () => {},
		cta = '',
		children
	}: {
		IconComponent?: IconComponentType;
		title?: string;
		subtitle?: string;
		onClick?: () => void;
		cta?: string;
		children?: Snippet;
	} = $props();

	// C presses the CTA, as it presses Create on a list: an empty page's CTA is its first item.
	const SHORTCUT = 'C';
	let root: HTMLDivElement | undefined = $state();
	let hasCta = $derived(!children && cta.length > 0);

	function handleKeydown(event: KeyboardEvent) {
		if (!hasCta || !isPlainKey(event, SHORTCUT) || !isFreeKeydown(event)) return;
		if (!isOnVisiblePage(root)) return;
		event.preventDefault();
		onClick();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<div bind:this={root} class="py-12 text-center">
	<IconComponent size={16} class="text-secondary mx-auto mb-4 h-16 w-16" />
	<p class="text-secondary mb-4 text-lg">{title}</p>
	<p class="text-tertiary mb-6">{subtitle}</p>
	{#if children}
		{@render children()}
	{:else if hasCta}
		<button onclick={onClick} class="btn-primary gap-2">
			{cta}
			<KbdKey key={SHORTCUT} size="xs" tone="inherit" />
		</button>
	{/if}
</div>
