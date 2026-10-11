import { describe, expect, it } from 'vitest';
import {
	compareTagsByGroup,
	groupNamed,
	groupsFor,
	tagAllowedOn,
	tagTooltip,
	withTagAdded
} from '$lib/features/tags/groups';
import { tagGroupFields, tagGroupIdOf, tagSetsParam } from '$lib/features/tags/groupFields';
import {
	createDefaultTag,
	createDefaultTagGroup,
	type Tag,
	type TagGroup
} from '$lib/features/tags/types/base';
import { getFieldKey, isOrderableField } from '$lib/shared/components/data/types';
import { groupItems } from '$lib/shared/components/data/controls/grouping';
import { sortItems } from '$lib/shared/components/data/controls/sorting';

function group(
	id: string,
	group_type: TagGroup['group_type'] = 'Custom',
	entity_types: TagGroup['entity_types'] = null
): TagGroup {
	return { ...createDefaultTagGroup('org'), id, name: id, group_type, entity_types };
}

const statusGroup = group('Status', 'Status');
const environmentGroup = group('Environment');
const applicationGroup = { ...group('Application', 'Application'), ordered: false };
const backupGroup = group('Backup', 'Custom', ['Host']);
const groups = [applicationGroup, statusGroup, backupGroup, environmentGroup];

function tag(id: string, tagGroup: TagGroup | null = null, position: number | null = null): Tag {
	return {
		...createDefaultTag('org'),
		id,
		name: id,
		tag_group_id: tagGroup?.id ?? null,
		position
	};
}

const planned = tag('planned', statusGroup, 1);
const active = tag('active', statusGroup, 2);
const decommissioned = tag('decommissioned', statusGroup, 3);
const production = tag('production', environmentGroup, 1);
// Application is unordered: its tags carry no position and list by name.
const web = tag('web', applicationGroup);
const db = tag('db', applicationGroup);
const nightly = tag('nightly', backupGroup, 1);
const critical = tag('critical');
const tags = [decommissioned, planned, active, production, web, db, nightly, critical];

describe('withTagAdded', () => {
	it('replaces the held tag of the same group and keeps the rest', () => {
		expect(withTagAdded(['planned', 'production', 'critical'], 'decommissioned', tags)).toEqual([
			'production',
			'critical',
			'decommissioned'
		]);
	});

	it('treats application tags as one group', () => {
		expect(withTagAdded(['web', 'planned'], 'db', tags)).toEqual(['planned', 'db']);
	});

	it('leaves every held tag alone when the added tag is in no group', () => {
		expect(withTagAdded(['planned', 'web'], 'critical', tags)).toEqual([
			'planned',
			'web',
			'critical'
		]);
	});
});

describe('entity-type scope', () => {
	it('offers a limited group only to the types it names', () => {
		expect(groupsFor(groups, 'Host')).toContain(backupGroup);
		expect(groupsFor(groups, 'Service')).not.toContain(backupGroup);
		expect(groupsFor(groups, 'Service')).toContain(statusGroup);
	});

	it('allows a tag in no group anywhere', () => {
		expect(tagAllowedOn(critical, groups, 'Vlan')).toBe(true);
		expect(tagAllowedOn(nightly, groups, 'Vlan')).toBe(false);
		expect(tagAllowedOn(nightly, groups, 'Host')).toBe(true);
	});
});

describe('compareTagsByGroup', () => {
	it('lists groups in the server order, each group in its own order, ungrouped tags last', () => {
		expect([...tags].sort(compareTagsByGroup(groups)).map((t) => t.id)).toEqual([
			'db',
			'web',
			'planned',
			'active',
			'decommissioned',
			'nightly',
			'production',
			'critical'
		]);
	});
});

describe('groupNamed', () => {
	it('finds a group by name or by a system group label, in any case', () => {
		expect(groupNamed(groups, ' environment ')).toBe(environmentGroup);
		expect(groupNamed(groups, 'STATUS')).toBe(statusGroup);
		expect(groupNamed(groups, 'Tier')).toBeNull();
	});
});

describe('tagTooltip', () => {
	it('puts the group first and the description under it', () => {
		const lines = tagTooltip({ ...planned, description: '  Not deployed yet ' }, groups).split(
			'\n'
		);
		expect(lines).toHaveLength(2);
		expect(lines[0]).toContain('Status');
		expect(lines[1]).toBe('Not deployed yet');
	});

	it('is empty for a tag with neither, so the chip shows no tooltip', () => {
		expect(tagTooltip({ ...critical, description: '   ' }, groups)).toBe('');
		expect(tagTooltip(null, groups)).toBe('');
	});
});

describe('tag group table columns', () => {
	type Row = { id: string; tags: string[] };
	const rows: Row[] = [
		{ id: 'retired', tags: ['decommissioned'] },
		{ id: 'untagged', tags: [] },
		{ id: 'new', tags: ['planned', 'production'] },
		{ id: 'live', tags: ['active'] }
	];
	const fieldsFor = (serverPaginated: boolean) =>
		tagGroupFields<Row>({
			groups,
			tags,
			entityType: 'Service',
			getItemTags: (row) => row.tags,
			serverPaginated
		});

	it('adds one hidden column per group that applies to the entity type', () => {
		const fields = fieldsFor(false);
		expect(fields.map((f) => tagGroupIdOf(getFieldKey(f)))).toEqual([
			'Application',
			'Status',
			'Environment'
		]);
		expect(fields.every((f) => f.display?.hiddenByDefault)).toBe(true);
	});

	it("sorts and groups rows in the group's tag order, rows without a tag last", () => {
		const status = fieldsFor(false)[1];
		const sorted = (direction: 'asc' | 'desc') =>
			sortItems(rows, [status], { field: getFieldKey(status), direction }).map((r) => r.id);
		expect(sorted('asc')).toEqual(['new', 'live', 'retired', 'untagged']);
		expect(sorted('desc')).toEqual(['retired', 'live', 'new', 'untagged']);
		const grouped = groupItems(
			rows,
			[status],
			getFieldKey(status),
			{ ungrouped: '—', yes: 'yes', no: 'no' },
			false
		);
		expect([...grouped.keys()]).toEqual(['planned', 'active', 'decommissioned', '—']);
	});

	it('is server-orderable on a server-paged list, keyed the way the server groups', () => {
		const status = fieldsFor(true)[1];
		expect(isOrderableField(status)).toBe(true);
		expect(status.serverFiltered).toBe(true);
		expect(status.getGroupValue!(rows[2])).toBe('0000000001:planned:planned');
		expect(status.getGroupValue!(rows[1])).toBeNull();
		// An unordered group's tags key at position 0, so the name orders them.
		const application = fieldsFor(true)[0];
		expect(application.getGroupValue!({ id: 'x', tags: ['web'] })).toBe('0000000000:web:web');
	});
});

describe('tagSetsParam', () => {
	it('sends one set per filtered group and drops a group with nothing checked', () => {
		expect(
			tagSetsParam({ Status: ['planned', 'active'], Environment: [], Backup: ['nightly'] }, tags)
		).toEqual(['planned,active', 'nightly']);
		expect(tagSetsParam({ Status: [] }, tags)).toBeUndefined();
	});
});
