<!--
	Draws a completion inside a text input, right after the typed text. Place it as a sibling of the
	input in a `relative` wrapper. `class` must give it the input's horizontal padding and font size,
	so the invisible copy of the text ends where the typed text does.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		value,
		class: className = '',
		children
	}: {
		/** The input's current text. */
		value: string;
		class?: string;
		children: Snippet;
	} = $props();
</script>

<div
	class="pointer-events-none absolute inset-0 flex items-center overflow-hidden {className}"
	aria-hidden="true"
>
	<span class="invisible whitespace-pre">{value}</span>
	{@render children()}
</div>
