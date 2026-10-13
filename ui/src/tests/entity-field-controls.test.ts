import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import {
	isGroupableField,
	isSortableField,
	serverOrderViolations
} from '$lib/shared/components/data/controls/sorting';
import { serverFilterViolations } from '$lib/shared/components/data/controls/filtering';
import { columnControls } from '$lib/shared/components/data/controls/headerControls';
import { getFieldKey, type FieldConfig } from '$lib/shared/components/data/types';
import {
	FEATURES,
	OPENAPI,
	columnKeys,
	dataControlsProps,
	dataControlsServerQuery,
	dataControlsTabs,
	tabColumns,
	type SchemaNode
} from './entity-tabs';

/**
 * Every column on every entity tab has a recorded sort, group and filter decision, and the tab
 * offers exactly what was decided.
 *
 * A new column fails here until someone decides whether it sorts, groups and filters, and writes
 * down why not when it doesn't. The tab's columns, as its `<Tab>.columns.ts` builds them, run
 * through the same `isSortableField` / `isGroupableField` rules the controls use, so a flag added
 * or dropped without updating the decision fails too.
 *
 * On a server-paginated tab (one that passes `serverPagination` to `DataControls`), only order
 * fields sort and group, and every filter is applied by the server through a query parameter of
 * the tab's list operation, named here as `{ param }` and checked against `static/openapi.json`.
 */

/** `true` means the column offers it; a string means it doesn't, and says why. */
type Choice = true | string;
/** A server-side filter names the list operation's query parameter that applies it. */
type FilterChoice = Choice | { param: string };

interface ColumnDecision {
	sort: Choice;
	group: Choice;
	filter: FilterChoice;
}

interface TabDecisions {
	/** Server-paginated tabs only: the path of the `GET` list operation that applies filters. */
	listPath?: string;
	/** Column key (as `columnKeys` derives it) → decision. */
	columns: Record<string, ColumnDecision>;
}

const CAPABILITIES = ['sort', 'group', 'filter'] as const;
type Capability = (typeof CAPABILITIES)[number];

const SEARCH = 'Search finds it';
const UNIQUE = 'Unique per row';
const NO_DATE_RANGE = 'No date-range control';
const ARRAY_SORT = 'An array has no single order';
const ARRAY_GROUP = 'An array has no single group';
const COUNT = 'A count; every value would be its own group';
const FREE_TEXT = 'Free text; search covers it';
const SCAN_MODE =
	"Read from the run's stored scan settings, which the server can't sort or filter on";

/** Sorted, grouped and filtered on the client. */
const YES: ColumnDecision = { sort: true, group: true, filter: true };
/** The row's name: sorts, but every row would be its own group. */
const IDENTITY: ColumnDecision = { sort: true, group: UNIQUE, filter: SEARCH };
const DATE: ColumnDecision = { sort: true, group: 'Near-unique timestamp', filter: NO_DATE_RANGE };
const LAST_SEEN: ColumnDecision = {
	...DATE,
	filter: 'No date-range control; Stale covers last seen'
};
const BOOLEAN: ColumnDecision = {
	sort: 'Two values; group and filter cover it',
	group: true,
	filter: true
};
const TEXT: ColumnDecision = { sort: 'Free text', group: 'Free text', filter: FREE_TEXT };
/** An array whose members are shared across rows, so filtering by one narrows the list. */
const SHARED_ARRAY: ColumnDecision = { sort: ARRAY_SORT, group: ARRAY_GROUP, filter: true };
/** An array whose members each belong to one row. */
const ownedArray = (filter: string): ColumnDecision => ({
	sort: ARRAY_SORT,
	group: ARRAY_GROUP,
	filter
});
/** Not sorted, grouped or filtered, all for one reason. */
const none = (reason: string): ColumnDecision => ({ sort: reason, group: reason, filter: reason });
/** Sorted and grouped by the server's order field, filtered by the server through `param`. */
const server = (param: string): ColumnDecision => ({ sort: true, group: true, filter: { param } });
/** Sorted by the server's order field only. */
const serverIdentity = (group: string): ColumnDecision => ({ sort: true, group, filter: SEARCH });

