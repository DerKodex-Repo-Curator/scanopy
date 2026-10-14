import { getFieldKey, type FieldConfig } from '../types';

/** What a field resolves to for one item. */
export type FieldValue = string | boolean | Date | string[] | null;

/**
 * Resolve a field's value for an item.
 *
 * Falls back to a property lookup by field key so a field that only needs to
 * read `item.name` doesn't have to declare a `getValue`.
 */
export function getFieldValue<T>(item: T, field: FieldConfig<T>): FieldValue {
	if (field.getValue) {
		return field.getValue(item);
	}
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	return (item as any)[getFieldKey(field)] ?? null;
}

/**
 * The value a filter matches an item on: `getFilterValue`, else the displayed value. A date has no
 * filter value.
 */
export function getFilterValue<T>(
	item: T,
	field: FieldConfig<T>
): string | boolean | string[] | null {
	if (field.getFilterValue) return field.getFilterValue(item);
	const value = getFieldValue(item, field);
	return value instanceof Date ? null : value;
}

/** The text a filter option shows for `value`. */
export function filterLabelOf<T>(field: FieldConfig<T>, value: string): string {
	return field.filterLabel?.(value) ?? value;
}

/**
 * The values a field's filter offers, ordered by the label each shows: `filterOptions` when the
 * field supplies them, else the distinct filter values across `items`.
 *
 * Array-typed fields are flattened, so a "tags" field offers each individual tag as an option
 * rather than one option per combination. Reading the loaded items is only meaningful for lists
 * holding every row: on a server-paginated list it would only ever see the loaded page, which is
 * why `FieldConfig.filterOptions` exists.
 */
export function filterOptionsOf<T>(items: T[], field: FieldConfig<T>): string[] {
	const values = new Set<string>(field.filterOptions);

	if (!field.filterOptions) {
		items.forEach((item) => {
			const value = getFilterValue(item, field);
			if (value === null || value === undefined) return;

			if (Array.isArray(value)) {
				value.forEach((v) => {
					if (v !== null && v !== undefined && v !== '') values.add(String(v));
				});
			} else if (value !== '') {
				values.add(String(value));
			}
		});
	}

	return Array.from(values).sort((a, b) =>
		filterLabelOf(field, a).localeCompare(filterLabelOf(field, b), undefined, { numeric: true })
	);
}
