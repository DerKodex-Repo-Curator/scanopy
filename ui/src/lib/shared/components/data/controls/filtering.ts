import { SvelteSet } from 'svelte/reactivity';
import { getFieldKey, type FieldConfig } from '../types';
import { getFieldValue, getFilterValue } from './fieldValues';

/** One field's filter selection. Readonly set so both `Set` and `SvelteSet` fit. */
export interface FieldFilter {
	type: 'string' | 'boolean' | 'array';
	values: ReadonlySet<string>;
	showTrue?: boolean;
	showFalse?: boolean;
}

export type FilterState = Record<string, FieldFilter>;

/**
 * Which filters the parent applies server-side, and so must not be re-applied
 * against the loaded rows.
 *
 * Filtering a server-paginated list on the client would only ever narrow the
 * page in hand, silently hiding every match on other pages — so these flags
 * gate the client pass rather than the server one.
 */
export interface ServerFilterMode {
	/** The server applies the tag filter from the list's query. */
	tags: boolean;
	/** The server applies `serverFiltered` fields from the list's query. */
	fields: boolean;
}

/** Whether an item survives the search box. Opt-in per field via `searchable`. */
export function matchesSearch<T>(item: T, fields: FieldConfig<T>[], query: string): boolean {
	const q = query.trim().toLowerCase();
	if (!q) return true;

	// An unmarked field is not searched: matching everything by default meant a
	// date field turned "2026" into a match on every row.
	return fields
		.filter((f) => f.searchable === true)
		.some((field) => {
			const value = getFieldValue(item, field);
			if (value === null || value === undefined) return false;

			if (field.type === 'array' && Array.isArray(value)) {
				return value.some((v) => String(v).toLowerCase().includes(q));
			}

			return String(value).toLowerCase().includes(q);
		});
}

/**
 * Whether the parent, rather than the client pass, applies this field's filter.
 *
 * The single definition of the handover, read both by `matchesFilters` — which
 * skips what the parent owns — and by `serverFilterViolations`, which reports
 * what nobody owns. Keeping them on one predicate is what stops the guard and
 * the filter drifting apart.
 */
export function isHandledServerSide<T>(field: FieldConfig<T>, server: ServerFilterMode): boolean {
	if (getFieldKey(field) === 'tags' && server.tags) return true;
	return field.serverFiltered === true && server.fields;
}

/**
 * Filterable fields a server-paginated list would wrongly filter client-side.
 *
 * Under server pagination the client holds one page while the count describes
 * the whole match set, so a filter applied here narrows the page and leaves the
 * count describing different rows — the list says "62 of 1550" and pages
 * through the wrong hosts. Every filter therefore has to be handled by the
 * server that produced the count, or not offered at all.
 *
 * Returns the offending field keys so a caller can name them; empty when the
 * list is not server-paginated, where client-side filtering is correct.
 */
export function serverFilterViolations<T>(
	fields: FieldConfig<T>[],
	serverPaginated: boolean,
	server: ServerFilterMode
): string[] {
	if (!serverPaginated) return [];

	return fields
		.filter((field) => field.filterable === true && !isHandledServerSide(field, server))
		.map(getFieldKey);
}

/**
 * A filter with nothing selected, shaped for the field's type.
 *
 * `applyDefaults` seeds a string field from `filterDefaults`, which is what a
 * fresh mount wants and what "clear filters" deliberately does not: clearing
 * means clearing, including a default the product chose.
 */
export function blankFilter<T>(field: FieldConfig<T>, applyDefaults: boolean): FieldFilter {
	if (field.type === 'boolean') {
		return { type: 'boolean', values: new SvelteSet(), showTrue: true, showFalse: true };
	}
	if (field.type === 'array') {
		return { type: 'array', values: new SvelteSet() };
	}
	return {
		type: 'string',
		values: new SvelteSet(applyDefaults ? field.filterDefaults : undefined)
	};
}

/** A blank filter for every filterable field. */
export function blankFilterState<T>(fields: FieldConfig<T>[], applyDefaults: boolean): FilterState {
	const state: FilterState = {};

	for (const field of fields) {
		if (!field.filterable) continue;
		state[getFieldKey(field)] = blankFilter(field, applyDefaults);
	}

	return state;
}

/** The checked booleans as strings, as a boolean filter's query param carries them. */
export function booleanFilterValues(showTrue: boolean, showFalse: boolean): string[] {
	const values: string[] = [];
	if (showTrue) values.push('true');
	if (showFalse) values.push('false');
	return values;
}

/**
 * Toggle one value of a string or array filter, returning the new state.
 *
 * A pure transition returning fresh objects: the component reassigns rather
 * than mutating, so `$derived` work downstream re-runs. Returns null when the
 * key names no filter of a togglable type, which the caller reads as "nothing
 * to do".
 */