/** The run that first or last found an entity: one chip, and Created and Last seen already say when. */
const FOUND_BY = none('A scan run; Created and Last seen order by when');

/** Keyed by tab path relative to `src/lib/features`. */
const DECISIONS: Record<string, TabDecisions> = {
	'hosts/components/HostTab.svelte': {
		listPath: '/api/v1/hosts',
		columns: {
			name: serverIdentity('Unique per host'),
			hostname: serverIdentity('Near-unique per host'),
			virtualized_by: server('virtualization_service_ids'),
			interface_ip: serverIdentity('Unique per host'),
			mac_address: serverIdentity('Unique per host'),
			site_id: server('site_ids'),
			created_at: DATE,
			updated_at: DATE,
			last_seen_at: LAST_SEEN,
			source: server('sources'),
			manufacturer: server('manufacturers'),
			model: server('models'),
			sys_location: server('sys_locations'),
			os_family: server('os_families'),
			description: TEXT,
			serial_number: none('Identifier, unique per device; search finds one'),
			asset_tag: none('Identifier, unique per device; search finds one'),
			chassis_id: none('Identifier, unique per device; search finds one'),
			sys_name: none('Identifier, unique per device; search finds one'),
			management_url: none('Identifier, unique per device; search finds one'),
			firmware_revision: {
				sort: 'Version strings do not compare across vendors',
				group: 'High cardinality',
				filter: 'High cardinality'
			},
			software_revision: {
				sort: 'Version strings do not compare across vendors',
				group: 'High cardinality',
				filter: 'High cardinality'
			},
			sys_object_id: {
				sort: 'Opaque OID',
				group: 'Model carries the readable form',
				filter: 'Model carries the readable form'
			},
			sys_contact: TEXT,
			sys_descr: TEXT,
			tags: { ...SHARED_ARRAY, filter: { param: 'tag_ids' } },
			credentials: { ...SHARED_ARRAY, filter: { param: 'credential_ids' } },
			interfaces: ownedArray('Interface names are unique per host; search finds them'),
			ports: ownedArray('No port filter on the hosts list; Services filters by port'),
			services: { ...SHARED_ARRAY, filter: { param: 'service_names' } },
			presented_by: none('Set on site identities only; Virtualized by groups and filters them'),
			first_found_by: FOUND_BY,
			last_found_by: FOUND_BY
		}
	},
	'services/components/ServiceTab.svelte': {
		listPath: '/api/v1/services',
		columns: {
			name: serverIdentity('Type groups the same values'),
			host: server('host_ids'),
			site_id: server('site_ids'),
			service_definition: server('service_definitions'),
			position: { sort: true, group: 'Per-host ordinal', filter: 'Per-host ordinal' },
			created_at: DATE,
			updated_at: DATE,
			last_seen_at: LAST_SEEN,
			containerized_by: server('virtualization_service_ids'),
			match_confidence: server('match_confidences'),
			source: server('sources'),
			port_bindings: { sort: ARRAY_SORT, group: ARRAY_GROUP, filter: { param: 'ports' } },
			ip_bindings: ownedArray('Near-unique; search finds it'),
			category: {
				sort: 'Derived in code; no SQL column',
				group: 'Derived in code; no SQL column',
				filter: { param: 'exclude_categories' }
			},
			tags: { ...SHARED_ARRAY, filter: { param: 'tag_ids' } },
			first_found_by: FOUND_BY,
			last_found_by: FOUND_BY
		}
	},
	'discovery/components/tabs/DiscoveryHistoryTab.svelte': {
		listPath: '/api/v1/discovery',
		columns: {
			// Runs of one scan share its name, so grouping gathers a scan's history.
			name: { sort: true, group: true, filter: SEARCH },
			daemon_id: server('daemon_ids'),
			site_id: server('site_ids'),
			discovery_type: server('discovery_types'),
			created_at: DATE,
			updated_at: DATE,
			phase: server('phases'),
			started_at: DATE,
			finished_at: DATE,
			duration: { sort: true, group: 'Continuous value', filter: 'Continuous value' },
			warnings: {
				sort: true,
				group: COUNT,
				filter: 'Sorting brings runs with warnings to the top'
			},
			// Light or full comes from the run's own stored scan settings, not a server column.
			scan_mode: { sort: SCAN_MODE, group: SCAN_MODE, filter: SCAN_MODE },
			tags: ownedArray('No server tag filter; runs are untagged unless tagged through the API')
		}
	},
	'discovery/components/tabs/DiscoveryScheduledTab.svelte': {
		columns: {
			name: IDENTITY,
			created_at: DATE,
			updated_at: DATE,
			daemon_id: YES,
			site_id: YES,
			discovery_type: YES,
			scan_count: { sort: true, group: COUNT, filter: COUNT },
			force_full_scan: BOOLEAN,
			legacy: YES,
			run_type: YES,
			schedule: none('Cron text'),
			last_run: DATE,
			progress: none('Live state of a running scan'),
			tags: SHARED_ARRAY
		}
	},
	'daemons/components/DaemonTab.svelte': {
		columns: {
			name: IDENTITY,
			site_id: YES,
			last_seen: DATE,
			created_at: DATE,
			updated_at: DATE,
			host_id: { sort: 'Near-unique', group: 'One daemon per host', filter: SEARCH },
			status: YES,
			os: YES,
			mode: YES,
			version: YES,
			maintainer: YES,
			url: none('Unique per daemon'),
			interfaced_subnet_ids: SHARED_ARRAY,
			tags: SHARED_ARRAY
		}
	},
	'daemon_api_keys/components/ApiKeyTab.svelte': {
		columns: {
			name: IDENTITY,
			site_id: YES,
			is_enabled: BOOLEAN,
			last_used: DATE,
			expires_at: DATE,
			created_at: DATE,
			updated_at: DATE,
			tags: SHARED_ARRAY
		}
	},
	'tags/components/TagTab.svelte': {
		columns: {
			name: IDENTITY,
			color: YES,
			// One group per tag: Application or a named group.
			tag_group: YES,
			created_at: DATE,
			updated_at: DATE,
			description: TEXT,
			icon: { sort: 'An icon name has no useful order', group: true, filter: true }
		}
	},
	'tags/components/TagGroupTab.svelte': {
		columns: {
			name: IDENTITY,
			description: TEXT,
			group_type: YES,
			entity_types: SHARED_ARRAY,
			ordered: YES,
			// Each tag is in one group, so filtering by one finds its group.
			tags: SHARED_ARRAY,
			created_at: DATE,
			updated_at: DATE
		}
	},
	'sites/components/SitesTab.svelte': {
		columns: {
			name: IDENTITY,
			vlans: ownedArray('Each belongs to one site'),
			daemons: ownedArray('Each belongs to one site'),
			subnets: ownedArray('Each belongs to one site'),
			// Credentials are shared across sites.
			credentials: SHARED_ARRAY,
			tags: SHARED_ARRAY,
			created_at: DATE,
			updated_at: DATE,
			effective_stale_after_hours: YES
		}
	},
	'users/components/UserTab.svelte': {
		columns: {
			email: IDENTITY,
			invite_status: YES,
			permissions: YES,
			site_ids: SHARED_ARRAY,
			oidc_provider: YES,
			invited_by: YES,
			invite_url: none('Unique per invite'),
			created_at: DATE,
			expires_at: DATE,
			updated_at: DATE,
			oidc_linked_at: DATE,
			terms_accepted_at: DATE,
			email_verified: BOOLEAN
		}
	},
	'user_api_keys/components/UserApiKeyTab.svelte': {
		columns: {
			name: IDENTITY,
			permissions: YES,
			site_ids: SHARED_ARRAY,
			is_enabled: BOOLEAN,
			last_used: DATE,
			expires_at: DATE,
			created_at: DATE,
			updated_at: DATE,
			tags: SHARED_ARRAY
		}
	},
	'subnets/components/SubnetTab.svelte': {
		columns: {
			name: IDENTITY,
			// Groups each range with the ranges nested inside it, not one group per CIDR.
			cidr: { sort: true, group: true, filter: SEARCH },
			subnet_type: YES,
			site_id: YES,
			created_at: DATE,
			updated_at: DATE,
			last_seen_at: DATE,
			description: TEXT,
			source: YES,
			managed_by: YES,
			first_found_by: FOUND_BY,
			last_found_by: FOUND_BY,
			tags: SHARED_ARRAY,
			utilization: {
				sort: true,
				group: 'A percentage; every value would be its own group',
				filter: 'A percentage; sort finds the fullest'
			}
		}
	},
	'credentials/components/CredentialsTab.svelte': {
		columns: {
			name: IDENTITY,
			created_at: DATE,
			updated_at: DATE,
			daemon_os: YES,
			credential_type: YES,
			integration: YES,
			beta: BOOLEAN,
			unofficial_api: BOOLEAN,
			description: TEXT,
			assigned_sites: SHARED_ARRAY,
			assigned_hosts: ownedArray(SEARCH),
			target: SHARED_ARRAY,
			tags: SHARED_ARRAY
		}
	},
	'vlans/components/VlanTab.svelte': {
		columns: {
			vlan_number: { sort: true, group: 'Unique per site', filter: SEARCH },
			name: IDENTITY,
			created_at: DATE,
			updated_at: DATE,
			last_seen_at: DATE,
			description: TEXT,
			source: YES,
			site_id: YES,
			subnet_ids: SHARED_ARRAY,
			interface_ids: SHARED_ARRAY,
			tags: SHARED_ARRAY,
			first_found_by: FOUND_BY,
			last_found_by: FOUND_BY
		}
	}
};

