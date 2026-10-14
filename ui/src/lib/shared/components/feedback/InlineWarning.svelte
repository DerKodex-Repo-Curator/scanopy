<script lang="ts">
	import { AlertTriangle } from 'lucide-svelte';
	import type { Snippet } from 'svelte';
	import BaseInlineFeedback from './BaseInlineFeedback.svelte';

	let {
		title = 'Error',
		body = null,
		items = null,
		dismissableKey = null,
		children
	}: {
		title?: string;
		body?: string | null;
		/** Independent statements, rendered as a bulleted list. See `BaseInlineFeedback`. */
		items?: string[] | null;
		dismissableKey?: string | null;
		/** An action that belongs to the message. See `BaseInlineFeedback`. */
		children?: Snippet;
	} = $props();
</script>

<BaseInlineFeedback
	{title}
	{body}
	{items}
	{dismissableKey}
	{children}
	Icon={AlertTriangle}
	borderColor="border-yellow-400 dark:border-yellow-600/30"
	bgColor="bg-yellow-100 dark:bg-yellow-900/20"
	textColor="text-warning"
/>
