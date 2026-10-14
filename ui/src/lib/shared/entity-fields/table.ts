/**
 * List-table columns from the entity field registry.
 *
 * The registry supplies each column's label, value, entity chips, attribution and default
 * visibility. A tab keeps only what is about the table rather than the field: which columns
 * search, filter, sort or group, widths, the primary column, and presentation the registry has no
 * format for (status chips, utilization bars). Registry fields a tab doesn't mention become
 * columns of their own, hidden by default, so a field added on the backend reaches every table.
 */

import type {
	BaseFieldConfig,
	CardFieldItem,
	DisplayFieldConfig,
	FieldConfig,
	OrderableFieldConfig,
	OrderableFieldEntry
} from '$lib/shared/components/data/types';
import { entityRef } from '$lib/shared/components/data/types';
import { entities } from '$lib/shared/stores/metadata';
import {
	entityDisplayText,
	entityFieldLabel,
	entityFields,
	fieldEntityType,
	fieldSource,
	formatFieldValue,
	resolveField,
	type EntityField,
	type EntityLookup,
	type EntityType
} from './index';

/** A tab's settings for one column: anything it leaves out comes from the registry. */
type ColumnOverride<T, Q extends string = string> = Partial<OrderableFieldEntry<T, Q>>;

/** The column type a registry field gets when the tab declares none. */
export function columnType(field: EntityField): BaseFieldConfig<unknown>['type'] {
	// The registry says when one entity can hold several values.
	if (field.many) return 'array';
	if (field.format.type === 'Boolean') return 'boolean';
	if (field.format.type === 'Timestamp') return 'date';
	return 'string';
}

/** The registry's column for one field, before a tab's settings. */
export function registryColumn<T extends object>(
	entityType: EntityType,
	field: EntityField,
	lookup: () => EntityLookup
): BaseFieldConfig<T> {
	const type = columnType(field);
	const targetType = fieldEntityType(field);
	const texts = (item: T) =>
		resolveField(field, item, lookup())
			.map((value) => formatFieldValue(field, value))
			.filter(Boolean);

	const getItems =
		field.format.type === 'Entity' && targetType
			? (item: T): CardFieldItem[] =>
					resolveField(field, item, lookup()).flatMap((related) => {
						const entity = related as { id?: string };
						if (!entity?.id) return [];
						return [
							{
								id: entity.id,
								label: entityDisplayText(targetType, entity),
								color: entities.getColorHelper(targetType).color,
								entityRef: entityRef(targetType, entity.id, entity)
							}
						];
					})
			: undefined;

	// Entity and metadata fields show a name; their filters hold the id the API filters on, and
	// render it back through the same formatting.
	const filterById =
		(field.format.type === 'Entity' && targetType) || field.format.type === 'Metadata';
	const rawValues = (item: T): string[] =>
		resolveField(field, item, lookup())
			.map((value) =>
				value && typeof value === 'object' ? (value as { id?: string }).id : String(value)
			)
			.filter((value): value is string => !!value);
	const filterLabel = (value: string): string => {
		if (field.format.type !== 'Entity' || !targetType) return formatFieldValue(field, value);
		const [entity] = lookup().byColumn(targetType, 'id', value);
		return entity ? entityDisplayText(targetType, entity) : value;
	};

	return {
		label: entityFieldLabel(entityType, field),
		type,
		...(filterById && {
			getFilterValue: (item: T) => {
				const values = rawValues(item);
				if (type === 'array') return values;
				return values[0] ?? null;
			},
			filterLabel
		}),
		getValue: (item: T) => {
			if (type === 'boolean') return Boolean(resolveField(field, item, lookup())[0]);
			if (type === 'date') return (resolveField(field, item, lookup())[0] as string) ?? null;
			const values = texts(item);
			if (type === 'array') return values;
			return values.length ? values.join(', ') : null;
		},
		display: {
			hiddenByDefault: !field.summary,
			getItems,
			wrap: field.format.type === 'Prose',
			getSource: field.source_field ? (item: T) => fieldSource(field, item) : undefined
		}
	};
}

/**
 * A listed column: the registry's, with the tab's settings on top. Whether it shows by default is
 * the tab's call, as it was before the registry; the registry's `summary` only decides for columns
 * the tab never listed.
 */
function merge<T, Q extends string>(
	base: BaseFieldConfig<T> | undefined,
	override: ColumnOverride<T, Q>
) {
	const baseDisplay = { ...base?.display };
	delete baseDisplay.hiddenByDefault;
	return {
		...base,
		...override,
		display: { ...baseDisplay, ...override.display }
	} as OrderableFieldEntry<T, Q>;
}

/**
 * `defineFields` with the registry underneath: every orderable column and display column whose
 * id is a registry field starts from the registry's column, and the tab's settings win. Registry
 * fields the tab doesn't list are appended, hidden by default.
 */
export function defineRegistryFields<T extends object, O extends string, Q extends string = string>(
	entityType: EntityType,
	lookup: () => EntityLookup,
	orderable: { [K in O]: ColumnOverride<T, Q> },
	display: (Partial<DisplayFieldConfig<T, Q>> & { key: string })[] = [],
	/** Registry fields this table leaves out, so the omission is on record. */
	omit: readonly string[] = []
): FieldConfig<T, O, Q>[] {
	const registry = new Map(
		entityFields(entityType).map((field) => [
			field.id,
			registryColumn<T>(entityType, field, lookup)
		])
	);
	const listed = new Set<string>(omit);

	const orderableColumns = (Object.entries(orderable) as [O, ColumnOverride<T, Q>][]).map(
		([orderField, override]) => {
			listed.add(orderField);
			return { ...merge(registry.get(orderField), override), orderField };
		}
	) as OrderableFieldConfig<T, O, Q>[];

	const displayColumns = display.map((column) => {
		listed.add(column.key);
		return { ...merge(registry.get(column.key), column), key: column.key };
	}) as DisplayFieldConfig<T, Q>[];

	const unlisted = [...registry]
		.filter(([id]) => !listed.has(id))
		.map(([id, column]) => ({ ...column, key: id }) as DisplayFieldConfig<T, Q>);

	const columns = [...orderableColumns, ...displayColumns, ...unlisted];
	if (import.meta.env.DEV) {
		for (const column of columns) {
			if (!column.label || !column.type) {
				console.warn(
					`[defineRegistryFields] ${entityType} column "${'orderField' in column ? column.orderField : column.key}" has no label or type: it is neither a registry field nor fully declared by the tab`
				);
			}
		}
	}
	return columns;
}
