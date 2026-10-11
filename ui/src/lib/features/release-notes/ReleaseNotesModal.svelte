<script lang="ts">
	import DOMPurify from 'dompurify';
	import GenericModal from '$lib/shared/components/layout/GenericModal.svelte';
	import { formatLongDate } from '$lib/shared/utils/formatting';
	import type { UnseenReleaseNotes } from './queries';
	import {
		common_close,
		releaseNotes_earlierReleases,
		releaseNotes_title
	} from '$lib/paraglide/messages';

	let {
		isOpen,
		notes,
		onClose
	}: {
		isOpen: boolean;
		notes: UnseenReleaseNotes | undefined;
		onClose: () => void;
	} = $props();

	// The server renders the notes without raw HTML; sanitizing here keeps the
	// modal safe whatever it is handed. Links in release content leave the app.
	function sanitize(html: string): string {
		const fragment = DOMPurify.sanitize(html, { RETURN_DOM_FRAGMENT: true });
		fragment.querySelectorAll('a').forEach((a) => {
			a.setAttribute('target', '_blank');
			a.setAttribute('rel', 'noopener noreferrer');
		});
		const container = document.createElement('div');
		container.appendChild(fragment);
		return container.innerHTML;
	}
</script>

<GenericModal {isOpen} title={releaseNotes_title()} size="lg" {onClose}>
	<div class="flex min-h-0 flex-1 flex-col">
		<div class="flex-1 space-y-8 overflow-auto p-6">
			{#each notes?.releases ?? [] as release (release.version)}
				<section>
					<div class="mb-3 flex flex-wrap items-baseline gap-x-3 gap-y-1">
						<h2 class="text-primary text-base font-semibold">{release.title}</h2>
						<span class="text-tertiary text-xs">
							v{release.version} · {formatLongDate(release.date, 'UTC')}
						</span>
					</div>
					<div class="release-notes-body text-secondary text-sm">
						<!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized above -->
						{@html sanitize(release.html)}
					</div>
				</section>
			{/each}
			{#if notes?.changelog_url}
				<!-- eslint-disable svelte/no-navigation-without-resolve -->
				<a
					class="text-link text-sm"
					href={notes.changelog_url}
					target="_blank"
					rel="external noopener noreferrer"
				>
					{releaseNotes_earlierReleases()}
				</a>
				<!-- eslint-enable svelte/no-navigation-without-resolve -->
			{/if}
		</div>

		<div class="modal-footer">
			<div class="flex items-center justify-end">
				<button type="button" class="btn-primary" onclick={() => onClose()}>
					{common_close()}
				</button>
			</div>
		</div>
	</div>
</GenericModal>

<style>
	.release-notes-body :global(h2),
	.release-notes-body :global(h3) {
		color: var(--color-text-primary);
		font-weight: 600;
		margin: 1rem 0 0.5rem;
	}
	.release-notes-body :global(p) {
		margin: 0 0 0.75rem;
	}
	.release-notes-body :global(ul),
	.release-notes-body :global(ol) {
		margin: 0 0 0.75rem;
		padding-left: 1.25rem;
	}
	.release-notes-body :global(ul) {
		list-style: disc;
	}
	.release-notes-body :global(ol) {
		list-style: decimal;
	}
	.release-notes-body :global(li) {
		margin-bottom: 0.375rem;
	}
	.release-notes-body :global(a) {
		text-decoration: underline;
	}
	.release-notes-body :global(code) {
		font-family: ui-monospace, monospace;
		font-size: 0.85em;
	}
</style>