// ============================================================================
// Reading the tab's columns
// ============================================================================

/** Every column a tab renders, by key, plus a problem for each key it declares twice. */
function tabFields(tab: string): { fields: Map<string, FieldConfig<unknown>>; problems: string[] } {
	const fields = new Map<string, FieldConfig<unknown>>();
	const problems: string[] = [];
	for (const config of tabColumns(tab)) {
		const key = getFieldKey(config);
		if (fields.has(key)) problems.push(`declares the column "${key}" twice`);
		else fields.set(key, config);
	}
	return { fields, problems };
}

/** The query parameter names of `GET path`, following `$ref` into `components.parameters`. */
function listQueryParams(listPath: string): Set<string> | undefined {
	const operation = OPENAPI.paths[listPath]?.get;
	if (!operation) return undefined;
	const resolve = (node: SchemaNode): SchemaNode =>
		node.$ref ? resolve(OPENAPI.components.parameters[node.$ref.split('/').pop()]) : node;
	return new Set(
		(operation.parameters ?? [])
			.map(resolve)
			.filter((param: SchemaNode) => param.in === 'query')
			.map((param: SchemaNode) => param.name)
	);
}

// ============================================================================
// Comparing decisions with the tab
// ============================================================================

const VERB: Record<Capability, string> = { sort: 'sorts', group: 'groups', filter: 'filters' };