export function toggleValue(
	state: FilterState,
	fieldKey: string,
	value: string
): FilterState | null {
	const filter = state[fieldKey];
	if (!filter || (filter.type !== 'string' && filter.type !== 'array')) return null;

	const values = new SvelteSet(filter.values);
	if (values.has(value)) {
		values.delete(value);
	} else {
		values.add(value);
	}

	return { ...state, [fieldKey]: { ...filter, values } };
}

/**
 * Toggle one box of a boolean filter, returning the new state and the boxes
 * left checked.
 *
 * `normalizeEmpty` re-checks both boxes when the last one is unticked. That
 * state asks for an empty set, which a query string cannot express and nobody
 * wants — it renders an empty table under a non-zero count — so a filter the
 * server applies falls back to no constraint instead. A client-side filter
 * keeps the literal selection.
 */
export function toggleBoolean(
	state: FilterState,
	fieldKey: string,
	box: 'showTrue' | 'showFalse',
	normalizeEmpty: boolean
): { state: FilterState; showTrue: boolean; showFalse: boolean } | null {
	const filter = state[fieldKey];
	if (!filter || filter.type !== 'boolean') return null;

	let showTrue = box === 'showTrue' ? !filter.showTrue : (filter.showTrue ?? false);
	let showFalse = box === 'showFalse' ? !filter.showFalse : (filter.showFalse ?? false);

	if (normalizeEmpty && !showTrue && !showFalse) {
		showTrue = true;
		showFalse = true;
	}

	return {
		state: { ...state, [fieldKey]: { ...filter, showTrue, showFalse } },
		showTrue,
		showFalse
	};
}

/**
 * Whether an item survives every active field filter.
 *
 * The `tags` filter holds tag ids, since the panel offers every tag and the
 * server filters by id. It matches against `getItemTags`, the item's tag ids,
 * rather than the field's `getValue`, which returns tag names so the search box
 * can match them.
 */
export function matchesFilters<T>(
	item: T,
	fields: FieldConfig<T>[],
	filterState: FilterState,
	server: ServerFilterMode,
	getItemTags: ((item: T) => string[]) | null = null
): boolean {
	return fields.every((field) => {
		if (!field.filterable) return true;

		const fieldKey = getFieldKey(field);
		const filterConfig = filterState[fieldKey];
		if (!filterConfig) return true;

		if (isHandledServerSide(field, server)) return true;

		const value =
			fieldKey === 'tags' && getItemTags ? getItemTags(item) : getFilterValue(item, field);

		if (field.type === 'boolean') {
			// A null boolean is "unknown" rather than false, so neither box excludes it.
			if (value === null || value === undefined) return true;
			const boolValue = Boolean(value);
			if (boolValue && !filterConfig.showTrue) return false;
			if (!boolValue && !filterConfig.showFalse) return false;
			return true;
		}

		if (field.type === 'array') {
			// An empty selection matches everything — otherwise opening the filter
			// panel would blank the list before the user has chosen anything.
			if (filterConfig.values.size === 0) return true;
			if (!Array.isArray(value) || value.length === 0) return false;
			return value.some((v) => filterConfig.values.has(String(v)));
		}

		if (field.type === 'string') {
			if (filterConfig.values.size === 0) return true;
			if (field.filterMode === 'exclude') {
				// Exclude hides the checked values. A null has no value to exclude,
				// so it survives — where include-mode drops it for not matching.
				return value == null || !filterConfig.values.has(String(value));
			}
			if (value === null || value === undefined) return false;
			return filterConfig.values.has(String(value));
		}

		return true;
	});
}

/** Whether one field's filter would currently narrow the list. */
export function isFieldFilterActive<T>(
	field: FieldConfig<T>,
	filter: FieldFilter | undefined
): boolean {
	if (!field.filterable || !filter) return false;
	if (field.type === 'boolean') return !filter.showTrue || !filter.showFalse;
	return filter.values.size > 0;
}

/** How many filters currently narrow the list: one per filtered column, plus Stale only. */
export function activeFilterCount<T>(
	fields: FieldConfig<T>[],
	filterState: FilterState,
	staleOnly: boolean
): number {
	const columns = fields.filter((field) =>
		isFieldFilterActive(field, filterState[getFieldKey(field)])
	).length;
	return columns + (staleOnly ? 1 : 0);
}

/** Whether any filter would currently narrow the list. */
export function hasActiveFilters<T>(
	fields: FieldConfig<T>[],
	filterState: FilterState,
	staleOnly: boolean
): boolean {
	return activeFilterCount(fields, filterState, staleOnly) > 0;
}
