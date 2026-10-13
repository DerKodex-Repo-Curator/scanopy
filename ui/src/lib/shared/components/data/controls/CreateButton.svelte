<!--
	A list page's main action, last in its toolbar: Create, or the page's own word for it (Invite).
	C presses it while its list is on screen; the key shows in the button and dims while the table
	search has focus. Outside a list (no `DataControls` around it) it has no shortcut.
-->
<script lang="ts">
	import KbdKey from '$lib/shared/components/feedback/KbdKey.svelte';
	import { common_create, common_shortcutsPausedInSearch } from '$lib/paraglide/messages';
	import { isFreeKeydown, isPlainKey, useListShortcuts } from './listShortcuts';

	let {
		onclick,
		label = common_create()
	}: {
		onclick: () => void;
		label?: string;
	} = $props();

	const SHORTCUT = 'C';
	const shortcuts = useListShortcuts();
	let paused = $derived(shortcuts?.isPaused() ?? false);

	function handleKeydown(event: KeyboardEvent) {
		if (!shortcuts?.isActive() || !isPlainKey(event, SHORTCUT) || !isFreeKeydown(event)) return;
		event.preventDefault();
		onclick();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<button class="btn-primary toolbar-control flex items-center gap-2" {onclick}>
	{label}
	{#if shortcuts}
		<span
			class="inline-flex transition-opacity {paused ? 'opacity-40' : ''}"
			title={paused ? common_shortcutsPausedInSearch() : undefined}
		>
			<KbdKey key={SHORTCUT} size="xs" tone="inherit" />
		</span>
	{/if}
</button>
