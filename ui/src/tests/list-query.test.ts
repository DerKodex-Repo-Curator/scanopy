import { describe, it, expect } from 'vitest';
import {
	CLIENT_QUERY,
	decodeListParams,
	hasListParams,
	listParams,
	narrowsList,
	NO_VALUE,
	valuesOrNoneCodec,
	type ListQueryState,
	type ServerQuery
} from '$lib/shared/components/data/controls/listQuery';
import { resolveOrdering } from '$lib/shared/components/data/controls/dataControlsStorage';
import {
	blankFilterState,
	toggleBoolean,
	toggleValue,
	type FilterState
} from '$lib/shared/components/data/controls/filtering';
import { reviveFilterState } from '$lib/shared/components/data/controls/dataControlsStorage';
import {
	getFieldKey,
	type FieldConfig,
	type TableDefaults
} from '$lib/shared/components/data/types';

type Row = Record<string, unknown>;

const site: FieldConfig<Row> = {
	key: 'site_id',
	label: 'Site',
	type: 'string',
	filterable: true,
	serverFiltered: true,
	queryParam: 'site_ids'
};
const ports: FieldConfig<Row> = {
	key: 'port_bindings',
	label: 'Ports',
	type: 'array',
	filterable: true,
	serverFiltered: true,
	queryParam: 'ports'
};
const enabled: FieldConfig<Row> = {
	key: 'is_enabled',
	label: 'Enabled',
	type: 'boolean',
	filterable: true
};
const virtualizedBy: FieldConfig<Row> = {
	key: 'virtualized_by',
	label: 'Virtualized by',
	type: 'string',
	filterable: true,
	serverFiltered: true,
	filterCodec: valuesOrNoneCodec('virtualization_service_names', 'include_unvirtualized')
};
const tags: FieldConfig<Row> = { key: 'tags', label: 'Tags', type: 'array', filterable: true };
const name: FieldConfig<Row> = { key: 'name', label: 'Name', type: 'string', sortable: true };
const FIELDS = [site, ports, enabled, virtualizedBy, tags, name];
const FIELD_KEYS = new Set(FIELDS.map(getFieldKey));

const SERVER: ServerQuery = { search: true, tags: true, stale: true, order: true, fields: true };
const DEFAULTS: TableDefaults = { sort: { field: 'name', direction: 'asc' } };

function state(overrides: Partial<ListQueryState> = {}): ListQueryState {
	return {
		sort: { field: 'name', direction: 'asc' },
		group: null,
		filterState: blankFilterState(FIELDS, false),
		staleOnly: false,
		search: '',
		...overrides
	};
}

function filters(...edits: ((s: FilterState) => FilterState | null)[]): FilterState {
	return edits.reduce((s, edit) => edit(s) ?? s, blankFilterState(FIELDS, false));
}

/** The view a link describes, as the list applies it: decoded, then resolved like saved state. */
function reopened(url: URLSearchParams): ListQueryState {
	const view = decodeListParams(url, FIELDS);
	const ordering = resolveOrdering({ sort: view.sort, group: view.group }, DEFAULTS, FIELD_KEYS);
	return {
		sort: ordering.sortState,
		group: ordering.groupField,
		filterState: { ...blankFilterState(FIELDS, false), ...reviveFilterState(view.filterState) },
		staleOnly: view.staleOnly,
		search: view.search
	};
}

/** A link reopens to the same request: the property every shared URL relies on. */
function expectRoundTrip(s: ListQueryState, server: ServerQuery = SERVER) {
	const sent = listParams(s, FIELDS, DEFAULTS, server);
	const again = listParams(reopened(sent.url), FIELDS, DEFAULTS, server);
	expect(again.url.toString()).toBe(sent.url.toString());
	expect(again.request.toString()).toBe(sent.request.toString());
}