/** Everything wrong with one tab's decisions, as messages naming the tab, column and fix. */
function decisionProblems(tab: string, decided: TabDecisions): string[] {
	const problems: string[] = [];
	const at = `DECISIONS["${tab}"].columns`;

	const { fields: configs, problems: duplicates } = tabFields(tab);
	if (duplicates.length > 0) return duplicates.map((problem) => `${tab}: ${problem}.`);

	const props = dataControlsProps(tab);
	const serverPaginated = props.has('serverPagination');
	const server = dataControlsServerQuery(tab);
	// The tab's order fields: the columns the server can sort and group by.
	const variants = new Set(
		[...configs.values()].filter((config) => 'orderField' in config).map(getFieldKey)
	);
	const columns = columnKeys(tab);

	if (serverPaginated && !decided.listPath) {
		problems.push(
			`${tab}: the tab is server-paginated but DECISIONS["${tab}"] has no listPath. Set it to ` +
				`the path of the GET operation that lists these rows.`
		);
	}
	if (!serverPaginated && decided.listPath) {
		problems.push(`${tab}: listPath is set but the tab is not server-paginated. Remove listPath.`);
	}
	const params = decided.listPath ? listQueryParams(decided.listPath) : undefined;
	if (decided.listPath && !params) {
		problems.push(`${tab}: listPath "${decided.listPath}" has no GET operation in openapi.json.`);
	}

	for (const column of columns) {
		if (!(column in decided.columns)) {
			problems.push(
				`${tab}: column "${column}" has no decision. Add ${at}.${column} with ` +
					`{ sort, group, filter }, each true or the reason it is not offered.`
			);
		}
	}

	if (serverPaginated) {
		for (const column of serverOrderViolations([...configs.values()], true)) {
			problems.push(
				`${tab}: display field "${column}" sets sortable or groupable on a server-paginated ` +
					`list, which would order only the loaded page. Drop the flag, or add an ` +
					`order field for it.`
			);
		}
		for (const column of serverFilterViolations([...configs.values()], true, {
			tags: server.tags,
			fields: server.fields
		})) {
			const fix =
				column === 'tags'
					? 'Pass onQueryChange to DataControls and send its query, which carries tag_ids'
					: "Mark it serverFiltered and give it the endpoint's queryParam";
			problems.push(
				`${tab}: "${column}" is filterable on a server-paginated list but not filtered by the ` +
					`server, so it would narrow only the loaded page. ${fix}, or drop filterable.`
			);
		}
	}

	for (const [column, decision] of Object.entries(decided.columns)) {
		if (!columns.has(column)) {
			problems.push(
				`${tab}: ${at}.${column} names a column the tab no longer declares. Remove the stale decision.`
			);
			continue;
		}
		const config = configs.get(column);
		const offered: Record<Capability, boolean> = {
			sort: config ? isSortableField(config, serverPaginated) : false,
			group: config ? isGroupableField(config, serverPaginated) : false,
			filter: config?.filterable === true
		};

		for (const capability of CAPABILITIES) {
			const choice = decision[capability] as FilterChoice | undefined;
			const where = `${tab}: ${at}.${column}.${capability}`;
			if (choice === undefined) {
				problems.push(`${where} is missing. Set it to true or the reason it is not offered.`);
				continue;
			}
			if (typeof choice === 'string' && !choice.trim()) {
				problems.push(`${where} has an empty reason. Say why "${column}" has no ${capability}.`);
				continue;
			}
			if (typeof choice === 'object' && capability !== 'filter') {
				problems.push(`${where} is { param }, which only a filter takes. Use true or a reason.`);
				continue;
			}
			const yes = choice === true || typeof choice === 'object';

			if (serverPaginated && yes && capability !== 'filter' && !variants.has(column)) {
				problems.push(
					`${where} is true, but "${column}" is not an order field of the tab and a ` +
						`server-paginated list ${VERB[capability]} only by order fields. Add the variant to ` +
						`the backend order-field enum, or record the reason it does not ${capability}.`
				);
			} else if (yes && !offered[capability]) {
				problems.push(
					`${where} is true, but the tab does not offer it. Set the field's flag, or record ` +
						`the reason "${column}" has no ${capability}.`
				);
			} else if (!yes && offered[capability]) {
				problems.push(
					`${tab}: the tab ${VERB[capability]} by "${column}", but ${at}.${column}.${capability} ` +
						`says no ("${choice}"). Drop the flag from the field, or set the decision to true.`
				);
			}

			if (capability !== 'filter' || !yes) continue;
			if (serverPaginated && choice === true) {
				problems.push(
					`${where} is true on a server-paginated tab. Name the query parameter that applies ` +
						`it: { param: '…' } from GET ${decided.listPath}.`
				);
			}
			if (typeof choice === 'object') {
				if (!serverPaginated) {
					problems.push(
						`${where} names param "${choice.param}", but the tab filters client-side. Set it to true.`
					);
				} else if (params && !params.has(choice.param)) {
					problems.push(
						`${where} names param "${choice.param}", which is not a query parameter of GET ` +
							`${decided.listPath}. Use one of: ${[...params].join(', ')}.`
					);
				}
			}
		}
	}
	return problems;
}

