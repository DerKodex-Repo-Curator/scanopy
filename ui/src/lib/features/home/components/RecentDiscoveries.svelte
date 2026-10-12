<!--
	Recent scans: one line on how they went, a tag on each run, and, only when there is something to
	act on, the rows to fix with the button that fixes each. Warnings that ask nothing of the reader
	are counted on their run's tag and listed only in the run's own report.
-->
<script lang="ts">
	import type { Discovery } from '$lib/features/discovery/types/base';
	import type { Daemon } from '$lib/features/daemons/types/base';
	import type { Credential } from '$lib/features/credentials/types/base';
	import type { components } from '$lib/api/schema';
	import type { EntityDiscriminants } from '$lib/api/entities';
	import HomeDiscoveryDisplay from './HomeDiscoveryDisplay.svelte';
	import WarningSubjectChip from '$lib/features/discovery/components/DiscoveryModal/WarningSubjectChip.svelte';
	import { warningAction } from '$lib/features/discovery/utils/warningActions';
	import { createColorHelper, createIconComponent } from '$lib/shared/utils/styling';
	import { recentScansView, type RecentDiscovery } from '../utils/recentScans';
	import {
		home_noScansYet,
		home_recentScans,
		home_recentScansFinished,
		home_recentScansMoreToFix,
		home_recentScansStopped,
		home_recentScansToFix,
		home_recentScansToFixHeading
	} from '$lib/paraglide/messages';

	type SiteSummary = components['schemas']['SiteSummary'];

	let {
		recent,
		daemons = [],
		sites = [],
		credentials = [],
		onOpenRun
	}: {
		recent: RecentDiscovery[];
		daemons?: Daemon[];
		sites?: SiteSummary[];
		credentials?: Credential[];
		onOpenRun?: (discovery: Discovery) => void;
	} = $props();

	// Credentials are the only entities named here that the page already holds. Anything else
	// resolves to nothing and shows as its address, which the run's report then names in full.
	let nameOfEntity = $derived((type: EntityDiscriminants, id: string) =>
		type === 'Credential' ? credentials.find((c) => c.id === id)?.name : undefined
	);

	let view = $derived(recentScansView(recent, nameOfEntity));

	let summary = $derived(
		[
			home_recentScansFinished({ count: view.finished, total: view.runs.length }),
			view.toFix > 0 ? home_recentScansToFix({ count: view.toFix }) : null,
			view.stopped > 0 ? home_recentScansStopped({ count: view.stopped }) : null
		]
			.filter(Boolean)
			.join(' · ')
	);
</script>

<section>
	<h3 class="text-primary mb-1 text-base font-semibold">{home_recentScans()}</h3>
	{#if view.runs.length === 0}
		<p class="text-tertiary text-sm">{home_noScansYet()}</p>
	{:else}
		<p class="text-secondary mb-3 text-sm">{summary}</p>
		<div class="grid grid-cols-[repeat(auto-fill,minmax(360px,1fr))] gap-4">
			{#each view.runs as run (run.discovery.id)}
				<button
					class="card card-static cursor-pointer text-left"
					onclick={() => onOpenRun?.(run.discovery)}
				>
					<HomeDiscoveryDisplay item={run} context={{ daemons, sites }} />
				</button>
			{/each}
		</div>

		{#if view.sections.length > 0}
			<div class="card card-static mt-4 space-y-3">
				<h4 class="text-primary text-sm font-semibold">{home_recentScansToFixHeading()}</h4>
				{#each view.sections as section (section.remedy)}
					{@const SectionIcon = createIconComponent(section.icon)}
					<div class="space-y-1">
						<p class="text-tertiary flex items-center gap-1.5 text-xs">
							<SectionIcon class="h-3.5 w-3.5" />
							<span>{section.title}</span>
						</p>
						<ul class="space-y-1">
							{#each section.items as item (item.key)}
								{@const EntryIcon = createIconComponent(item.entry.icon)}
								{@const colors = createColorHelper(item.entry.color)}
								{@const action = warningAction(item.entry, item.payload, credentials)}
								<li class="flex items-start gap-2">
									<button
										type="button"
										class="hover:bg-tertiary/40 -mx-1 flex min-w-0 flex-1 cursor-pointer items-start gap-2 rounded px-1 py-1 text-left"
										onclick={() => onOpenRun?.(item.discovery)}
									>
										<EntryIcon class="mt-0.5 h-4 w-4 shrink-0 {colors.icon}" />
										<span class="text-primary shrink-0 text-sm">{item.entry.title}</span>
										{#if item.entry.subjects.length > 0}
											<span class="flex flex-wrap items-center gap-1">
												{#each item.entry.subjects as subject (subject.label)}
													<WarningSubjectChip {subject} />
												{/each}
											</span>
										{/if}
									</button>
									{#if action}
										{@const ActionIcon = action.icon}
										<button
											type="button"
											class="btn-secondary flex shrink-0 items-center gap-1 py-1 text-xs"
											onclick={action.run}
										>
											<ActionIcon class="h-3.5 w-3.5" />
											<span>{action.label}</span>
										</button>
									{/if}
								</li>
							{/each}
						</ul>
					</div>
				{/each}
				{#if view.hidden}
					{@const hidden = view.hidden}
					<button
						type="button"
						class="text-link cursor-pointer text-sm hover:underline"
						onclick={() => onOpenRun?.(hidden.discovery)}
					>
						{home_recentScansMoreToFix({ count: hidden.count })}
					</button>
				{/if}
			</div>
		{/if}
	{/if}
</section>
