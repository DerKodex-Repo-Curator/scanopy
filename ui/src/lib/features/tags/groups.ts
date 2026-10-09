import type { components } from '$lib/api/schema';
import type { IconComponent } from '$lib/shared/utils/types';
import { concepts } from '$lib/shared/stores/metadata';
import { createIconComponent } from '$lib/shared/utils/styling';
import {
	common_application,
	common_status,
	tags_applicationHelp,
	tags_statusHelp,
	tags_tagGroupTooltip
} from '$lib/paraglide/messages';
import type { Tag } from './types/base';

export type TagGroup = components['schemas']['TagGroup'];

/** A group Scanopy defines, as opposed to one an organization names. */
export type SystemTagGroup = Exclude<TagGroup, { type: 'Named' }>;

/**
 * How each system group reads in the UI. Keyed by every system group type, so a system group the
 * backend adds fails the type check until it is described here. Its icon is the concept of the same
 * name.
 */
const SYSTEM_GROUPS: Record<SystemTagGroup['type'], { label: () => string; help: () => string }> = {
	Application: { label: () => common_application(), help: () => tags_applicationHelp() },
	Status: { label: () => common_status(), help: () => tags_statusHelp() }
};

/** Every system group, in the order the picker offers them. */
export const systemTagGroups: SystemTagGroup[] = (
	Object.keys(SYSTEM_GROUPS) as SystemTagGroup['type'][]
).map((type) => ({ type }));

/** The icon a system group is drawn with in the group picker. */
export function systemGroupIcon(group: SystemTagGroup) {
	return concepts.getIconComponent(group.type);
}

/** The help text for a tag group. */
export function tagGroupHelp(group: TagGroup | null | undefined): string | null {
	return group && group.type !== 'Named' ? SYSTEM_GROUPS[group.type].help() : null;
}

/**
 * Whether `name` belongs to a system group, ignoring case. The server reads a named group given
 * such a name as the system group itself, so the picker never offers to create one.
 */
export function isSystemGroupName(name: string): boolean {
	const needle = name.trim().toLowerCase();
	return systemTagGroups.some(
		(group) => group.type.toLowerCase() === needle || tagGroupLabel(group)?.toLowerCase() === needle
	);
}

/** Whether this tag drives the application view. */
export function isApplicationTag(tag: Pick<Tag, 'tag_group'> | null | undefined): boolean {
	return tag?.tag_group?.type === 'Application';
}

/** Whether two tag groups are the same group. `null` is no group, which is never shared. */
export function sameGroup(a: TagGroup | null | undefined, b: TagGroup | null | undefined) {
	if (!a || !b || a.type !== b.type) return false;
	return a.type !== 'Named' || (b.type === 'Named' && a.name === b.name);
}

/** How a tag group is named in the UI. */
export function tagGroupLabel(group: TagGroup | null | undefined): string | null {
	if (!group) return null;
	return group.type === 'Named' ? group.name : SYSTEM_GROUPS[group.type].label();
}

/**
 * A tag chip's hover text: its tag group, then its description, one per line. Empty when the tag
 * has neither, so the chip shows no tooltip.
 */
export function tagTooltip(tag: Pick<Tag, 'tag_group' | 'description'> | null | undefined): string {
	const group = tagGroupLabel(tag?.tag_group);
	const description = tag?.description?.trim();
	return [group ? tags_tagGroupTooltip({ group }) : null, description || null]
		.filter((line): line is string => line !== null)
		.join('\n');
}

/** The named groups in use, from the tags that carry them. A group exists while a tag carries it. */
export function groupNames(tags: Pick<Tag, 'tag_group'>[]): string[] {
	const names = new Set<string>();
	for (const tag of tags) {
		if (tag.tag_group?.type === 'Named') names.add(tag.tag_group.name);
	}
	return [...names].sort((a, b) => a.localeCompare(b));
}

/**
 * The icon a tag is drawn with: the application icon for application tags, the tag's own icon
 * otherwise, or none.
 */
export function tagIcon(tag: Pick<Tag, 'tag_group' | 'icon'> | null | undefined) {
	if (!tag) return null;
	if (isApplicationTag(tag)) return concepts.getIconComponent('Application');
	return tag.icon ? (createIconComponent(tag.icon) as IconComponent) : null;
}

/**
 * The tag ids left after adding `tagId`: the other tags of its tag group drop out, which is what
 * the server does on assignment.
 */
export function withTagAdded(selectedIds: string[], tagId: string, tags: Tag[]): string[] {
	const added = tags.find((t) => t.id === tagId);
	const kept = selectedIds.filter((id) => {
		if (id === tagId) return false;
		const held = tags.find((t) => t.id === id);
		return !sameGroup(held?.tag_group, added?.tag_group);
	});
	return [...kept, tagId];
}