describe('listParams round trip', () => {
	it('carries a sort and a grouping, including grouping explicitly off', () => {
		expectRoundTrip(state({ sort: { field: 'name', direction: 'desc' }, group: 'site_id' }));
		expectRoundTrip(state({ sort: { field: null, direction: 'asc' } }), CLIENT_QUERY);
	});

	it('carries a tag-group sort and grouping in the tag-group params', () => {
		const sent = listParams(
			state({ sort: { field: 'tag_group:g1', direction: 'desc' }, group: 'tag_group:g2' }),
			FIELDS,
			DEFAULTS,
			SERVER
		);
		expect(sent.request.get('order_by_tag_group')).toBe('g1');
		expect(sent.request.get('group_by_tag_group')).toBe('g2');
		const view = decodeListParams(sent.url, FIELDS);
		expect(view.sort?.field).toBe('tag_group:g1');
		expect(view.group).toBe('tag_group:g2');
	});

	it('carries string, array and boolean filters by value', () => {
		expectRoundTrip(
			state({
				filterState: filters(
					(s) => toggleValue(s, 'site_id', 'site-a'),
					(s) => toggleValue(s, 'site_id', 'site-b'),
					(s) => toggleValue(s, 'port_bindings', '443'),
					(s) => toggleBoolean(s, 'is_enabled', 'showFalse', false)?.state ?? null
				)
			})
		);
	});

	it('carries a client-side boolean with neither box checked', () => {
		const neither = filters(
			(s) => toggleBoolean(s, 'is_enabled', 'showFalse', false)?.state ?? null,
			(s) => toggleBoolean(s, 'is_enabled', 'showTrue', false)?.state ?? null
		);
		expectRoundTrip(state({ filterState: neither }), CLIENT_QUERY);
		const view = decodeListParams(
			listParams(state({ filterState: neither }), FIELDS, DEFAULTS, CLIENT_QUERY).url,
			FIELDS
		);
		expect(view.filterState.is_enabled).toMatchObject({ showTrue: false, showFalse: false });
	});

	it('splits a values-or-none filter across its two params', () => {
		const s = state({
			filterState: filters(
				(f) => toggleValue(f, 'virtualized_by', 'Proxmox'),
				(f) => toggleValue(f, 'virtualized_by', NO_VALUE)
			)
		});
		const { request } = listParams(s, FIELDS, DEFAULTS, SERVER);
		expect(request.getAll('virtualization_service_names')).toEqual(['Proxmox']);
		expect(request.get('include_unvirtualized')).toBe('true');
		expectRoundTrip(s);
	});

	it('carries tags, search and staleness', () => {
		expectRoundTrip(
			state({
				filterState: filters((s) => toggleValue(s, 'tags', 't1')),
				search: 'nas',
				staleOnly: true
			})
		);
	});

	it('reads the page and the selection', () => {
		const url = new URLSearchParams('page=3&selected=a&selected=b');
		const view = decodeListParams(url, FIELDS);
		expect(view.page).toBe(3);
		expect(view.selected).toEqual(['a', 'b']);
	});
});

describe('listParams defaults and the server split', () => {
	it('writes nothing for a client list at its defaults', () => {
		expect(listParams(state(), FIELDS, DEFAULTS, CLIENT_QUERY).url.toString()).toBe('');
	});

	it('always orders a server list, which does not know the tab default', () => {
		const { request } = listParams(state(), FIELDS, DEFAULTS, SERVER);
		expect(request.get('order_by')).toBe('name');
		expect(request.get('order_direction')).toBe('asc');
		expect(narrowsList(request)).toBe(false);
	});

	it('keeps client-side parts out of the request but in the URL', () => {
		const s = state({
			filterState: filters((f) => toggleValue(f, 'site_id', 'site-a')),
			search: 'x'
		});
		const { url, request } = listParams(s, FIELDS, DEFAULTS, {
			...CLIENT_QUERY,
			stale: true
		});
		expect(url.getAll('site_ids')).toEqual(['site-a']);
		expect(url.get('search')).toBe('x');
		expect(request.toString()).toBe('');
	});
});

describe('decodeListParams with malformed or stale params', () => {
	it('ignores an unknown direction and a bad page, and applies the rest', () => {
		const view = decodeListParams(
			new URLSearchParams(
				'order_by=name&order_direction=sideways&group_by=site_id&page=0&search=nas'
			),
			FIELDS
		);
		expect(view.sort).toBeUndefined();
		expect(view.group).toBe('site_id');
		expect(view.page).toBeUndefined();
		expect(view.search).toBe('nas');
		expect(decodeListParams(new URLSearchParams('page=abc'), FIELDS).page).toBeUndefined();
	});

	it('lands a sort or group on a removed field on the tab default', () => {
		const reopenedState = reopened(new URLSearchParams('order_by=gone&order_direction=desc'));
		expect(reopenedState.sort).toEqual(DEFAULTS.sort);
		expect(reopenedState.group).toBeNull();
	});

	it("never reads a param for a filter the list doesn't have, and drops it on the next write", () => {
		const url = new URLSearchParams('removed_field=x&site_ids=site-a');
		expect(hasListParams(url, FIELDS)).toBe(true);
		const again = listParams(reopened(url), FIELDS, DEFAULTS, CLIENT_QUERY).url;
		expect(again.has('removed_field')).toBe(false);
		expect(again.getAll('site_ids')).toEqual(['site-a']);
	});

	it('treats a URL with no list params as no link', () => {
		expect(hasListParams(new URLSearchParams('modal=host-editor&id=x'), FIELDS)).toBe(false);
	});
});