/**
 * Everything that would leave a tab's filter or group unreachable in table view, which filters
 * and groups only from column headers.
 */
function headerControlProblems(tab: string): string[] {
	const { fields, problems: duplicates } = tabFields(tab);
	if (duplicates.length > 0) return duplicates.map((problem) => `${tab}: ${problem}.`);

	const props = dataControlsProps(tab);
	const serverPaginated = props.has('serverPagination');
	const staleWired = dataControlsServerQuery(tab).stale;
	const configs = [...fields.values()];
	const problems: string[] = [];

	for (const [column, config] of fields) {
		const filters = config.filterable === true;
		const groups = isGroupableField(config, serverPaginated);
		if (!filters && !groups) continue;

		if (config.display?.hidden === true) {
			problems.push(
				`${tab}: "${column}" ${filters ? 'filters' : 'groups'} but sets display.hidden, so it ` +
					`has no column header to do it from. Move the filter onto a column, or drop the flag.`
			);
			continue;
		}
		const control = columnControls(column, configs, serverPaginated, staleWired);
		if (control?.filter !== filters || control?.group !== groups) {
			problems.push(`${tab}: column "${column}" gets no header control for its flags.`);
		}
	}

	const staleColumns = configs.filter((config) => config.staleFilter === true);
	if (staleWired && staleColumns.length !== 1) {
		problems.push(
			`${tab}: sends staleness to the server, so exactly one field needs staleFilter: true to ` +
				`carry the Stale toggle; found ${staleColumns.length}.`
		);
	}
	if (!staleWired && staleColumns.length > 0) {
		problems.push(
			`${tab}: marks staleFilter without sending staleness to the server (onQueryChange).`
		);
	}
	return problems;
}

