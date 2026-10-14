<script lang="ts" generics="T">
	import { ChevronDown, ChevronUp, Columns3Cog, GripVertical } from 'lucide-svelte';
	import { tick } from 'svelte';
	import {
		columnSections,
		isMovableColumn,
		movableIds,
		moveColumn,
		moveColumnTo,
		type EntityColumn
	} from './columns';
	import {
		common_dragToReorderColumn,
		common_columns,
		common_moveColumnDown,
		common_moveColumnUp,
		common_resetColumns,
		common_shortcutsPausedInSearch
	} from '$lib/paraglide/messages';
	import { isShiftShortcut, shiftShortcutLabel } from '$lib/shared/utils/shortcuts';
	import { isFreeKeydown, useListShortcuts } from '../controls/listShortcuts';

	let {
		columns,
		order,
		visibility,
		onToggle,
		onReorder,
		onReset
	}: {
		columns: EntityColumn<T>[];
		/** The full column order, pinned columns included. */
		order: string[];
		visibility: Record<string, boolean>;
		onToggle: (id: string) => void;
		onReorder: (order: string[]) => void;
		onReset: () => void;
	} = $props();

	let open = $state(false);
	let panelElement = $state<HTMLDivElement | undefined>();
	/** The panel reaches down to the bottom of the screen, less this gap, and scrolls past that. */
	const BOTTOM_GAP_PX = 16;
	let panelMaxHeight = $state<number | null>(null);

	function fitPanel() {
		if (!panelElement) return;
		const top = panelElement.getBoundingClientRect().top;
		panelMaxHeight = Math.max(window.innerHeight - top - BOTTOM_GAP_PX, 160);
	}

	$effect(() => {
		if (!open || !panelElement) return;
		fitPanel();
		window.addEventListener('resize', fitPanel);
		window.addEventListener('scroll', fitPanel, true);
		return () => {
			window.removeEventListener('resize', fitPanel);
			window.removeEventListener('scroll', fitPanel, true);
		};
	});
	let menuElement = $state<HTMLDivElement | undefined>();

	/**
	 * The primary column stays put: it carries the row's identity and its
	 * checkbox, so hiding it would leave rows with nothing to identify them by.
	 * Listed in the user's order, so the menu reads the way the table does.
	 */
	let toggleable = $derived.by(() => {
		const byId = new Map(columns.map((c) => [c.id, c]));
		return order
			.map((id) => byId.get(id))
			.filter((c): c is EntityColumn<T> => c !== undefined && !c.primary);
	});

	let sections = $derived(columnSections(toggleable));

	/** The columns a column moves among: those of its own section. */
	function peersOf(id: string): EntityColumn<T>[] {
		return sections.find((s) => s.columns.some((c) => c.id === id))?.columns ?? [];
	}

	let draggingId = $state<string | null>(null);
	let dropTarget = $state<{ id: string; position: 'before' | 'after' } | null>(null);

	/**
	 * Set while a move re-sorts the rows. Moving a focused row in the DOM blurs
	 * it with no `relatedTarget`, which would otherwise read as focus leaving the
	 * menu and close it under the keyboard user.
	 */
	let reordering = false;

	// Shift+C (columns) opens the panel while this list is on screen; Shift+F already clears filters.
	const COLUMNS_KEY = 'C';
	const columnsShortcut = shiftShortcutLabel(COLUMNS_KEY);
	const listShortcuts = useListShortcuts();

	async function openFromKeyboard() {
		open = true;
		await tick();
		// Focus inside the menu, so a click or Tab away closes it through `handleFocusOut`.
		panelElement?.focus();
	}

	function handleKeydown(event: KeyboardEvent) {
		if (!listShortcuts?.isActive()) return;
		if (open && event.key === 'Escape' && menuElement?.contains(event.target as Node)) {
			event.preventDefault();
			open = false;
			menuElement.querySelector<HTMLButtonElement>('button')?.focus();
			return;
		}
		if (!isShiftShortcut(event, COLUMNS_KEY) || !isFreeKeydown(event)) return;
		event.preventDefault();
		if (open) open = false;
		else openFromKeyboard();
	}

	function handleFocusOut(event: FocusEvent) {
		if (reordering) return;
		const next = event.relatedTarget as Node | null;
		if (next && menuElement?.contains(next)) return;
		open = false;
	}

	async function step(id: string, delta: -1 | 1) {
		const next = moveColumn(order, peersOf(id), id, delta);
		if (next === order) return;

		reordering = true;
		onReorder(next);
		await tick();
		reordering = false;

		// Keep focus on the control that was pressed, or on its twin once the
		// column reaches an end and that control disables.
		const direction = delta < 0 ? 'up' : 'down';
		const opposite = delta < 0 ? 'down' : 'up';
		const pressed = menuElement?.querySelector<HTMLButtonElement>(
			`[data-move="${direction}"][data-column="${id}"]`
		);
		const fallback = menuElement?.querySelector<HTMLButtonElement>(
			`[data-move="${opposite}"][data-column="${id}"]`
		);
		(pressed && !pressed.disabled ? pressed : fallback)?.focus();
	}

	function handleDragStart(event: DragEvent, id: string) {
		draggingId = id;
		if (event.dataTransfer) {
			event.dataTransfer.effectAllowed = 'move';
			event.dataTransfer.setData('text/plain', id);
		}
	}

	function handleDragOver(event: DragEvent, id: string) {
		if (!draggingId || !movableIds(order, peersOf(draggingId)).includes(id)) return;
		event.preventDefault();
		if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';

		const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const position = event.clientY < rect.top + rect.height / 2 ? 'before' : 'after';
		dropTarget = { id, position };
	}

	function handleDrop(event: DragEvent) {
		event.preventDefault();
		if (draggingId && dropTarget) {
			const next = moveColumnTo(
				order,
				peersOf(draggingId),
				draggingId,
				dropTarget.id,
				dropTarget.position
			);
			if (next !== order) onReorder(next);
		}
		clearDrag();
	}

	function clearDrag() {
		draggingId = null;
		dropTarget = null;
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<div bind:this={menuElement} class="relative" onfocusout={handleFocusOut} role="presentation">
	<button
		type="button"
		onclick={() => (open = !open)}
		class="btn-secondary summary-control"
		aria-expanded={open}
		aria-haspopup="true"
	>
		<Columns3Cog class="h-3.5 w-3.5" />
		{common_columns()}
		{#if listShortcuts}
			{@const paused = listShortcuts.isPaused()}
			<!-- Same key hint as the other controls in the summary row (`PaginationBar`). -->
			<span
				class="text-tertiary font-normal transition-opacity {paused ? 'opacity-40' : ''}"
				title={paused ? common_shortcutsPausedInSearch() : undefined}>({columnsShortcut})</span
			>
		{/if}
	</button>

	{#if open}
		<!--
			Focusable but out of the tab order, so a mousedown on something that is not
			focusable — a drag grip, row padding — moves focus here instead of nowhere.
			Nowhere reads to `handleFocusOut` as leaving the menu, and closed it before
			a drag could start.
		-->
		<div
			bind:this={panelElement}
			tabindex="-1"
			class="card absolute left-0 z-30 mt-1 w-72 overflow-y-auto !rounded-lg !p-3 shadow-lg outline-none"
			style:max-height={panelMaxHeight === null ? undefined : `${panelMaxHeight}px`}
		>
			<div class="mb-2 flex items-center justify-between">
				<span class="text-primary text-sm font-semibold">{common_columns()}</span>
				<button
					type="button"
					onclick={onReset}
					class="text-tertiary hover:text-secondary text-xs transition-colors"
				>
					{common_resetColumns()}
				</button>
			</div>
			{#each sections as { section, columns: sectionColumns } (section)}
				{@const movable = movableIds(order, sectionColumns)}
				{#if section}
					<div class="text-tertiary mb-1 mt-3 px-1 text-xs font-semibold uppercase tracking-wide">
						{section}
					</div>
				{/if}
				<ul class="space-y-0.5">
					{#each sectionColumns as column (column.id)}
						{@const canMove = isMovableColumn(column)}
						{@const index = movable.indexOf(column.id)}
						{@const isTarget = dropTarget?.id === column.id && draggingId !== column.id}
						<li
							class="flex items-center gap-1 rounded border-y-2 border-transparent px-1 py-0.5 {draggingId ===
							column.id
								? 'opacity-50'
								: ''} {isTarget && dropTarget?.position === 'before'
								? '!border-t-blue-500'
								: ''} {isTarget && dropTarget?.position === 'after' ? '!border-b-blue-500' : ''}"
							draggable={canMove}
							ondragstart={(event) => handleDragStart(event, column.id)}
							ondragover={(event) => handleDragOver(event, column.id)}
							ondrop={handleDrop}
							ondragend={clearDrag}
						>
							{#if canMove}
								<span
									class="text-tertiary cursor-grab"
									title={common_dragToReorderColumn({ column: column.label })}
									aria-hidden="true"
								>
									<GripVertical class="h-4 w-4" />
								</span>
							{:else}
								<span class="w-4" aria-hidden="true"></span>
							{/if}
							<label class="flex min-w-0 flex-1 cursor-pointer items-center gap-2">
								<input
									type="checkbox"
									checked={visibility[column.id] !== false}
									onchange={() => onToggle(column.id)}
									class="checkbox-card h-4 w-4 rounded"
								/>
								<span class="text-secondary truncate text-sm" title={column.label}
									>{column.label}</span
								>
							</label>
							{#if canMove}
								<button
									type="button"
									data-move="up"
									data-column={column.id}
									onclick={() => step(column.id, -1)}
									disabled={index <= 0}
									aria-label={common_moveColumnUp({ column: column.label })}
									class="text-tertiary hover:text-primary rounded p-0.5 transition-colors disabled:opacity-30"
								>
									<ChevronUp class="h-4 w-4" />
								</button>
								<button
									type="button"
									data-move="down"
									data-column={column.id}
									onclick={() => step(column.id, 1)}
									disabled={index === movable.length - 1}
									aria-label={common_moveColumnDown({ column: column.label })}
									class="text-tertiary hover:text-primary rounded p-0.5 transition-colors disabled:opacity-30"
								>
									<ChevronDown class="h-4 w-4" />
								</button>
							{/if}
						</li>
					{/each}
				</ul>
			{/each}
		</div>
	{/if}
</div>
