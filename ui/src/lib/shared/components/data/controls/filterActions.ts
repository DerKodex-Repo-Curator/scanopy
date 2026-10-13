import { getFieldKey, type FieldConfig } from '../types';
import {
	blankFilter,
	blankFilterState,
	toggleBoolean,
	toggleValue,
	type FilterState
} from './filtering';

/**
 * The filter state the actions read and write.
 *
 * Accessors rather than values, so the component passes its live `$state`
 * through getters and setters and every action sees the current selection.
 * The parent hears of a change through the query the component derives from
 * this state, so no action notifies it.
 */
export interface FilterActionsContext<T> {
	readonly fields: FieldConfig<T>[];
	filterState: FilterState;
	staleOnly: boolean;
	/** The server applies `serverFiltered` fields. */
	readonly serverFields: boolean;
}

export type FilterActions = ReturnType<typeof createFilterActions>;

/** Every way a user changes a filter, shared by the table's column headers so they edit one selection. */
export function createFilterActions<T>(ctx: FilterActionsContext<T>) {
	/** Whether the server, not the client pass, acts on this field's filter. */
	function isServerFiltered(fieldKey: string): boolean {
		if (!ctx.serverFields) return false;
		return ctx.fields.find((f) => getFieldKey(f) === fieldKey)?.serverFiltered === true;
	}

	return {
		toggleString(fieldKey: string, value: string) {
			const next = toggleValue(ctx.filterState, fieldKey, value);
			if (next) ctx.filterState = next;
		},

		toggleBoolean(fieldKey: string, box: 'showTrue' | 'showFalse') {
			const next = toggleBoolean(ctx.filterState, fieldKey, box, isServerFiltered(fieldKey));
			if (next) ctx.filterState = next.state;
		},

		// Uses tag ids, which is what the server filters by.
		toggleTag(tagId: string) {
			const next = toggleValue(ctx.filterState, 'tags', tagId);
			if (next) ctx.filterState = next;
		},

		toggleStale() {
			ctx.staleOnly = !ctx.staleOnly;
		},

		/** Clear every filter. No defaults: clearing clears, including a default the product chose. */
		clearAll() {
			ctx.filterState = blankFilterState(ctx.fields, false);
			ctx.staleOnly = false;
		},

		/** Clear one column's filter, including its stale toggle when it carries one. */
		clearField(fieldKey: string) {
			const field = ctx.fields.find((f) => getFieldKey(f) === fieldKey);
			if (!field) return;

			if (field.staleFilter) ctx.staleOnly = false;
			if (!field.filterable) return;

			ctx.filterState = { ...ctx.filterState, [fieldKey]: blankFilter(field, false) };
		}
	};
}