describe('entity field controls', () => {
	it('has decisions for every tab that renders DataControls', () => {
		const tabs = dataControlsTabs();
		expect(tabs.length).toBeGreaterThan(10);

		const missing = tabs.filter((tab) => !(tab in DECISIONS));
		expect(
			missing,
			`These tabs render <DataControls> with no DECISIONS entry:\n\n  ${missing.join('\n  ')}\n\n` +
				`Add one per tab with a sort, group and filter decision for every column.`
		).toEqual([]);

		const stale = Object.keys(DECISIONS).filter((tab) => !tabs.includes(tab));
		expect(
			stale,
			`DECISIONS entries whose file is missing or no longer renders <DataControls>:\n\n  ` +
				`${stale.join('\n  ')}\n\nUpdate the path or remove the entry.`
		).toEqual([]);
	});

	for (const [tab, decided] of Object.entries(DECISIONS)) {
		it(`${tab} offers the decided sort, group and filter on every column`, () => {
			expect(fs.existsSync(path.join(FEATURES, tab)), `${tab} does not exist`).toBe(true);
			const problems = decisionProblems(tab, decided);
			expect(problems, problems.join('\n')).toEqual([]);
		});

		it(`${tab} gives every filterable or groupable field a column header control`, () => {
			const problems = headerControlProblems(tab);
			expect(problems, problems.join('\n')).toEqual([]);
		});
	}
});
