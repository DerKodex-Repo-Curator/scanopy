<script lang="ts">
	import { X, ArrowRight } from 'lucide-svelte';
	import { onMount } from 'svelte';
	import type { IconComponent } from '$lib/shared/utils/types';
	import { trackEvent } from '$lib/shared/utils/analytics';
	import { common_dismiss } from '$lib/paraglide/messages';

	let {
		id,
		title,
		description,
		actionLabel,
		onAction,
		onDismiss,
		iconColor,
		Icon,
		dismissable = true,
		dismissOnAction = true
	}: {
		id: string;
		title: string;
		description: string;
		actionLabel: string;
		onAction: () => void;
		onDismiss?: () => void;
		dismissable?: boolean;
		/** Whether acting on the nudge also dismisses it. Off for a nudge that clears on its own
		 *  once the action is done, so backing out of the action leaves it in place. */
		dismissOnAction?: boolean;
		Icon: IconComponent;
		iconColor: string;
	} = $props();

	let dismissed = $state(false);
	let dismissKey = $derived(`nudge-dismissed:${id}`);

	onMount(() => {
		dismissed = localStorage.getItem(dismissKey) === 'true';
	});

	function dismiss() {
		trackEvent('nudge_dismissed', { nudge_id: id });
		localStorage.setItem(dismissKey, 'true');
		dismissed = true;
		onDismiss?.();
	}

	function handleAction() {
		trackEvent('nudge_action_clicked', { nudge_id: id });
		if (dismissOnAction) dismiss();
		onAction();
	}
</script>

{#if !dismissed}
	<div class="card card-static">
		<div class="flex items-start justify-between gap-3">
			{#if Icon}
				<div class={`mt-0.5 shrink-0 ${iconColor}`}>
					<Icon size={20} />
				</div>
			{/if}
			<div class="flex-1">
				<h4 class="text-primary mb-1 text-sm font-semibold">{title}</h4>
				<!-- eslint-disable-next-line svelte/no-at-html-tags -- description is developer-provided, not user input -->
				<p class="text-tertiary text-sm">{@html description}</p>
				<button
					onclick={handleAction}
					class="mt-2 inline-flex items-center gap-1 text-sm font-medium text-blue-400 transition-colors hover:text-blue-300"
				>
					{actionLabel}
					<ArrowRight class="h-3.5 w-3.5" />
				</button>
			</div>
			{#if dismissable}
				<button
					onclick={dismiss}
					class="text-tertiary shrink-0 rounded p-0.5 transition-colors hover:bg-white/10"
					aria-label={common_dismiss()}
				>
					<X class="h-4 w-4" />
				</button>
			{/if}
		</div>
	</div>
{/if}
